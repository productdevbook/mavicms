//! Saying what a site publishes.
//!
//! Reading is open to any account, a build included: a front end that lays out
//! a course has to know a course has a price. Changing them is an
//! administrator's, because a field removed here is a field that disappears
//! from every piece of content that had it.

use axum::{Json, extract::Path, http::StatusCode};
use uuid::Uuid;

use crate::{
    auth::Administrator,
    content::{self, ContentTypeResponse, SaveContentTypeRequest},
    error::AppResult,
    tenants::Site,
};

/// The kinds of thing this site holds, and what each is made of.
#[utoipa::path(
    get,
    path = "/content-types",
    tag = "content",
    responses((status = 200, description = "The kinds", body = Vec<ContentTypeResponse>))
)]
pub async fn list_content_types(Site(state): Site) -> AppResult<Json<Vec<ContentTypeResponse>>> {
    Ok(Json(content::described(state.db()).await?))
}

/// Add one.
#[utoipa::path(
    post,
    path = "/content-types",
    tag = "content",
    request_body = SaveContentTypeRequest,
    responses(
        (status = 201, description = "Made", body = ContentTypeResponse),
        (status = 409, description = "That name is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn create_content_type(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<SaveContentTypeRequest>,
) -> AppResult<(StatusCode, Json<ContentTypeResponse>)> {
    Ok((
        StatusCode::CREATED,
        Json(content::create(state.db(), payload).await?),
    ))
}

/// Change what one is called, or what it is made of.
///
/// Its address is not among the things that can change: a front end is already
/// asking for `?kind=course`, and renaming it would empty that page without
/// anything appearing to have gone wrong.
#[utoipa::path(
    put,
    path = "/content-types/{id}",
    tag = "content",
    params(("id" = Uuid, Path, description = "The kind's id")),
    request_body = SaveContentTypeRequest,
    responses((status = 200, description = "Changed", body = ContentTypeResponse))
)]
pub async fn update_content_type(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<SaveContentTypeRequest>,
) -> AppResult<Json<ContentTypeResponse>> {
    Ok(Json(content::update(state.db(), id, payload).await?))
}

/// Remove one nothing is written in.
#[utoipa::path(
    delete,
    path = "/content-types/{id}",
    tag = "content",
    params(("id" = Uuid, Path, description = "The kind's id")),
    responses(
        (status = 204, description = "Removed"),
        (status = 400, description = "Built in, or there is content of this kind", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_content_type(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    content::remove(state.db(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}
