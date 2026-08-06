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
        FormField, FormResponse, SaveFormRequest, SubmissionRequest, SubmissionResponse,
        clean_fields, clean_submission,
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
    let db = state.db();
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
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await?;

    Ok((StatusCode::CREATED, Json(row(created, 0, 0)?)))
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

async fn find_form(db: &sea_orm::DatabaseConnection, id: Uuid) -> AppResult<form::Model> {
    form::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("form {id}")))
}

/// One form.
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
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();

    // Not left to the foreign key: SQLite enforces one only when the
    // connection asked it to, so the rows would otherwise outlive the form on
    // one database and not on the others.
    form_submission::Entity::delete_many()
        .filter(form_submission::Column::FormId.eq(id))
        .exec(db)
        .await?;

    let result = form::Entity::delete_by_id(id).exec(db).await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("form {id}")));
    }
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
#[utoipa::path(
    get,
    path = "/forms/{id}/submissions",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id"), SubmissionsQuery),
    responses((status = 200, description = "Submissions, newest first", body = Vec<SubmissionResponse>))
)]
pub async fn list_submissions(
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
#[utoipa::path(
    post,
    path = "/forms/{id}/seen",
    tag = "forms",
    params(("id" = Uuid, Path, description = "Form id")),
    responses((status = 204, description = "Marked"))
)]
pub async fn mark_seen(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
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
    Path((id, submission_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let result = form_submission::Entity::delete_many()
        .filter(form_submission::Column::Id.eq(submission_id))
        // Both halves of the path have to agree, or one form's address could
        // be used to delete another form's post.
        .filter(form_submission::Column::FormId.eq(id))
        .exec(state.db())
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("submission {submission_id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Send something through a form.
///
/// This one is open: it is the address the site's own pages post to, and a
/// visitor filling in a contact form has no account here. The form's fields
/// are the whole of what it accepts — see `dto::forms::clean_submission`.
#[utoipa::path(
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

    let form = form::Entity::find()
        .filter(form::Column::Slug.eq(&slug))
        // A switched-off form is missing rather than refused: whether one
        // exists is not something an open address should tell people.
        .filter(form::Column::Active.eq(true))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("form {slug}")))?;

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

    Ok(StatusCode::CREATED)
}
