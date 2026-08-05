use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::entities::post::Model as PostModel;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PostStatus {
    #[default]
    Draft,
    Review,
    Scheduled,
    Published,
}

impl PostStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PostStatus::Draft => "draft",
            PostStatus::Review => "review",
            PostStatus::Scheduled => "scheduled",
            PostStatus::Published => "published",
        }
    }

    pub fn from_str_lenient(value: &str) -> Self {
        match value {
            "review" => PostStatus::Review,
            "scheduled" => PostStatus::Scheduled,
            "published" => PostStatus::Published,
            _ => PostStatus::Draft,
        }
    }
}

/// A blog post as stored and returned by the API.
#[derive(Debug, Serialize, ToSchema)]
pub struct PostResponse {
    pub id: Uuid,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub status: PostStatus,
    pub publish_at: Option<DateTime<FixedOffset>>,
    pub author: String,
    pub category: String,
    pub category_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub cover_url: String,
    pub seo_title: String,
    pub seo_description: String,
    pub canonical: String,
    pub featured: bool,
    pub allow_comments: bool,
    pub content_html: String,
    pub locale: String,
    pub translation_group_id: Uuid,
    /// Which languages this post exists in, including its own. Cheap enough to
    /// return on the list endpoint; the full sibling details come from
    /// `GET /posts/{id}`.
    pub locales: Vec<String>,
    /// Sibling language versions. Empty on the list endpoint.
    pub translations: Vec<PostTranslation>,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

/// A sibling language version. `slug` is included so consumers can emit
/// `hreflang` alternates without a request per language.
#[derive(Debug, Serialize, ToSchema)]
pub struct PostTranslation {
    pub id: Uuid,
    pub locale: String,
    pub title: String,
    pub slug: String,
    pub status: PostStatus,
}

impl PostResponse {
    /// `category_ids` isn't on the `posts` row itself (it lives in the
    /// `post_categories` join table), so callers fetch it separately and
    /// attach it here rather than through a plain `From` conversion.
    pub fn from_model(
        model: PostModel,
        category_ids: Vec<Uuid>,
        locales: Vec<String>,
        translations: Vec<PostTranslation>,
    ) -> Self {
        let tags: Vec<String> = serde_json::from_value(model.tags).unwrap_or_default();

        Self {
            id: model.id,
            title: model.title,
            slug: model.slug,
            excerpt: model.excerpt,
            status: PostStatus::from_str_lenient(&model.status),
            publish_at: model.publish_at,
            author: model.author,
            category: model.category,
            category_ids,
            tags,
            cover_url: model.cover_url,
            seo_title: model.seo_title,
            seo_description: model.seo_description,
            canonical: model.canonical,
            featured: model.featured,
            allow_comments: model.allow_comments,
            content_html: model.content_html,
            locale: model.locale,
            translation_group_id: model.translation_group_id,
            locales,
            translations,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Payload for creating a new post. Omitted fields fall back to sensible defaults.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePostRequest {
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub status: PostStatus,
    #[serde(default)]
    pub publish_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub category_ids: Vec<Uuid>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover_url: String,
    #[serde(default)]
    pub seo_title: String,
    #[serde(default)]
    pub seo_description: String,
    #[serde(default)]
    pub canonical: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default = "default_true")]
    pub allow_comments: bool,
    #[serde(default)]
    pub content_html: String,
    /// Defaults to the site's default language.
    #[serde(default)]
    pub locale: Option<String>,
    /// The id of an existing post this is a translation of; the server takes
    /// that post's translation group.
    #[serde(default)]
    pub translation_of: Option<Uuid>,
}

/// Payload for updating an existing post. Every field is optional; only the
/// fields present in the request body are changed.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePostRequest {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub excerpt: Option<String>,
    pub status: Option<PostStatus>,
    pub publish_at: Option<Option<DateTime<Utc>>>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub category_ids: Option<Vec<Uuid>>,
    pub tags: Option<Vec<String>>,
    pub cover_url: Option<String>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub canonical: Option<String>,
    pub featured: Option<bool>,
    pub allow_comments: Option<bool>,
    pub content_html: Option<String>,
}
