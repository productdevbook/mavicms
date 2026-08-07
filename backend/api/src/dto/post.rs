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

/// A short, stable fingerprint of everything a post renders to.
///
/// A site generator compares it against what it built last time to decide
/// whether a page has to be made again, so it covers what ends up on the page
/// and deliberately not `updated_at`: opening a post and saving it unchanged
/// must not rebuild the site.
///
/// It is on the listing whether or not the bodies were asked for. A build that
/// only wants to know what changed should not have to download every body to
/// find out.
pub fn digest(model: &PostModel) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hasher.update(bytes);
        // Without a separator "ab" + "c" and "a" + "bc" are the same post.
        hasher.update([0x1f]);
    };

    field(model.title.as_bytes());
    field(model.slug.as_bytes());
    field(model.excerpt.as_bytes());
    field(model.status.as_bytes());
    field(
        model
            .publish_at
            .map(|at| at.to_rfc3339())
            .unwrap_or_default()
            .as_bytes(),
    );
    field(model.author.as_bytes());
    field(model.category.as_bytes());
    field(model.tags.to_string().as_bytes());
    field(model.cover_url.as_bytes());
    field(model.seo_title.as_bytes());
    field(model.seo_description.as_bytes());
    field(model.canonical.as_bytes());
    field(&[model.featured as u8]);
    field(&[model.allow_comments as u8]);
    field(model.content_html.as_bytes());
    field(
        model
            .content_markdown
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    field(model.locale.as_bytes());
    field(model.translation_group_id.as_bytes());
    field(model.created_at.to_rfc3339().as_bytes());

    hex::encode(&hasher.finalize()[..16])
}

