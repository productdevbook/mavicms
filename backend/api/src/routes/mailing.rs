//! Lists, subscribers, templates and campaigns.
//!
//! Everything a site's own people touch is here. The sending itself is in
//! `mailing`, run by the worker; these routes only ever put a campaign in the
//! queue and read back how far it got.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::Administrator,
    entities::{
        campaign, campaign_event, campaign_list, email_log, mail_list, mail_template, subscriber,
        subscriber_list,
    },
    error::{AppError, AppResult},
    mailing::{self, BLOCKED, CONFIRMED, ENABLED, UNCONFIRMED, UNSUBSCRIBED},
    slug::slugify_or,
    tenants::{Hosting, Resolved, Site},
};

/// The most rows one page of anything here returns.
const PAGE: u64 = 200;
/// The largest import this will read. A list of a hundred thousand addresses
/// is about 4 MB.
pub const MAX_IMPORT: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct ListResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub opt_in: String,
    pub public: bool,
    pub confirmed: u64,
    pub unconfirmed: u64,
    pub unsubscribed: u64,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveListRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// "single" or "double". Double sends a confirmation first, which is what
    /// keeps a list clean and what several countries ask for.
    #[serde(default = "double")]
    pub opt_in: String,
    #[serde(default = "yes")]
    pub public: bool,
}

fn double() -> String {
    "double".to_string()
}
fn yes() -> bool {
    true
}

async fn counted(
    db: &sea_orm::DatabaseConnection,
    list: mail_list::Model,
) -> AppResult<ListResponse> {
    let of = |status: &'static str| {
        subscriber_list::Entity::find()
            .filter(subscriber_list::Column::ListId.eq(list.id))
            .filter(subscriber_list::Column::Status.eq(status))
            .count(db)
    };

    Ok(ListResponse {
        id: list.id.to_string(),
        confirmed: of(CONFIRMED).await?,
        unconfirmed: of(UNCONFIRMED).await?,
        unsubscribed: of(UNSUBSCRIBED).await?,
        name: list.name,
        slug: list.slug,
        description: list.description,
        opt_in: list.opt_in,
        public: list.public,
        created_at: list.created_at.to_rfc3339(),
    })
}

/// The site's lists.
#[utoipa::path(
    // Answers without a credential.
    security(()),get, path = "/mail/lists", tag = "mail",
    responses((status = 200, description = "Lists", body = Vec<ListResponse>)))]
pub async fn list_lists(Site(state): Site) -> AppResult<Json<Vec<ListResponse>>> {
    let db = state.db();
    let mut out = Vec::new();
    for row in mail_list::Entity::find()
        .order_by(mail_list::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?
    {
        out.push(counted(db, row).await?);
    }
    Ok(Json(out))
}

/// Make one.
#[utoipa::path(post, path = "/mail/lists", tag = "mail", request_body = SaveListRequest,
    responses((status = 201, description = "Made", body = ListResponse)))]
pub async fn create_list(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<SaveListRequest>,
) -> AppResult<(StatusCode, Json<ListResponse>)> {
    let db = state.db();
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("the list needs a name".to_string()));
    }

    let made = mail_list::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(slugify_or(&name, "list")),
        name: Set(name),
        description: Set(payload.description.trim().to_string()),
        opt_in: Set(if payload.opt_in == "single" {
            "single".to_string()
        } else {
            "double".to_string()
        }),
        public: Set(payload.public),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok((StatusCode::CREATED, Json(counted(db, made).await?)))
}

/// Change one.
#[utoipa::path(put, path = "/mail/lists/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "List id")), request_body = SaveListRequest,
    responses((status = 200, description = "Changed", body = ListResponse)))]
pub async fn update_list(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveListRequest>,
) -> AppResult<Json<ListResponse>> {
    let db = state.db();
    let existing = mail_list::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("list {id}")))?;

    let mut changed: mail_list::ActiveModel = existing.into();
    changed.name = Set(payload.name.trim().to_string());
    changed.description = Set(payload.description.trim().to_string());
    changed.opt_in = Set(if payload.opt_in == "single" {
        "single".to_string()
    } else {
        "double".to_string()
    });
    changed.public = Set(payload.public);

    Ok(Json(counted(db, changed.update(db).await?).await?))
}

/// Remove one, and everybody's place on it. The people stay.
#[utoipa::path(delete, path = "/mail/lists/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "List id")),
    responses((status = 204, description = "Removed")))]
