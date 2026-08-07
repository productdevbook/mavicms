use axum::{Json, extract::Path, http::StatusCode};
use uuid::Uuid;

use crate::{auth::Administrator, error::AppResult, tenants::Site, trash};

/// Everything that has been deleted and can still be got back.
#[utoipa::path(
    get,
    path = "/trash",
    tag = "trash",
    responses((status = 200, description = "What is in the bin", body = Vec<trash::Entry>))
)]
pub async fn list_trash(
    _admin: Administrator,
    Site(state): Site,
) -> AppResult<Json<Vec<trash::Entry>>> {
    Ok(Json(trash::list(state.db()).await?))
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct RestoredResponse {
    /// What came back, named the way a person would recognise it.
    pub what: String,
}

/// Puts one back.
#[utoipa::path(
    post,
    path = "/trash/{id}/restore",
    tag = "trash",
    params(("id" = String, Path, description = "The bin entry")),
    responses(
        (status = 200, description = "It is back", body = RestoredResponse),
        (status = 404, description = "Nothing here by that name", body = crate::error::ErrorBody),
    )
)]
pub async fn restore_trash(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<Json<RestoredResponse>> {
    Ok(Json(RestoredResponse {
        what: trash::restore(state.db(), id).await?,
    }))
}

/// Throws one away for good, without waiting for the thirty days.
#[utoipa::path(
    delete,
    path = "/trash/{id}",
    tag = "trash",
    params(("id" = String, Path, description = "The bin entry")),
    responses((status = 204, description = "Gone"))
)]
pub async fn purge_trash(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();
    let (entry, payload) = trash::take(db, id).await?;

    // The bytes go with the row, and only now: this is the point at which
    // nobody is coming back for them.
    match entry.kind.as_str() {
        trash::MEDIA => trash::drop_the_file(&state, &payload).await,
        trash::POST | trash::PAGE => trash::drop_what_a_purged_post_used(&state, &payload).await,
        _ => {}
    }

    trash::forget(db, entry.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
