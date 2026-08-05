use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::entities::language;

#[derive(Debug, Serialize, ToSchema)]
pub struct LanguageResponse {
    pub code: String,
    pub name: String,
    pub native_name: String,
    pub direction: String,
    pub is_default: bool,
    pub is_active: bool,
    pub sort_order: i32,
}

impl From<language::Model> for LanguageResponse {
    fn from(model: language::Model) -> Self {
        Self {
            code: model.code,
            name: model.name,
            native_name: model.native_name,
            direction: model.direction,
            is_default: model.is_default,
            is_active: model.is_active,
            sort_order: model.sort_order,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLanguageRequest {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub native_name: String,
    /// "ltr" (default) or "rtl".
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}

/// `code` is intentionally absent: it is the primary key and is denormalized
/// into posts/categories/tags without a foreign key, so renaming it would
/// orphan content. Replace a language by creating a new one and reassigning.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateLanguageRequest {
    pub name: Option<String>,
    pub native_name: Option<String>,
    pub direction: Option<String>,
    pub is_active: Option<bool>,
    pub is_default: Option<bool>,
    pub sort_order: Option<i32>,
}

/// Returned with 409 when a language still holds content, so the panel can
/// tell the admin exactly what would be lost.
#[derive(Debug, Serialize, ToSchema)]
pub struct LanguageUsage {
    pub posts: u64,
    pub categories: u64,
    pub tags: u64,
}