pub async fn delete_list(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();

    subscriber_list::Entity::delete_many()
        .filter(subscriber_list::Column::ListId.eq(id))
        .exec(db)
        .await?;
    campaign_list::Entity::delete_many()
        .filter(campaign_list::Column::ListId.eq(id))
        .exec(db)
        .await?;

    let gone = mail_list::Entity::delete_by_id(id).exec(db).await?;
    if gone.rows_affected == 0 {
        return Err(AppError::NotFound(format!("list {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Subscribers
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct SubscriberResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub status: String,
    #[schema(value_type = Object)]
    pub attributes: serde_json::Value,
    /// The lists they are on, and where they stand on each.
    pub lists: Vec<Membership>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Membership {
    pub list_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SubscriberQuery {
    /// Part of an address or a name.
    pub q: Option<String>,
    pub list: Option<Uuid>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveSubscriberRequest {
    pub email: String,
    #[serde(default)]
    pub name: String,
    /// Lists to put them on, replacing what they were on.
    #[serde(default)]
    pub lists: Vec<Uuid>,
    #[serde(default)]
    pub attributes: serde_json::Value,
    /// "enabled" or "blocked".
    #[serde(default)]
    pub status: Option<String>,
}

async fn with_lists(
    db: &sea_orm::DatabaseConnection,
    person: subscriber::Model,
) -> AppResult<SubscriberResponse> {
    let places = subscriber_list::Entity::find()
        .filter(subscriber_list::Column::SubscriberId.eq(person.id))
        .all(db)
        .await?;

    Ok(SubscriberResponse {
        id: person.id.to_string(),
        attributes: serde_json::from_str(&person.attributes).unwrap_or(serde_json::json!({})),
        lists: places
            .into_iter()
            .map(|row| Membership {
                list_id: row.list_id.to_string(),
                status: row.status,
            })
            .collect(),
        email: person.email,
        name: person.name,
        status: person.status,
        created_at: person.created_at.to_rfc3339(),
    })
}

/// The people this site knows.
#[utoipa::path(get, path = "/mail/subscribers", tag = "mail", params(SubscriberQuery),
    responses((status = 200, description = "Subscribers", body = Vec<SubscriberResponse>)))]
pub async fn list_subscribers(
    Site(state): Site,
    Query(query): Query<SubscriberQuery>,
) -> AppResult<Json<Vec<SubscriberResponse>>> {
    let db = state.db();
    let mut find = subscriber::Entity::find();

    if let Some(text) = query.q.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let like = format!("%{}%", text.to_lowercase());
        find = find.filter(
            sea_orm::Condition::any()
                .add(subscriber::Column::Email.contains(&like))
                .add(subscriber::Column::Name.contains(&like)),
        );
    }

    if let Some(list) = query.list {
        let ids: Vec<Uuid> = subscriber_list::Entity::find()
            .filter(subscriber_list::Column::ListId.eq(list))
            .all(db)
            .await?
            .into_iter()
            .map(|row| row.subscriber_id)
            .collect();
        find = find.filter(subscriber::Column::Id.is_in(ids));
    }

    let rows = find
        .order_by(subscriber::Column::CreatedAt, Order::Desc)
        .limit(query.limit.unwrap_or(PAGE).min(PAGE))
        .offset(query.offset.unwrap_or(0))
        .all(db)
        .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(with_lists(db, row).await?);
    }
    Ok(Json(out))
}

/// Puts somebody on the lists given, leaving alone any list not mentioned that
/// they had already unsubscribed from.
async fn place_on_lists(
    db: &sea_orm::DatabaseConnection,
    who: Uuid,
    lists: &[Uuid],
    status: &str,
) -> AppResult<()> {
    for list in lists {
        let existing = subscriber_list::Entity::find_by_id((who, *list))
            .one(db)
            .await?;

        match existing {
            // Somebody who unsubscribed stays unsubscribed. Adding them back
            // by uploading the same file again is how a list gets a
            // reputation for not listening.
            Some(row) if row.status == UNSUBSCRIBED => continue,
            Some(row) if row.status == status => continue,
            Some(row) => {
                let mut changed: subscriber_list::ActiveModel = row.into();
                changed.status = Set(status.to_string());
                changed.update(db).await?;
            }
            None => {
                subscriber_list::ActiveModel {
                    subscriber_id: Set(who),
                    list_id: Set(*list),
                    status: Set(status.to_string()),
                    created_at: Set(Utc::now().fixed_offset()),
                }
                .insert(db)
                .await?;
            }
        }
    }
    Ok(())
}

/// Add somebody by hand.
#[utoipa::path(post, path = "/mail/subscribers", tag = "mail", request_body = SaveSubscriberRequest,
    responses((status = 201, description = "Added", body = SubscriberResponse)))]
pub async fn create_subscriber(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<SaveSubscriberRequest>,
) -> AppResult<(StatusCode, Json<SubscriberResponse>)> {
    let db = state.db();
    let email = mailing::clean_email(&payload.email)?;

    let person = match subscriber::Entity::find()
        .filter(subscriber::Column::Email.eq(&email))
        .one(db)
        .await?
    {
        Some(found) => found,
        None => {
            subscriber::ActiveModel {
                id: Set(Uuid::now_v7()),
                email: Set(email),
                name: Set(payload.name.trim().to_string()),
                status: Set(ENABLED.to_string()),
                attributes: Set(payload.attributes.to_string()),
                created_at: Set(Utc::now().fixed_offset()),
            }
            .insert(db)
            .await?
        }
    };

    // Added from the panel by somebody who says they have permission, so
    // confirmed rather than pending: the double opt-in is for the form on the
    // website, where nobody vouches for the address.
    place_on_lists(db, person.id, &payload.lists, CONFIRMED).await?;

    Ok((StatusCode::CREATED, Json(with_lists(db, person).await?)))
}

/// Change somebody.
#[utoipa::path(put, path = "/mail/subscribers/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Subscriber id")),
    request_body = SaveSubscriberRequest,
    responses((status = 200, description = "Changed", body = SubscriberResponse)))]
pub async fn update_subscriber(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveSubscriberRequest>,
) -> AppResult<Json<SubscriberResponse>> {
    let db = state.db();
    let existing = subscriber::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("subscriber {id}")))?;

    let mut changed: subscriber::ActiveModel = existing.into();
    changed.email = Set(mailing::clean_email(&payload.email)?);
    changed.name = Set(payload.name.trim().to_string());
    changed.attributes = Set(payload.attributes.to_string());
    if let Some(status) = payload.status.as_deref() {
        changed.status = Set(if status == BLOCKED {
            BLOCKED.to_string()
        } else {
            ENABLED.to_string()
        });
    }
    let saved = changed.update(db).await?;

    place_on_lists(db, saved.id, &payload.lists, CONFIRMED).await?;
    Ok(Json(with_lists(db, saved).await?))
}

/// Remove somebody entirely — what to do when they ask to be forgotten.
#[utoipa::path(delete, path = "/mail/subscribers/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Subscriber id")),
    responses((status = 204, description = "Removed")))]
