use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::entities::media;

/// Pulls an image from a public URL into the media library — the migration
/// path for sites moving off WordPress.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportMediaRequest {
    pub url: String,
    /// Defaults to the last path segment of the URL.
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub alt_text: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MediaResponse {
    pub id: Uuid,
    pub filename: String,
    pub url: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub alt_text: String,
    pub uploaded_at: DateTime<FixedOffset>,
}

impl From<media::Model> for MediaResponse {
    fn from(model: media::Model) -> Self {
        Self {
            id: model.id,
            filename: model.filename,
            url: model.url_path,
            mime_type: model.mime_type,
            size_bytes: model.size_bytes,
            alt_text: model.alt_text,
            uploaded_at: model.uploaded_at,
        }
    }
}
