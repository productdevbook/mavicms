//! Forms, and what people send through them.
//!
//! A form here is a definition and a mailbox, not a page. Whoever builds the
//! site draws the form themselves — in their own markup, their own app, their
//! own checkout — and posts the answers to one address. This keeps the shape
//! of the answers, checks them against it, and shows what came in.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, FromQueryResult, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::Administrator,
    dto::forms::{
        FormField, FormResponse, FormSchema, SaveFormRequest, SubmissionRequest,
        SubmissionResponse, clean_fields, clean_submission,
    },
    entities::{form, form_submission},
    error::{AppError, AppResult},
    slug::slugify_or,
    tenants::Site,
};

/// The largest submission this will read.
///
/// Generous for a contact form and small enough that an open address cannot
/// be used to fill a disk one request at a time.
pub const MAX_SUBMISSION_BYTES: usize = 64 * 1024;

/// The most submissions one page of the panel asks for.
const MAX_PAGE: u64 = 200;

fn fields_of(model: &form::Model) -> AppResult<Vec<FormField>> {
    serde_json::from_str(&model.fields)
        .map_err(|err| AppError::Internal(format!("stored form fields are unreadable: {err}")))
}

fn row(model: form::Model, submissions: u64, unseen: u64) -> AppResult<FormResponse> {
    Ok(FormResponse {
        id: model.id.to_string(),
        fields: fields_of(&model)?,
        name: model.name,
        slug: model.slug,
        description: model.description,
        active: model.active,
        notify: model.notify,
        submissions,
        unseen,
        created_at: model.created_at.to_rfc3339(),
        updated_at: model.updated_at.to_rfc3339(),
    })
}

#[derive(FromQueryResult)]
struct Tally {
    form_id: Uuid,
    total: i64,
}

/// How many submissions each form holds, in one query rather than one per row.
async fn tally(
    db: &sea_orm::DatabaseConnection,
    unseen_only: bool,
) -> AppResult<HashMap<Uuid, u64>> {
    let mut find = form_submission::Entity::find()
        .select_only()
        .column(form_submission::Column::FormId)
        .column_as(form_submission::Column::Id.count(), "total")
        .group_by(form_submission::Column::FormId);
    if unseen_only {
        find = find.filter(form_submission::Column::Seen.eq(false));
    }

    Ok(find
        .into_model::<Tally>()
        .all(db)
        .await?
        .into_iter()
        .map(|row| (row.form_id, row.total.max(0) as u64))
        .collect())
}

/// The forms this site has.
#[utoipa::path(
    get,
    path = "/forms",
    tag = "forms",
    responses((status = 200, description = "The site's forms", body = Vec<FormResponse>))
)]
pub async fn list_forms(Site(state): Site) -> AppResult<Json<Vec<FormResponse>>> {
    let db = state.db();
    let forms = form::Entity::find()
        .order_by(form::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?;

    let totals = tally(db, false).await?;
    let unseen = tally(db, true).await?;

    forms
        .into_iter()
        .map(|model| {
            let id = model.id;
            row(
                model,
                totals.get(&id).copied().unwrap_or(0),
                unseen.get(&id).copied().unwrap_or(0),
            )
        })
        .collect::<AppResult<Vec<_>>>()
        .map(Json)
}

async fn slug_taken(
    db: &sea_orm::DatabaseConnection,
    slug: &str,
    except: Option<Uuid>,
) -> AppResult<bool> {
    Ok(form::Entity::find()
        .filter(form::Column::Slug.eq(slug))
        .one(db)
        .await?
        .is_some_and(|found| Some(found.id) != except))
}

/// Make a form.
#[utoipa::path(
    post,
    path = "/forms",
    tag = "forms",
    request_body = SaveFormRequest,
    responses(
        (status = 201, description = "Made", body = FormResponse),
        (status = 409, description = "That address is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn create_form(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<SaveFormRequest>,
) -> AppResult<(StatusCode, Json<FormResponse>)> {
    Ok((
        StatusCode::CREATED,
        Json(create(state.db(), payload).await?),
    ))
}

/// Making a form, as the endpoint does it. Separate from the handler because
/// an assistant makes them too, and two implementations of "which field names
/// are usable" would eventually disagree.
pub async fn create(
    db: &sea_orm::DatabaseConnection,
    payload: SaveFormRequest,
) -> AppResult<FormResponse> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("the form needs a name".to_string()));
    }
    let fields = clean_fields(payload.fields)?;
    let slug = slugify_or(payload.slug.as_deref().unwrap_or(&name), "form");

    if slug_taken(db, &slug, None).await? {
        return Err(AppError::Conflict(format!(
            "another form already answers at \"{slug}\""
        )));
    }

    let now = Utc::now().fixed_offset();
    let created = form::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(name),
        slug: Set(slug),
        description: Set(payload.description.trim().to_string()),
        fields: Set(serde_json::to_string(&fields)
            .map_err(|err| AppError::Internal(format!("could not store the fields: {err}")))?),
        active: Set(payload.active),
        notify: Set(notify_address(&payload.notify)?),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;

    row(created, 0, 0)
}