pub async fn delete_subscriber(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();

    subscriber_list::Entity::delete_many()
        .filter(subscriber_list::Column::SubscriberId.eq(id))
        .exec(db)
        .await?;
    campaign_event::Entity::delete_many()
        .filter(campaign_event::Column::SubscriberId.eq(id))
        .exec(db)
        .await?;

    let gone = subscriber::Entity::delete_by_id(id).exec(db).await?;
    if gone.rows_affected == 0 {
        return Err(AppError::NotFound(format!("subscriber {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Import and export
// ---------------------------------------------------------------------------

/// One row of a CSV, taking quotes seriously enough for a spreadsheet's
/// output: a quoted field may hold commas, newlines and doubled quotes.
fn csv_row(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    out.push(field);
    out.into_iter()
        .map(|field| unescape_formula(&field))
        .collect()
}

/// Characters a spreadsheet reads as the start of a formula.
const FORMULA_START: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// One field, quoted for CSV and defused for a spreadsheet.
///
/// The names and fields in here come from a form on the open web — anybody
/// may sign up, and nobody vouches for what they type. Excel and its
/// relatives treat a cell beginning with = + - or @ as a formula, so an
/// address book exported by an administrator and opened by them is a way to
/// run something on their machine. Quoting alone does not help: the
/// spreadsheet strips the quotes as CSV syntax and evaluates what is left.
///
/// A leading apostrophe is what stops it, and `unescape_formula` takes it off
/// again on the way back in, so the file still round-trips through this
/// program unchanged.
fn csv_field(value: &str) -> String {
    let value = if value.starts_with(FORMULA_START) {
        format!("'{value}")
    } else {
        value.to_string()
    };

    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// Undoes what `csv_field` added, and only that.
///
/// The apostrophe comes off only when a formula character is behind it, so a
/// name that genuinely begins with one — "'Tis", an Irish surname typed with
/// a leading quote — keeps it.
fn unescape_formula(value: &str) -> String {
    match value.strip_prefix('\'') {
        Some(rest) if rest.starts_with(FORMULA_START) => rest.to_string(),
        _ => value.to_string(),
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ImportQuery {
    /// The list to put them on.
    pub list: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ImportReport {
    pub added: u64,
    pub updated: u64,
    /// Lines that were not an address, with the reason.
    pub skipped: Vec<String>,
}

/// Take a CSV of people onto a list.
///
/// The first line may be a header; a line whose first field is not an address
/// is skipped and reported rather than failing the whole file, because a
/// spreadsheet export nearly always has something odd in it and losing four
/// thousand good rows over one bad one helps nobody.
#[utoipa::path(post, path = "/mail/subscribers/import", tag = "mail", params(ImportQuery),
    request_body(content = String, description = "CSV: email, name, then any columns as fields"),
    responses((status = 200, description = "What came of it", body = ImportReport)))]
pub async fn import_subscribers(
    _admin: Administrator,
    Site(state): Site,
    Query(query): Query<ImportQuery>,
    body: String,
) -> AppResult<Json<ImportReport>> {
    let db = state.db();
    mail_list::Entity::find_by_id(query.list)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Validation("no such list".to_string()))?;

    let mut report = ImportReport {
        added: 0,
        updated: 0,
        skipped: Vec::new(),
    };

    let mut headers: Option<Vec<String>> = None;
    for (number, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = csv_row(line);

        // A header only if its first field is not an address — a file with no
        // header should not lose its first person.
        if headers.is_none() && !crate::email::looks_like_an_address(fields[0].trim()) {
            headers = Some(fields.iter().map(|f| f.trim().to_lowercase()).collect());
            continue;
        }

        let email = match mailing::clean_email(&fields[0]) {
            Ok(email) => email,
            Err(_) => {
                if report.skipped.len() < 50 {
                    report
                        .skipped
                        .push(format!("line {}: {}", number + 1, fields[0].trim()));
                }
                continue;
            }
        };

        let name = fields
            .get(1)
            .map(|f| f.trim().to_string())
            .unwrap_or_default();

        // Every column past the second becomes a field, named by the header
        // if there was one.
        let mut attributes = serde_json::Map::new();
        for (index, value) in fields.iter().enumerate().skip(2) {
            let key = headers
                .as_ref()
                .and_then(|h| h.get(index).cloned())
                .unwrap_or_else(|| format!("field{index}"));
            if !key.is_empty() && !value.trim().is_empty() {
                attributes.insert(key, serde_json::Value::String(value.trim().to_string()));
            }
        }

        let existing = subscriber::Entity::find()
            .filter(subscriber::Column::Email.eq(&email))
            .one(db)
            .await?;

        let person = match existing {
            Some(found) => {
                let mut changed: subscriber::ActiveModel = found.into();
                if !name.is_empty() {
                    changed.name = Set(name);
                }
                if !attributes.is_empty() {
                    changed.attributes = Set(serde_json::Value::Object(attributes).to_string());
                }
                report.updated += 1;
                changed.update(db).await?
            }
            None => {
                report.added += 1;
                subscriber::ActiveModel {
                    id: Set(Uuid::now_v7()),
                    email: Set(email),
                    name: Set(name),
                    status: Set(ENABLED.to_string()),
                    attributes: Set(serde_json::Value::Object(attributes).to_string()),
                    created_at: Set(Utc::now().fixed_offset()),
                }
                .insert(db)
                .await?
            }
        };

        place_on_lists(db, person.id, &[query.list], CONFIRMED).await?;
    }

    Ok(Json(report))
}

/// Give the whole list back as a CSV, so nobody is locked in.
#[utoipa::path(get, path = "/mail/subscribers/export", tag = "mail", params(SubscriberQuery),
    responses((status = 200, description = "CSV")))]
pub async fn export_subscribers(
    _admin: Administrator,
    Site(state): Site,
    Query(query): Query<SubscriberQuery>,
) -> AppResult<Response> {
    let db = state.db();

    let ids: Option<Vec<Uuid>> = match query.list {
        Some(list) => Some(
            subscriber_list::Entity::find()
                .filter(subscriber_list::Column::ListId.eq(list))
                .all(db)
                .await?
                .into_iter()
                .map(|row| row.subscriber_id)
                .collect(),
        ),
        None => None,
    };

    let mut find = subscriber::Entity::find();
    if let Some(ids) = ids {
        find = find.filter(subscriber::Column::Id.is_in(ids));
    }
    let rows = find
        .order_by(subscriber::Column::CreatedAt, Order::Asc)
        .all(db)
        .await?;

    let mut csv = String::from("email,name,status,attributes\n");
    for row in rows {
        csv.push_str(&format!(
            "{},{},{},{}\n",
            csv_field(&row.email),
            csv_field(&row.name),
            csv_field(&row.status),
            csv_field(&row.attributes)
        ));
    }

    Ok((
        [
            ("content-type", "text/csv; charset=utf-8"),
            (
                "content-disposition",
                "attachment; filename=\"subscribers.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct TemplateResponse {
    pub id: String,
    pub name: String,
    pub subject: String,
    pub body: String,
    pub is_default: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub subject: String,
    /// The letterhead. `{{ content }}` is where a campaign goes.
    pub body: String,
    #[serde(default)]
    pub is_default: bool,
}

fn template_row(row: mail_template::Model) -> TemplateResponse {
    TemplateResponse {
        id: row.id.to_string(),
        name: row.name,
        subject: row.subject,
        body: row.body,
        is_default: row.is_default,
        created_at: row.created_at.to_rfc3339(),
    }
}

/// Exactly one template is the default, so making one the default unmakes the
/// others. Two defaults is a question with no answer.
async fn only_default(db: &sea_orm::DatabaseConnection, keep: Uuid) -> AppResult<()> {
    for other in mail_template::Entity::find()
        .filter(mail_template::Column::IsDefault.eq(true))
        .filter(mail_template::Column::Id.ne(keep))
        .all(db)
        .await?
    {
        let mut changed: mail_template::ActiveModel = other.into();
        changed.is_default = Set(false);
        changed.update(db).await?;
    }
    Ok(())
}

#[utoipa::path(get, path = "/mail/templates", tag = "mail",
    responses((status = 200, description = "Templates", body = Vec<TemplateResponse>)))]
pub async fn list_templates(Site(state): Site) -> AppResult<Json<Vec<TemplateResponse>>> {
    Ok(Json(
        mail_template::Entity::find()
            .order_by(mail_template::Column::CreatedAt, Order::Desc)
            .all(state.db())
            .await?
            .into_iter()
            .map(template_row)
            .collect(),
    ))
}

#[utoipa::path(post, path = "/mail/templates", tag = "mail", request_body = SaveTemplateRequest,
    responses((status = 201, description = "Made", body = TemplateResponse)))]
pub async fn create_template(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<SaveTemplateRequest>,
) -> AppResult<(StatusCode, Json<TemplateResponse>)> {
    let db = state.db();
    let made = mail_template::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(payload.name.trim().to_string()),
        subject: Set(payload.subject.trim().to_string()),
        body: Set(payload.body),
        is_default: Set(payload.is_default),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    if made.is_default {
        only_default(db, made.id).await?;
    }
    Ok((StatusCode::CREATED, Json(template_row(made))))
}

#[utoipa::path(put, path = "/mail/templates/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Template id")), request_body = SaveTemplateRequest,
    responses((status = 200, description = "Changed", body = TemplateResponse)))]
pub async fn update_template(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveTemplateRequest>,
) -> AppResult<Json<TemplateResponse>> {
    let db = state.db();
    let existing = mail_template::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("template {id}")))?;

    let mut changed: mail_template::ActiveModel = existing.into();
    changed.name = Set(payload.name.trim().to_string());
    changed.subject = Set(payload.subject.trim().to_string());
    changed.body = Set(payload.body);
    changed.is_default = Set(payload.is_default);
    let saved = changed.update(db).await?;

    if saved.is_default {
        only_default(db, saved.id).await?;
    }
    Ok(Json(template_row(saved)))
}