/// A blog post as stored and returned by the API.
#[derive(Debug, Serialize, ToSchema)]
pub struct PostSummary {
    pub id: Uuid,
    /// Only present when the listing was asked for it with `include=content`.
    /// A site generator building every page needs every body, and one request
    /// carrying them beats nine hundred that each fetch one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_markdown: Option<String>,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    /// The three strings a page's <head> is made of, and only when the listing
    /// asked with `include=seo`. They are short enough to have lived here
    /// outright, but a listing is a published shape and adding fields to it
    /// unasked changes what every existing consumer receives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seo_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seo_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    pub status: PostStatus,
    pub publish_at: Option<DateTime<FixedOffset>>,
    pub author: String,
    pub category: String,
    pub category_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub cover_url: String,
    pub featured: bool,
    pub locale: String,
    pub translation_group_id: Uuid,
    /// Which languages this post exists in, including its own.
    pub locales: Vec<String>,
    /// See [`digest`]. What a build keys its cache on.
    pub digest: String,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

impl PostSummary {
    pub fn from_model(
        model: PostModel,
        category_ids: Vec<Uuid>,
        locales: Vec<String>,
        extras: Extras,
    ) -> Self {
        Self {
            digest: digest(&model),
            id: model.id,
            content_html: extras.content.then_some(model.content_html),
            content_markdown: extras.content.then_some(model.content_markdown).flatten(),
            title: model.title,
            slug: model.slug,
            excerpt: model.excerpt,
            seo_title: extras.seo.then_some(model.seo_title),
            seo_description: extras.seo.then_some(model.seo_description),
            canonical: extras.seo.then_some(model.canonical),
            status: PostStatus::from_str_lenient(&model.status),
            publish_at: model.publish_at,
            author: model.author,
            category: model.category,
            category_ids,
            tags: serde_json::from_value(model.tags).unwrap_or_default(),
            cover_url: model.cover_url,
            featured: model.featured,
            locale: model.locale,
            translation_group_id: model.translation_group_id,
            locales,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

/// What a listing was asked to carry beyond the usual.
///
/// Both are opt-in, and for the same reason: a build wants everything in one
/// answer, and a search index or a sitemap wants none of it. The bodies are
/// large, the SEO strings are not, and neither belongs in a shape somebody
/// else is already parsing unless they asked.
#[derive(Debug, Clone, Copy, Default)]
pub struct Extras {
    pub content: bool,
    pub seo: bool,
}

impl Extras {
    /// Reads the `include` query, which is a comma-separated list of names.
    pub fn from_include(include: Option<&str>) -> Self {
        let has = |wanted: &str| {
            include.is_some_and(|value| value.split(',').any(|part| part.trim() == wanted))
        };
        Self {
            content: has("content"),
            seo: has("seo"),
        }
    }
}

/// A page of posts, with the total so a consumer can page through predictably.
#[derive(Debug, Serialize, ToSchema)]
pub struct PostPage {
    pub items: Vec<PostSummary>,
    /// How many posts match the filters, ignoring limit and offset.
    pub total: u64,
    pub limit: u64,
    pub offset: u64,
    /// How many posts are in each status, across every page. A dashboard
    /// counting the rows it was handed would report the size of the page.
    pub counts: StatusCounts,
}

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct StatusCounts {
    pub draft: u64,
    pub review: u64,
    pub scheduled: u64,
    pub published: u64,
}

impl StatusCounts {
    pub fn add(&mut self, status: &str, n: u64) {
        match PostStatus::from_str_lenient(status) {
            PostStatus::Draft => self.draft += n,
            PostStatus::Review => self.review += n,
            PostStatus::Scheduled => self.scheduled += n,
            PostStatus::Published => self.published += n,
        }
    }
}

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
    pub content_markdown: Option<String>,
    pub locale: String,
    pub translation_group_id: Uuid,
    /// Which languages this post exists in, including its own. Cheap enough to
    /// return on the list endpoint; the full sibling details come from
    /// `GET /posts/{id}`.
    pub locales: Vec<String>,
    /// Sibling language versions. Empty on the list endpoint.
    pub translations: Vec<PostTranslation>,
    /// See [`digest`]. What a build keys its cache on.
    pub digest: String,
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
        let fingerprint = digest(&model);
        let tags: Vec<String> = serde_json::from_value(model.tags).unwrap_or_default();

        Self {
            digest: fingerprint,
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
            content_markdown: model.content_markdown,
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
    /// The canonical form. The editor sends both; an importer that only has
    /// HTML may send that alone.
    #[serde(default)]
    pub content_markdown: Option<String>,
    /// Defaults to the site's default language.
    #[serde(default)]
    pub locale: Option<String>,
    /// The id of an existing post this is a translation of; the server takes
    /// that post's translation group.
    #[serde(default)]
    pub translation_of: Option<Uuid>,
    /// Original creation time, for content imported from another system.
    /// Without it an archive of old posts would all read as written today.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
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
    pub content_markdown: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Extras, PostModel, PostSummary, digest};

    fn post() -> PostModel {
        let now = chrono::Utc::now().fixed_offset();
        PostModel {
            id: uuid::Uuid::now_v7(),
            title: "A title".to_string(),
            slug: "a-title".to_string(),
            excerpt: String::new(),
            status: "published".to_string(),
            publish_at: None,
            author: "Someone".to_string(),
            category: String::new(),
            tags: serde_json::json!([]),
            cover_url: String::new(),
            seo_title: String::new(),
            seo_description: String::new(),
            canonical: String::new(),
            featured: false,
            allow_comments: true,
            content_html: "<p>A body</p>".to_string(),
            content_markdown: Some("A body".to_string()),
            locale: "en".to_string(),
            translation_group_id: uuid::Uuid::now_v7(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn a_listing_carries_the_head_of_a_page_only_when_asked() {
        let mut model = post();
        model.seo_title = "What the tab says".to_string();
        model.seo_description = "What the search result says".to_string();
        model.canonical = "https://example.test/a-title".to_string();

        let listed =
            |extras| PostSummary::from_model(model.clone(), Vec::new(), Vec::new(), extras);

        // The shape every existing consumer already parses, unchanged.
        let plain = listed(Extras::default());
        assert_eq!(plain.seo_title, None);
        assert_eq!(plain.canonical, None);
        assert_eq!(plain.content_html, None);

        // Asked for, and the values are the post's own rather than blanks —
        // reading them from a listing used to give nothing back at all, which
        // is how a site shipped with the CMS's titles quietly unused.
        let with_seo = listed(Extras::from_include(Some("seo")));
        assert_eq!(with_seo.seo_title.as_deref(), Some("What the tab says"));
        assert_eq!(
            with_seo.seo_description.as_deref(),
            Some("What the search result says")
        );
        assert_eq!(
            with_seo.canonical.as_deref(),
            Some("https://example.test/a-title")
        );
        // The bodies are the expensive part and are still not sent.
        assert_eq!(with_seo.content_html, None);

        let both = listed(Extras::from_include(Some("content, seo")));
        assert!(both.seo_title.is_some());
        assert!(both.content_html.is_some());

        // A name nobody defined is not a way to get everything.
        let neither = listed(Extras::from_include(Some("seotitle,contents")));
        assert_eq!(neither.seo_title, None);
        assert_eq!(neither.content_html, None);
    }

    #[test]
    fn changing_the_body_changes_the_digest() {
        let before = post();
        let mut after = before.clone();
        after.content_html = "<p>Another body</p>".to_string();

        assert_ne!(digest(&before), digest(&after));
    }

    #[test]
    fn saving_without_changing_anything_does_not() {
        let before = post();
        let mut after = before.clone();
        after.updated_at += chrono::Duration::hours(1);

        assert_eq!(digest(&before), digest(&after));
    }

    /// Two fields that run into one another would let a rename go unnoticed.
    #[test]
    fn a_field_boundary_is_part_of_the_digest() {
        let mut before = post();
        before.title = "ab".to_string();
        before.slug = "c".to_string();

        let mut after = before.clone();
        after.title = "a".to_string();
        after.slug = "bc".to_string();

        assert_ne!(digest(&before), digest(&after));
    }
}
