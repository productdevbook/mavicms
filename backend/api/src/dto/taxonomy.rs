use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::entities::{category, tag};

#[derive(Debug, Serialize, ToSchema)]
pub struct CategoryResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub parent_id: Option<Uuid>,
    pub description: String,
    pub locale: String,
    pub translation_group_id: Uuid,
    /// "complete" or "needs_translation" (an auto-created stub).
    pub translation_status: String,
}

impl From<category::Model> for CategoryResponse {
    fn from(model: category::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            slug: model.slug,
            parent_id: model.parent_id,
            description: model.description,
            locale: model.locale,
            translation_group_id: model.translation_group_id,
            translation_status: model.translation_status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub description: String,
    /// Defaults to the site's default language.
    #[serde(default)]
    pub locale: Option<String>,
    /// The id of an existing category this is a translation of. The server
    /// resolves its group — clients never supply a raw group id, so a group
    /// can't be invented that references nothing.
    #[serde(default)]
    pub translation_of: Option<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    pub name: Option<String>,
    pub parent_id: Option<Option<Uuid>>,
    pub description: Option<String>,
    pub translation_status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TagResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub locale: String,
    pub translation_group_id: Uuid,
    pub translation_status: String,
}

impl From<tag::Model> for TagResponse {
    fn from(model: tag::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            slug: model.slug,
            locale: model.locale,
            translation_group_id: model.translation_group_id,
            translation_status: model.translation_status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(default)]
    pub locale: Option<String>,
    /// See `CreateCategoryRequest::translation_of`.
    #[serde(default)]
    pub translation_of: Option<Uuid>,
}

/// Filter for the list endpoints.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct LocaleQuery {
    /// Comma-separated language codes. Omitted means every language.
    #[serde(default)]
    pub locale: Option<String>,
    /// Exact address to look for. Lets an importer ask whether something it is
    /// about to send is already here, without fetching the whole archive.
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub offset: Option<u64>,
    /// Comma-separated extras to include; `content` adds the post body.
    #[serde(default)]
    pub include: Option<String>,
    /// Free text to search for in the title, excerpt and body.
    #[serde(default)]
    pub q: Option<String>,
    /// Comma-separated statuses. Omitted means every status, which is what the
    /// panel wants and the opposite of what a build does: a site generator
    /// asks for `published` and would otherwise put somebody's drafts online.
    #[serde(default)]
    pub status: Option<String>,
}

impl LocaleQuery {
    /// The statuses asked for, as the strings the column holds. A name that is
    /// not a status is refused rather than ignored, because ignoring it would
    /// answer a build asking for `publshed` with every draft the site has.
    pub fn statuses(&self) -> crate::error::AppResult<Option<Vec<String>>> {
        let Some(raw) = self.status.as_deref() else {
            return Ok(None);
        };

        let mut wanted = Vec::new();
        for name in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let status = match name {
                "draft" => "draft",
                "review" => "review",
                "scheduled" => "scheduled",
                "published" => "published",
                other => {
                    return Err(crate::error::AppError::Validation(format!(
                        "{other} is not a status: draft, review, scheduled or published"
                    )));
                }
            };
            if !wanted.iter().any(|held| held == status) {
                wanted.push(status.to_string());
            }
        }
        Ok((!wanted.is_empty()).then_some(wanted))
    }

    pub fn codes(&self) -> Option<Vec<String>> {
        let raw = self.locale.as_deref()?;
        let codes: Vec<String> = raw
            .split(',')
            .map(crate::languages::normalize_code)
            .filter(|c| !c.is_empty())
            .collect();
        (!codes.is_empty()).then_some(codes)
    }
}