#[utoipa::path(delete, path = "/mail/templates/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Template id")),
    responses((status = 204, description = "Removed")))]
pub async fn delete_template(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();

    // A campaign that used it keeps its own body and loses the letterhead,
    // rather than failing to send at all.
    for row in campaign::Entity::find()
        .filter(campaign::Column::TemplateId.eq(id))
        .all(db)
        .await?
    {
        let mut changed: campaign::ActiveModel = row.into();
        changed.template_id = Set(None);
        changed.update(db).await?;
    }

    let gone = mail_template::Entity::delete_by_id(id).exec(db).await?;
    if gone.rows_affected == 0 {
        return Err(AppError::NotFound(format!("template {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Campaigns
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct CampaignResponse {
    pub id: String,
    pub name: String,
    pub subject: String,
    pub body: String,
    pub template_id: Option<String>,
    pub from_address: String,
    pub status: String,
    pub lists: Vec<String>,
    pub send_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub to_send: i32,
    pub sent: i32,
    pub failed: i32,
    /// How many opened it, and how many followed a link. Counted once per
    /// person, because ten opens by one reader is one reader.
    pub opened: u64,
    pub clicked: u64,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveCampaignRequest {
    pub name: String,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub template_id: Option<Uuid>,
    /// Which of the site's addresses it goes out as. Empty for the default.
    #[serde(default)]
    pub from_address: String,
    #[serde(default)]
    pub lists: Vec<Uuid>,
    /// When to send it. Absent means when somebody presses send.
    #[serde(default)]
    pub send_at: Option<String>,
}

async fn campaign_row(
    db: &sea_orm::DatabaseConnection,
    row: campaign::Model,
) -> AppResult<CampaignResponse> {
    let lists = campaign_list::Entity::find()
        .filter(campaign_list::Column::CampaignId.eq(row.id))
        .all(db)
        .await?;

    let unique = |kind: &'static str| async move {
        let rows = campaign_event::Entity::find()
            .filter(campaign_event::Column::CampaignId.eq(row.id))
            .filter(campaign_event::Column::Kind.eq(kind))
            .all(db)
            .await?;
        Ok::<u64, AppError>(
            rows.into_iter()
                .map(|e| e.subscriber_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len() as u64,
        )
    };

    Ok(CampaignResponse {
        id: row.id.to_string(),
        opened: unique(mailing::OPEN).await?,
        clicked: unique(mailing::CLICK).await?,
        lists: lists.into_iter().map(|l| l.list_id.to_string()).collect(),
        name: row.name,
        subject: row.subject,
        body: row.body,
        template_id: row.template_id.map(|id| id.to_string()),
        from_address: row.from_address,
        status: row.status,
        send_at: row.send_at.map(|t| t.to_rfc3339()),
        started_at: row.started_at.map(|t| t.to_rfc3339()),
        finished_at: row.finished_at.map(|t| t.to_rfc3339()),
        to_send: row.to_send,
        sent: row.sent,
        failed: row.failed,
        created_at: row.created_at.to_rfc3339(),
    })
}

#[utoipa::path(get, path = "/mail/campaigns", tag = "mail",
    responses((status = 200, description = "Campaigns", body = Vec<CampaignResponse>)))]
pub async fn list_campaigns(Site(state): Site) -> AppResult<Json<Vec<CampaignResponse>>> {
    let db = state.db();
    let mut out = Vec::new();
    for row in campaign::Entity::find()
        .order_by(campaign::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?
    {
        out.push(campaign_row(db, row).await?);
    }
    Ok(Json(out))
}

#[utoipa::path(get, path = "/mail/campaigns/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses((status = 200, description = "The campaign", body = CampaignResponse)))]
pub async fn get_campaign(
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CampaignResponse>> {
    let db = state.db();
    let row = campaign::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("campaign {id}")))?;
    Ok(Json(campaign_row(db, row).await?))
}

async fn set_lists(
    db: &sea_orm::DatabaseConnection,
    campaign_id: Uuid,
    lists: &[Uuid],
) -> AppResult<()> {
    campaign_list::Entity::delete_many()
        .filter(campaign_list::Column::CampaignId.eq(campaign_id))
        .exec(db)
        .await?;

    for list in lists {
        campaign_list::ActiveModel {
            campaign_id: Set(campaign_id),
            list_id: Set(*list),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

#[utoipa::path(post, path = "/mail/campaigns", tag = "mail", request_body = SaveCampaignRequest,
    responses((status = 201, description = "Made", body = CampaignResponse)))]
pub async fn create_campaign(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<SaveCampaignRequest>,
) -> AppResult<(StatusCode, Json<CampaignResponse>)> {
    let db = state.db();
    let send_at = parse_when(payload.send_at.as_deref())?;

    let made = campaign::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(payload.name.trim().to_string()),
        subject: Set(payload.subject.trim().to_string()),
        body: Set(payload.body),
        template_id: Set(payload.template_id),
        from_address: Set(payload.from_address.trim().to_string()),
        status: Set(mailing::DRAFT.to_string()),
        send_at: Set(send_at),
        started_at: Set(None),
        finished_at: Set(None),
        to_send: Set(0),
        sent: Set(0),
        failed: Set(0),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    set_lists(db, made.id, &payload.lists).await?;
    Ok((StatusCode::CREATED, Json(campaign_row(db, made).await?)))
}

fn parse_when(value: Option<&str>) -> AppResult<Option<chrono::DateTime<chrono::FixedOffset>>> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(text) => chrono::DateTime::parse_from_rfc3339(text)
            .map(Some)
            .map_err(|_| AppError::Validation("that is not a time".to_string())),
    }
}

#[utoipa::path(put, path = "/mail/campaigns/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")), request_body = SaveCampaignRequest,
    responses((status = 200, description = "Changed", body = CampaignResponse)))]
pub async fn update_campaign(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveCampaignRequest>,
) -> AppResult<Json<CampaignResponse>> {
    let db = state.db();
    let existing = campaign::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("campaign {id}")))?;

    // Editing what is already going out would mean two people getting two
    // different letters with the same name on them.
    if existing.status == mailing::RUNNING || existing.status == mailing::FINISHED {
        return Err(AppError::Conflict(
            "this campaign has already gone out; copy it instead".to_string(),
        ));
    }

    let send_at = parse_when(payload.send_at.as_deref())?;
    let mut changed: campaign::ActiveModel = existing.into();
    changed.name = Set(payload.name.trim().to_string());
    changed.subject = Set(payload.subject.trim().to_string());
    changed.body = Set(payload.body);
    changed.template_id = Set(payload.template_id);
    changed.from_address = Set(payload.from_address.trim().to_string());
    changed.send_at = Set(send_at);
    let saved = changed.update(db).await?;

    set_lists(db, saved.id, &payload.lists).await?;
    Ok(Json(campaign_row(db, saved).await?))
}