/// How many submissions one form holds, and how many are still unread.
async fn counts(db: &sea_orm::DatabaseConnection, id: Uuid) -> AppResult<(u64, u64)> {
    let all = form_submission::Entity::find().filter(form_submission::Column::FormId.eq(id));
    Ok((
        all.clone().count(db).await?,
        all.filter(form_submission::Column::Seen.eq(false))
            .count(db)
            .await?,
    ))
}

/// The address a form tells, checked before it is stored rather than when
/// something finally comes in — a typo found at save time is a typo; one
/// found on the first submission is a lost enquiry.
fn notify_address(given: &str) -> AppResult<String> {
    let given = given.trim();
    if given.is_empty() {
        return Ok(String::new());
    }
    if !crate::email::looks_like_an_address(given) {
        return Err(AppError::Validation(format!(
            "\"{given}\" is not an email address"
        )));
    }
    Ok(given.to_string())
}

async fn find_form(db: &sea_orm::DatabaseConnection, id: Uuid) -> AppResult<form::Model> {
    form::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("form {id}")))
}

/// One form.
///
/// Any account on the site, a build included: this is the shape a form
/// accepts, not what anybody sent through it, and a build that draws the
/// site's contact form from this definition is a reader worth allowing.
#[utoipa::path(
    get,
    path = "/forms/{id}",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id")),
    responses(
        (status = 200, description = "The form", body = FormResponse),
        (status = 404, description = "No such form", body = crate::error::ErrorBody),
    )
)]
pub async fn get_form(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<Json<FormResponse>> {
    let db = state.db();
    let model = find_form(db, id).await?;
    let (submissions, unseen) = counts(db, id).await?;

    Ok(Json(row(model, submissions, unseen)?))
}

/// Change a form.
///
/// Fields can be added, renamed and removed. What has already come in is left
/// exactly as it was received — a submission is a record of what somebody
/// sent, and rewriting it to match a form they never saw would be a lie.
#[utoipa::path(
    put,
    path = "/forms/{id}",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id")),
    request_body = SaveFormRequest,
    responses(
        (status = 200, description = "Changed", body = FormResponse),
        (status = 409, description = "That address is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn update_form(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveFormRequest>,
) -> AppResult<Json<FormResponse>> {
    let db = state.db();
    let existing = find_form(db, id).await?;

    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("the form needs a name".to_string()));
    }
    let fields = clean_fields(payload.fields)?;
    let slug = match payload.slug.as_deref().map(str::trim) {
        Some(given) if !given.is_empty() => slugify_or(given, "form"),
        _ => existing.slug.clone(),
    };

    if slug != existing.slug && slug_taken(db, &slug, Some(id)).await? {
        return Err(AppError::Conflict(format!(
            "another form already answers at \"{slug}\""
        )));
    }

    let mut changed: form::ActiveModel = existing.into();
    changed.name = Set(name);
    changed.slug = Set(slug);
    changed.description = Set(payload.description.trim().to_string());
    changed.fields = Set(serde_json::to_string(&fields)
        .map_err(|err| AppError::Internal(format!("could not store the fields: {err}")))?);
    changed.active = Set(payload.active);
    changed.notify = Set(notify_address(&payload.notify)?);
    changed.updated_at = Set(Utc::now().fixed_offset());
    let saved = changed.update(db).await?;
    let (submissions, unseen) = counts(db, id).await?;

    Ok(Json(row(saved, submissions, unseen)?))
}

