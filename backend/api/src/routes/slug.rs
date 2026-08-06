use axum::{Json, extract::Query};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::{AppError, AppResult},
    slug::slugify,
};

#[derive(Debug, Deserialize, IntoParams)]
pub struct SlugQuery {
    /// The text to turn into an address, usually a title.
    pub text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SlugResponse {
    pub slug: String,
}

/// Turn a piece of text into an address.
///
/// Exists so that everything agrees on one answer. Editors used to slug titles
/// themselves, which meant two implementations that could drift apart and give
/// the address in the browser a different shape from the one the server stored.
#[utoipa::path(
    get,
    path = "/slug",
    tag = "content",
    params(SlugQuery),
    responses(
        (status = 200, description = "The slug for that text", body = SlugResponse),
        (status = 400, description = "No text given", body = crate::error::ErrorBody),
    )
)]
pub async fn make_slug(Query(query): Query<SlugQuery>) -> AppResult<Json<SlugResponse>> {
    if query.text.trim().is_empty() {
        return Err(AppError::Validation("text must not be empty".to_string()));
    }
    Ok(Json(SlugResponse {
        slug: slugify(&query.text),
    }))
}