#[utoipa::path(delete, path = "/mail/campaigns/{id}", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses((status = 204, description = "Removed")))]
pub async fn delete_campaign(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();

    campaign_list::Entity::delete_many()
        .filter(campaign_list::Column::CampaignId.eq(id))
        .exec(db)
        .await?;
    campaign_event::Entity::delete_many()
        .filter(campaign_event::Column::CampaignId.eq(id))
        .exec(db)
        .await?;

    let gone = campaign::Entity::delete_by_id(id).exec(db).await?;
    if gone.rows_affected == 0 {
        return Err(AppError::NotFound(format!("campaign {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Starting, pausing and stopping
// ---------------------------------------------------------------------------

/// Put it in the queue. The worker picks it up; nothing is sent in this
/// request, because a campaign is thousands of messages and a browser will
/// not wait for them.
#[utoipa::path(post, path = "/mail/campaigns/{id}/send", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses(
        (status = 202, description = "Queued", body = CampaignResponse),
        (status = 409, description = "Not in a state to send", body = crate::error::ErrorBody),
    ))]
pub async fn send_campaign(
    _admin: Administrator,
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<CampaignResponse>)> {
    let Resolved::Tenant(tenant) = &resolved else {
        return Err(AppError::Validation(
            "campaigns belong to a hosted site".to_string(),
        ));
    };

    let db = state.db();
    let found = campaign::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("campaign {id}")))?;

    if found.status == mailing::RUNNING || found.status == mailing::FINISHED {
        return Err(AppError::Conflict(
            "this campaign is already going out".to_string(),
        ));
    }

    // Refused now rather than discovered by the worker an hour later, when
    // nobody is looking at the screen that would have said so.
    let settings = crate::plugins::load::<crate::email::EmailConfig>(
        db,
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await?;
    if !settings.is_some_and(|stored| stored.enabled) {
        return Err(AppError::Validation(
            "mail is not switched on for this site".to_string(),
        ));
    }

    let audience = mailing::audience(db, id).await?;
    if audience == 0 {
        return Err(AppError::Validation(
            "nobody confirmed is on the lists this campaign is for".to_string(),
        ));
    }

    // A time in the future means scheduled, not running: nothing has gone
    // out yet, and a page that says "going out" while nothing is would be
    // lying for however long the wait is.
    let later = found
        .send_at
        .filter(|when| *when > Utc::now().fixed_offset());

    let mut changed: campaign::ActiveModel = found.into();
    changed.status = Set(if later.is_some() {
        mailing::SCHEDULED.to_string()
    } else {
        mailing::RUNNING.to_string()
    });
    changed.started_at = Set(later.is_none().then(|| Utc::now().fixed_offset()));
    changed.finished_at = Set(None);
    changed.to_send = Set(audience as i32);
    let saved = changed.update(db).await?;

    mailing::enqueue(hosting.registry()?.control(), tenant.id, id, later).await?;

    Ok((StatusCode::ACCEPTED, Json(campaign_row(db, saved).await?)))
}

/// Stop it where it is. What has gone has gone; the rest waits.
#[utoipa::path(post, path = "/mail/campaigns/{id}/pause", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses((status = 200, description = "Paused", body = CampaignResponse)))]
pub async fn pause_campaign(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CampaignResponse>> {
    Ok(Json(set_status(&state, id, mailing::PAUSED).await?))
}

/// Stop it for good.
#[utoipa::path(post, path = "/mail/campaigns/{id}/cancel", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses((status = 200, description = "Cancelled", body = CampaignResponse)))]
pub async fn cancel_campaign(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<Json<CampaignResponse>> {
    Ok(Json(set_status(&state, id, mailing::CANCELLED).await?))
}

async fn set_status(
    state: &crate::state::AppState,
    id: Uuid,
    status: &str,
) -> AppResult<CampaignResponse> {
    let db = state.db();
    let found = campaign::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("campaign {id}")))?;

    let mut changed: campaign::ActiveModel = found.into();
    changed.status = Set(status.to_string());
    // The worker re-reads the campaign between batches, so this stops it
    // within one batch rather than at the end.
    let saved = changed.update(db).await?;
    campaign_row(db, saved).await
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TestSendRequest {
    pub to: String,
}

/// Send it to one address, to see what it looks like.
///
/// Rendered exactly as it would be for a real subscriber, placeholders and
/// unsubscribe link included — a preview that skips the rendering is a
/// preview of something else.
#[utoipa::path(post, path = "/mail/campaigns/{id}/test", tag = "mail",
    params(("id" = Uuid, Path, description = "Campaign id")), request_body = TestSendRequest,
    responses((status = 204, description = "Sent")))]