/// Remove a form, and everything sent through it.
#[utoipa::path(
    delete,
    path = "/forms/{id}",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id")),
    responses((status = 204, description = "Removed"))
)]
pub async fn delete_form(
    _admin: Administrator,
    Site(state): Site,
    user: axum::Extension<crate::entities::user::Model>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();

    let existing = form::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("form {id}")))?;

    // What people sent goes with the form. Deleting a contact form is the one
    // that would hurt most to get wrong: the enquiries are somebody else's
    // words and there is nowhere else they exist.
    let submissions = form_submission::Entity::find()
        .filter(form_submission::Column::FormId.eq(id))
        .all(db)
        .await?;

    crate::trash::keep(
        db,
        crate::trash::FORM,
        id,
        &existing.name,
        serde_json::json!({ "form": existing, "submissions": submissions }),
        &user.username,
    )
    .await?;

    // Not left to the foreign key: SQLite enforces one only when the
    // connection asked it to, so the rows would otherwise outlive the form on
    // one database and not on the others.
    form_submission::Entity::delete_many()
        .filter(form_submission::Column::FormId.eq(id))
        .exec(db)
        .await?;

    form::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SubmissionsQuery {
    /// How many to return, newest first.
    pub limit: Option<u64>,
    /// How many to skip.
    pub offset: Option<u64>,
}