pub async fn test_campaign(
    _admin: Administrator,
    axum::Extension(resolved): axum::Extension<Resolved>,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<TestSendRequest>,
) -> AppResult<StatusCode> {
    let db = state.db();
    let found = campaign::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("campaign {id}")))?;

    let settings = crate::plugins::load::<crate::email::EmailConfig>(
        db,
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await?
    .ok_or_else(|| AppError::Validation("mail is not set up yet".to_string()))?;

    let template = match found.template_id {
        Some(template) => mail_template::Entity::find_by_id(template).one(db).await?,
        None => None,
    };

    let host = match &resolved {
        Resolved::Tenant(tenant) => tenant.host.clone(),
        Resolved::Host => "localhost".to_string(),
    };
    let site_url = format!("https://{host}");

    // A stand-in, never written to the subscriber table: a test send should
    // not add whoever is testing to the list.
    let pretend = subscriber::Model {
        id: Uuid::now_v7(),
        email: mailing::clean_email(&payload.to)?,
        name: "…".to_string(),
        status: ENABLED.to_string(),
        attributes: "{}".to_string(),
        created_at: Utc::now().fixed_offset(),
    };

    let message = mailing::render_for(
        &state.secrets,
        &format!("{site_url}/api"),
        &site_url,
        &found,
        template.as_ref(),
        &pretend,
    )?;

    let outcome = crate::email::send(
        &settings.config,
        crate::email::Message {
            to: &pretend.email,
            subject: &format!("[test] {}", message.subject),
            text: &message.text,
            html: Some(&message.html),
            from: None,
            unsubscribe_url: None,
            tags: &[],
        },
    )
    .await;

    mailing::log_message(db, &pretend.email, &message.subject, &outcome).await;
    outcome?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// The log
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct LogEntry {
    pub id: String,
    pub to_address: String,
    pub subject: String,
    pub status: String,
    pub detail: String,
    pub created_at: String,
}

/// Every message this site tried to send. The answer to "did it go?".
#[utoipa::path(get, path = "/mail/log", tag = "mail",
    responses((status = 200, description = "Recent messages", body = Vec<LogEntry>)))]
pub async fn list_log(Site(state): Site) -> AppResult<Json<Vec<LogEntry>>> {
    Ok(Json(
        email_log::Entity::find()
            .order_by(email_log::Column::CreatedAt, Order::Desc)
            .limit(PAGE)
            .all(state.db())
            .await?
            .into_iter()
            .map(|row| LogEntry {
                id: row.id.to_string(),
                to_address: row.to_address,
                subject: row.subject,
                status: row.status,
                detail: row.detail,
                created_at: row.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

// ---------------------------------------------------------------------------
// The open half: what a visitor and a mail client reach, with no account
// ---------------------------------------------------------------------------

/// Join a list from the site's own pages.
///
/// A public list only, and on a double opt-in list nothing is sent until the
/// address answers. That is the difference between a mailing list and a list
/// of people somebody typed in.
#[utoipa::path(
    // Answers without a credential.
    security(()),post, path = "/mail/subscribe", tag = "mail",
    request_body = crate::mailing::SubscribeRequest,
    responses(
        (status = 202, description = "Taken; a confirmation may have been sent"),
        (status = 400, description = "Not an address, or no such list", body = crate::error::ErrorBody),
    ))]
pub async fn subscribe(
    axum::Extension(resolved): axum::Extension<Resolved>,
    Site(state): Site,
    Json(payload): Json<crate::mailing::SubscribeRequest>,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;
    let email = mailing::clean_email(&payload.email)?;

    let wanted = mail_list::Entity::find()
        .filter(
            mail_list::Column::Slug.is_in(payload.lists.iter().map(|s| s.trim().to_lowercase())),
        )
        .filter(mail_list::Column::Public.eq(true))
        .all(db)
        .await?;

    if wanted.is_empty() {
        return Err(AppError::Validation(
            "no list of that name takes sign-ups".to_string(),
        ));
    }

    let person = match subscriber::Entity::find()
        .filter(subscriber::Column::Email.eq(&email))
        .one(db)
        .await?
    {
        Some(found) => found,
        None => {
            subscriber::ActiveModel {
                id: Set(Uuid::now_v7()),
                email: Set(email),
                name: Set(payload.name.trim().chars().take(200).collect()),
                status: Set(ENABLED.to_string()),
                attributes: Set(match &payload.attributes {
                    serde_json::Value::Object(_) => payload.attributes.to_string(),
                    _ => "{}".to_string(),
                }),
                created_at: Set(Utc::now().fixed_offset()),
            }
            .insert(db)
            .await?
        }
    };

    let host = match &resolved {
        Resolved::Tenant(tenant) => tenant.host.clone(),
        Resolved::Host => "localhost".to_string(),
    };
    let api_base = format!("https://{host}/api");

    for list in wanted {
        let double = list.opt_in != "single";
        place_on_lists(
            db,
            person.id,
            &[list.id],
            if double { UNCONFIRMED } else { CONFIRMED },
        )
        .await?;

        if !double {
            continue;
        }

        // Only where they are not already on it: somebody pressing the button
        // twice should not get two letters.
        let standing = subscriber_list::Entity::find_by_id((person.id, list.id))
            .one(db)
            .await?
            .map(|row| row.status);
        if standing.as_deref() != Some(UNCONFIRMED) {
            continue;
        }

        let url = format!(
            "{api_base}/mail/confirm/{}",
            mailing::link_token(&state.secrets, "confirm", person.id, Some(list.id))?
        );
        let (subject, html) = mailing::confirmation(&list.name, &url);

        if let Ok(Some(settings)) = crate::plugins::load::<crate::email::EmailConfig>(
            db,
            &state.secrets,
            crate::plugins::EMAIL_PLUGIN,
        )
        .await
            && settings.enabled
        {
            let outcome = crate::email::send(
                &settings.config,
                crate::email::Message {
                    to: &person.email,
                    subject: &subject,
                    text: &mailing::as_text(&html),
                    html: Some(&html),
                    from: None,
                    unsubscribe_url: None,
                    tags: &[],
                },
            )
            .await;
            mailing::log_message(db, &person.email, &subject, &outcome).await;
        }
    }

    // Accepted, not created, and it says nothing about whether the address
    // was already known: a sign-up form that answers differently for an
    // address already on the list is a way to ask whether somebody is on it.
    Ok(StatusCode::ACCEPTED)
}

fn page(title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex\"><title>{title}</title>\
         <style>body{{margin:0;min-height:100svh;display:flex;align-items:center;\
         justify-content:center;font:16px/1.6 system-ui,sans-serif;padding:24px;\
         text-align:center;color-scheme:light dark}}</style></head>\
         <body><div>{body}</div></body></html>"
    );
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

/// The link in a confirmation message.
#[utoipa::path(
    // Answers without a credential.
    security(()),get, path = "/mail/confirm/{token}", tag = "mail",
    params(("token" = String, Path, description = "From the confirmation message")),
    responses((status = 200, description = "A page saying what happened")))]
pub async fn confirm(Site(state): Site, Path(token): Path<String>) -> AppResult<Response> {
    let db = state.db_or_unavailable()?;

    let Some((who, Some(list))) = mailing::read_token(&state.secrets, "confirm", &token) else {
        return Ok(page(
            "Link expired",
            "<p>That link cannot be read. It may have been broken in half by a mail program.</p>",
        ));
    };

    if let Some(row) = subscriber_list::Entity::find_by_id((who, list))
        .one(db)
        .await?
    {
        // Somebody who left and then followed an old confirmation link stays
        // left. The last thing they said is the thing that counts.
        if row.status == UNCONFIRMED {
            let mut changed: subscriber_list::ActiveModel = row.into();
            changed.status = Set(CONFIRMED.to_string());
            changed.update(db).await?;
        }
    }

    Ok(page("Confirmed", "<p>Thank you — you are on the list.</p>"))
}

/// The unsubscribe link, and the button on the page it shows.
///
/// A GET shows a page rather than acting: mail clients and security scanners
/// follow every link in a message, and a link that unsubscribes on sight
/// removes people who never touched it.
///
/// Nothing from the address reaches the page. The form carries no action, so
/// the browser posts back to the URL it was served from — which means the
/// token never has to be written into the HTML, and a token shaped like
/// `"><script>` has nowhere to land. Escaping would also have worked; not
/// interpolating cannot be got wrong later.
#[utoipa::path(
    // Answers without a credential.
    security(()),get, path = "/mail/unsubscribe/{token}", tag = "mail",
    params(("token" = String, Path, description = "From the message")),
    responses((status = 200, description = "A page with a button on it")))]
pub async fn unsubscribe_page(Site(state): Site, Path(token): Path<String>) -> Response {
    let readable = state
        .db_or_unavailable()
        .is_ok()
        .then(|| mailing::read_token(&state.secrets, "unsubscribe", &token))
        .flatten()
        .is_some();

    if !readable {
        return page("Link expired", "<p>That link cannot be read.</p>");
    }

    page(
        "Unsubscribe",
        "<p>Stop receiving these messages?</p>\
         <form method=\"post\">\
         <button style=\"font:inherit;padding:10px 18px;cursor:pointer\">Yes, unsubscribe</button>\
         </form>",
    )
}

/// Actually leave.
///
/// Also what a mail client posts to when it offers its own unsubscribe button
/// through the List-Unsubscribe header, which is why it takes a POST with no
/// body and no session.
#[utoipa::path(
    // Answers without a credential.
    security(()),post, path = "/mail/unsubscribe/{token}", tag = "mail",
    params(("token" = String, Path, description = "From the message")),
    responses((status = 200, description = "A page saying it is done")))]
pub async fn unsubscribe(Site(state): Site, Path(token): Path<String>) -> AppResult<Response> {
    let db = state.db_or_unavailable()?;

    let Some((who, list)) = mailing::read_token(&state.secrets, "unsubscribe", &token) else {
        return Ok(page("Link expired", "<p>That link cannot be read.</p>"));
    };

    let mut places =
        subscriber_list::Entity::find().filter(subscriber_list::Column::SubscriberId.eq(who));
    // A link made for one list takes them off that one; the ones in campaigns
    // carry no list and take them off everything, which is what somebody
    // pressing "unsubscribe" almost always means.
    if let Some(list) = list {
        places = places.filter(subscriber_list::Column::ListId.eq(list));
    }

    for row in places.all(db).await? {
        let mut changed: subscriber_list::ActiveModel = row.into();
        changed.status = Set(UNSUBSCRIBED.to_string());
        changed.update(db).await?;
    }

    Ok(page(
        "Unsubscribed",
        "<p>Done. You will not hear from this list again.</p>",
    ))
}

/// Where Amazon posts everything it knows.
///
/// Open, because SNS has no account here and never will. What stands in for
/// one is the address itself: the token in the path is made from the site's
/// own key and cannot be guessed, which is the same thing the unsubscribe
/// links rely on. The topic is checked as well, so a token that did leak is
/// still only good for the topic this site made.
///
/// Full SNS signature verification would be stronger and is the reason to
/// come back here: it needs the certificate fetched, cached and checked, and
/// the two cheap checks below already mean an attacker has to know a secret
/// this site never publishes.
#[utoipa::path(post, path = "/mail/events/{token}", tag = "mail",
    params(("token" = String, Path, description = "From the plugin settings")),
    responses((status = 204, description = "Taken")))]
pub async fn receive_events(
    Site(state): Site,
    Path(token): Path<String>,
    body: String,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;

    // Anything at all wrong looks the same from outside: a wrong token must
    // not be distinguishable from a well-formed message about another site.
    let refused = || AppError::NotFound("that address".to_string());

    let settings = crate::plugins::load::<crate::email::EmailConfig>(
        db,
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await?
    .ok_or_else(refused)?;

    if settings.config.events_token.is_empty() || settings.config.events_token != token {
        return Err(refused());
    }

    let envelope: mailing::SnsEnvelope = serde_json::from_str(&body).map_err(|_| refused())?;

    // Once a topic is known, every message has to name it — including one that
    // names no topic at all, which would otherwise be the way around this.
    if !settings.config.events_topic_arn.is_empty()
        && envelope.topic_arn != settings.config.events_topic_arn
    {
        return Err(refused());
    }

    match envelope.kind.as_str() {
        // SNS proves a subscription by posting a link here and waiting for it
        // to be fetched. Fetching it is the confirmation.
        // Only a real SNS address is fetched. Answering whatever URL a
        // request hands over would make this endpoint a way to have the
        // server fetch anything, from inside the cluster.
        "SubscriptionConfirmation" if mailing::is_an_sns_url(&envelope.subscribe_url) => {
            // Redirects are refused as well: the address checked above is only
            // the first one, and following a hop chosen by whoever answers
            // would hand the check straight back.
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|_| refused())?;
            let _ = client.get(&envelope.subscribe_url).send().await;
            tracing::info!("confirmed an SNS subscription");
        }
        "Notification" => {
            mailing::record_ses_event(db, &envelope.message).await?;
        }
        // UnsubscribeConfirmation and anything else: nothing to do, and
        // answering anyway keeps SNS from retrying.
        _ => {}
    }

    Ok(StatusCode::NO_CONTENT)
}