/// What has been sent through a form.
///
/// Administrators only, unlike the form's own definition above. What a form
/// asks for is a shape and reading it lets a build draw the form; what people
/// answered is their name, their address and their telephone number, and the
/// build token that reads this site's posts is handed to whatever builds the
/// pages. It has no business with any of that.
#[utoipa::path(
    get,
    path = "/forms/{id}/submissions",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id"), SubmissionsQuery),
    responses((status = 200, description = "Submissions, newest first", body = Vec<SubmissionResponse>))
)]
pub async fn list_submissions(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Query(query): Query<SubmissionsQuery>,
) -> AppResult<Json<Vec<SubmissionResponse>>> {
    let db = state.db();
    find_form(db, id).await?;

    let rows = form_submission::Entity::find()
        .filter(form_submission::Column::FormId.eq(id))
        .order_by(form_submission::Column::CreatedAt, Order::Desc)
        .limit(query.limit.unwrap_or(MAX_PAGE).min(MAX_PAGE))
        .offset(query.offset.unwrap_or(0))
        .all(db)
        .await?;

    Ok(Json(
        rows.into_iter()
            .map(|model| SubmissionResponse {
                id: model.id.to_string(),
                form_id: model.form_id.to_string(),
                // Stored as this server wrote it. Unreadable JSON would mean
                // the row was edited outside the CMS; show it rather than
                // fail the whole page.
                data: serde_json::from_str(&model.data)
                    .unwrap_or(serde_json::Value::String(model.data)),
                seen: model.seen,
                created_at: model.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// Mark everything a form holds as read.
///
/// A build token cannot reach this today — it is a POST, and a build may only
/// read — but the gate is stated here rather than left to that, because "the
/// method happens to be POST" is not the reason this is refused.
#[utoipa::path(
    post,
    path = "/forms/{id}/seen",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id")),
    responses((status = 204, description = "Marked"))
)]
pub async fn mark_seen(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();
    find_form(db, id).await?;

    form_submission::Entity::update_many()
        .col_expr(
            form_submission::Column::Seen,
            sea_orm::sea_query::Expr::value(true),
        )
        .filter(form_submission::Column::FormId.eq(id))
        .filter(form_submission::Column::Seen.eq(false))
        .exec(db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Throw one submission away.
#[utoipa::path(
    delete,
    path = "/forms/{id}/submissions/{submission_id}",
    tag = "forms",
    params(
        ("id" = Uuid, Path, description = "Form id"),
        ("submission_id" = Uuid, Path, description = "Submission id"),
    ),
    responses((status = 204, description = "Removed"))
)]
pub async fn delete_submission(
    _admin: Administrator,
    Site(state): Site,
    user: axum::Extension<crate::entities::user::Model>,
    Path((id, submission_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let db = state.db();

    // Both halves of the path have to agree, or one form's address could be
    // used to delete another form's post.
    let existing = form_submission::Entity::find_by_id(submission_id)
        .filter(form_submission::Column::FormId.eq(id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("submission {submission_id}")))?;

    crate::trash::keep(
        db,
        crate::trash::FORM_SUBMISSION,
        submission_id,
        &crate::routes::forms::describe(&existing),
        serde_json::json!({ "submission": existing }),
        &user.username,
    )
    .await?;

    form_submission::Entity::delete_by_id(submission_id)
        .exec(db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A line of a submission, for the bin's list. Whoever is looking for what
/// they deleted knows the name or the address that was in it, not its id.
pub fn describe(submission: &form_submission::Model) -> String {
    let data: serde_json::Value =
        serde_json::from_str(&submission.data).unwrap_or(serde_json::Value::Null);

    let pick = |name: &str| {
        data.get(name)
            .and_then(|value| value.as_str())
            .map(str::to_string)
    };

    ["name", "email", "subject", "title", "message"]
        .into_iter()
        .filter_map(pick)
        .find(|value| !value.trim().is_empty())
        .unwrap_or_else(|| submission.created_at.to_rfc3339())
}

/// What a form accepts.
///
/// The companion to the address below, and the reason it can be used without
/// being told anything else: which fields a form takes is not something an
/// API description can hold, because a form is made in the panel long after
/// the description was written. So the address describes itself. Ask this,
/// get the field names, their types and whether they are required, then post
/// to `/forms/{slug}/submit`.
#[utoipa::path(
    get,
    path = "/forms/{slug}/schema",
    tag = "forms",
    params(("slug" = String, Path, description = "The form's address")),
    responses(
        (status = 200, description = "The fields this form accepts", body = FormSchema),
        (status = 404, description = "No form answers there", body = crate::error::ErrorBody),
    ),
    // Answers without a credential.
    security(())
)]
pub async fn form_schema(
    Site(state): Site,
    Path(slug): Path<String>,
) -> AppResult<Json<FormSchema>> {
    let db = state.db_or_unavailable()?;
    let form = active_form(db, &slug).await?;

    Ok(Json(FormSchema {
        fields: fields_of(&form)?,
        name: form.name,
        slug: form.slug,
    }))
}

/// The form answering at an address, if one is taking answers.
///
/// A switched-off form is missing rather than refused: whether one exists is
/// not something an open address should tell people.
async fn active_form(db: &sea_orm::DatabaseConnection, slug: &str) -> AppResult<form::Model> {
    form::Entity::find()
        .filter(form::Column::Slug.eq(slug))
        .filter(form::Column::Active.eq(true))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("form {slug}")))
}

/// Send something through a form.
///
/// Open, because the visitor filling in a contact form has no account here.
/// What it accepts is exactly what `/forms/{slug}/schema` lists and nothing
/// else — a key the form does not define is refused rather than kept.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    post,
    path = "/forms/{slug}/submit",
    tag = "forms",
    params(("slug" = String, Path, description = "The form's address")),
    request_body = SubmissionRequest,
    responses(
        (status = 201, description = "Received"),
        (status = 400, description = "It does not match the form", body = crate::error::ErrorBody),
        (status = 404, description = "No form answers there", body = crate::error::ErrorBody),
    )
)]
pub async fn submit_form(
    Site(state): Site,
    Path(slug): Path<String>,
    Json(payload): Json<SubmissionRequest>,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;
    let form = active_form(db, &slug).await?;

    let data = clean_submission(&fields_of(&form)?, payload.0)?;

    form_submission::ActiveModel {
        id: Set(Uuid::now_v7()),
        form_id: Set(form.id),
        data: Set(serde_json::to_string(&data)
            .map_err(|err| AppError::Internal(format!("could not store the answers: {err}")))?),
        seen: Set(false),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    notify(&state, &form, &data).await;

    // After the row and never in front of it, for the same reason the message
    // above is: the visitor is owed their answer being kept, and a flow that
    // cannot start is not their problem. Queued, not run — the form answers
    // before anybody's mail server is involved.
    if let Err(err) = crate::flows::fire(
        db,
        crate::flows::trigger::FORM_SUBMITTED,
        serde_json::json!({
            "form": form.slug,
            "form_id": form.id.to_string(),
            "fields": data,
        }),
    )
    .await
    {
        tracing::warn!(form = %form.slug, error = %err, "could not start the flows for this form");
    }

    Ok(StatusCode::CREATED)
}

/// Tells whoever the form names that something came in.
///
/// After the row is written and never in front of it: a visitor filling in a
/// contact form is owed their answer being kept, and mail that Amazon is slow
/// about, or refuses, is not their problem. Every failure here is logged and
/// swallowed for the same reason.
async fn notify(state: &crate::state::AppState, form: &form::Model, data: &serde_json::Value) {
    if form.notify.trim().is_empty() {
        return;
    }
    let Ok(db) = state.db_or_unavailable() else {
        return;
    };

    // Whichever account this site sends by — its own, or the server's with
    // this site named as its own Amazon tenant.
    let post = match crate::outbound::how(state).await {
        Ok(post) => post,
        Err(err) => {
            tracing::warn!(form = %form.slug, error = %err, "a form names an address but cannot send");
            return;
        }
    };
    // A form that stops emailing because the day's messages ran out is worth
    // a line in the log: the submission is kept either way, and somebody has
    // to be able to find out why the message never came.
    if let Err(err) = crate::outbound::may_send(&post, db, 1).await {
        tracing::warn!(form = %form.slug, error = %err, "not sending the notification");
        return;
    }

    let subject = format!("{}: a new submission", form.name);
    let body = readable(form, data);

    if let Err(err) = crate::email::send(
        &post.config,
        crate::email::Message {
            to: &form.notify,
            subject: &subject,
            text: &body,
            html: None,
            from: None,
            unsubscribe_url: None,
            tags: &[],
        },
    )
    .await
    {
        tracing::warn!(form = %form.slug, error = %err, "could not send the notification");
    }
}

/// The submission as a person reads it: the label somebody gave the field,
/// then what was answered. The stored JSON is the record; this is the letter.
fn readable(form: &form::Model, data: &serde_json::Value) -> String {
    let fields = fields_of(form).unwrap_or_default();
    let mut out = format!("{}\n\n", form.name);

    for field in &fields {
        let value = match data.get(&field.name) {
            None | Some(serde_json::Value::Null) => "—".to_string(),
            Some(serde_json::Value::String(text)) => text.clone(),
            Some(serde_json::Value::Bool(flag)) => if *flag { "yes" } else { "no" }.to_string(),
            Some(other) => other.to_string(),
        };
        out.push_str(&format!("{}: {}\n", field.label, value));
    }

    out
}