/// The pixel at the bottom of a message.
#[utoipa::path(
    // Answers without a credential.
    security(()),get, path = "/mail/open/{token}", tag = "mail",
    params(("token" = String, Path, description = "Signed, from the message")),
    responses((status = 200, description = "A one-pixel image")))]
pub async fn open_pixel(Site(state): Site, Path(token): Path<String>) -> Response {
    // Whatever happens, a picture comes back. A broken image in somebody's
    // letter is worse than a count that is one short.
    let image = (
        [
            ("content-type", "image/gif"),
            ("cache-control", "no-store, must-revalidate"),
        ],
        mailing::PIXEL,
    );

    let Ok(db) = state.db_or_unavailable() else {
        return image.into_response();
    };

    // The token names its campaign, so a pixel from one message cannot be
    // replayed as an open of another.
    if let Some((campaign_id, who)) = read_open(&state.secrets, &token) {
        let _ = campaign_event::ActiveModel {
            id: Set(Uuid::now_v7()),
            campaign_id: Set(campaign_id),
            subscriber_id: Set(who),
            kind: Set(mailing::OPEN.to_string()),
            detail: Set(String::new()),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(db)
        .await;
    }

    image.into_response()
}

/// An open token names its campaign in the purpose, so one cannot be replayed
/// against another campaign.
fn read_open(secrets: &crate::crypto::SecretBox, token: &str) -> Option<(Uuid, Uuid)> {
    let restored = token.replace('-', "+").replace('_', "/");
    let plain = secrets.decrypt(&restored).ok()?;

    let mut parts = plain.splitn(4, ':');
    if parts.next()? != "open" {
        return None;
    }
    let campaign = Uuid::parse_str(parts.next()?).ok()?;
    let who = Uuid::parse_str(parts.next()?).ok()?;
    Some((campaign, who))
}

/// A link in a message: counted, then followed.
#[utoipa::path(
    // Answers without a credential.
    security(()),get, path = "/mail/click/{token}", tag = "mail",
    params(("token" = String, Path, description = "Signed, from the message")),
    responses((status = 303, description = "On to where it was going")))]
pub async fn click(Site(state): Site, Path(token): Path<String>) -> Response {
    let Some((campaign_id, who, url)) = mailing::read_click(&state.secrets, &token) else {
        // Nothing signed by this site, so there is nowhere to send them. A
        // redirect to whatever was in the link is exactly the open redirect
        // this design exists to avoid.
        return page("Link expired", "<p>That link cannot be read.</p>");
    };

    if let Ok(db) = state.db_or_unavailable() {
        let _ = campaign_event::ActiveModel {
            id: Set(Uuid::now_v7()),
            campaign_id: Set(campaign_id),
            subscriber_id: Set(who),
            kind: Set(mailing::CLICK.to_string()),
            detail: Set(url.chars().take(2000).collect()),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(db)
        .await;
    }

    Redirect::to(&url).into_response()
}

#[cfg(test)]
mod csv_tests {
    use super::{csv_field, csv_row};

    #[test]
    fn ordinary_values_are_left_alone() {
        assert_eq!(csv_field("Ada Lovelace"), "Ada Lovelace");
        assert_eq!(csv_field("ada@example.com"), "ada@example.com");
    }

    #[test]
    fn commas_and_quotes_are_still_quoted() {
        assert_eq!(csv_field("Hopper, Grace"), "\"Hopper, Grace\"");
        assert_eq!(csv_field("she said \"hi\""), "\"she said \"\"hi\"\"\"");
    }

    #[test]
    fn a_formula_cannot_reach_a_spreadsheet() {
        // Anybody may put this in the name field of a public sign-up form.
        for attack in [
            "=1+1",
            "+1+1",
            "-1+1",
            "@SUM(A1)",
            "=HYPERLINK(\"http://example.test\",\"click\")",
            "=cmd|'/c calc'!A1",
        ] {
            let written = csv_field(attack);
            assert!(
                written.starts_with('\'') || written.starts_with("\"'"),
                "{attack} was written as {written}"
            );
        }
    }

    #[test]
    fn the_file_still_reads_back_as_what_went_in() {
        for original in ["=1+1", "Hopper, Grace", "Ada", "@home", "-5"] {
            let line = csv_field(original);
            assert_eq!(csv_row(&line)[0], original, "{original} did not round-trip");
        }
    }

    #[test]
    fn an_apostrophe_somebody_meant_is_kept() {
        // Only the one this program added comes off, and it is known by what
        // follows it.
        assert_eq!(csv_row("'Tis")[0], "'Tis");
        assert_eq!(csv_row("O'Brien")[0], "O'Brien");
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventEntry {
    pub id: String,
    pub kind: String,
    pub address: String,
    pub campaign_id: Option<String>,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventSummary {
    /// One count per kind, over the window asked for.
    pub counts: std::collections::BTreeMap<String, u64>,
    pub recent: Vec<EventEntry>,
}

/// What Amazon has said about this site's mail.
///
/// Beside the deliverability figures rather than instead of them: those are
/// Amazon's own aggregate, and these are the individual events, with the
/// address each one is about. When somebody says "my colleague did not get
/// it", this is the page that answers.
#[utoipa::path(get, path = "/mail/events", tag = "mail",
    responses((status = 200, description = "Events and their counts", body = EventSummary)))]
pub async fn list_events(Site(state): Site) -> AppResult<Json<EventSummary>> {
    use crate::entities::mail_event;

    let db = state.db();
    let rows = mail_event::Entity::find()
        .order_by(mail_event::Column::CreatedAt, Order::Desc)
        .limit(PAGE)
        .all(db)
        .await?;

    let mut counts: std::collections::BTreeMap<String, u64> = Default::default();
    for row in mail_event::Entity::find().all(db).await? {
        *counts.entry(row.kind).or_default() += 1;
    }

    Ok(Json(EventSummary {
        counts,
        recent: rows
            .into_iter()
            .map(|row| EventEntry {
                id: row.id.to_string(),
                kind: row.kind,
                address: row.address,
                campaign_id: row.campaign_id.map(|id| id.to_string()),
                detail: row.detail,
                created_at: row.created_at.to_rfc3339(),
            })
            .collect(),
    }))
}
