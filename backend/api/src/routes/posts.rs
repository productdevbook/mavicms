use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
    sea_query::{Expr, ExprTrait, Func, LikeExpr},
};
use serde::Deserialize;
use uuid::Uuid;

/// How many posts a listing returns when no limit is asked for.
const DEFAULT_PAGE: u64 = 50;
/// The most a single page will ever return.
const MAX_PAGE: u64 = 200;

/// Any `src`, `href` or bare address in post content. Deliberately broad: the
/// point is to notice that a file is still referenced, and a false positive
/// only means a file is kept.
static MEDIA_IN_HTML: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"(?:src|href)=["']([^"']+)["']"#).expect("the pattern is valid")
});

use crate::{
    dto::{
        post::{
            CreatePostRequest, PostPage, PostResponse, PostStatus, PostSummary, PostTranslation,
            StatusCounts, UpdatePostRequest,
        },
        taxonomy::LocaleQuery,
    },
    entities::{category, post, post_category, post_tag},
    error::{AppError, AppResult},
    languages::resolve,
    routes::{categories::resolve_for_locale, tags::get_or_create_tag},
    tenants::Site,
};

/// A scheduled post with no date is not a schedule.
///
/// Nothing used to look, and the status did nothing, so the two agreed with
/// each other. Now that the clock is read, a post left in this state is one
/// the panel says is going out and that nothing will ever send.
fn scheduled_needs_a_time(status: &str, has_time: bool) -> AppResult<()> {
    if status == PostStatus::Scheduled.as_str() && !has_time {
        return Err(AppError::Validation(
            "a scheduled post needs a publish date to be scheduled for".to_string(),
        ));
    }
    Ok(())
}

/// The HTML a post is stored with.
///
/// The editor sends both forms and its own wins. Markdown on its own — from an
/// importer, or from an assistant — used to be stored with an empty body,
/// which read correctly in the editor and blank on the site.
fn rendered(markdown: &Option<String>, html: String) -> String {
    match markdown {
        Some(markdown) if html.trim().is_empty() => crate::markdown::to_html(markdown),
        _ => html,
    }
}

async fn category_ids_for(db: &impl ConnectionTrait, post_id: Uuid) -> AppResult<Vec<Uuid>> {
    let rows = post_category::Entity::find()
        .filter(post_category::Column::PostId.eq(post_id))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|row| row.category_id).collect())
}

/// Replaces a post's categories with exactly `category_ids`, translated into
/// the post's own language. See `categories::resolve_for_locale` for why a
/// foreign-language category is resolved rather than rejected.
async fn sync_categories(
    db: &impl ConnectionTrait,
    post_id: Uuid,
    locale: &str,
    category_ids: &[Uuid],
) -> AppResult<Vec<Uuid>> {
    let mut resolved = Vec::with_capacity(category_ids.len());
    for id in category_ids {
        let found = category::Entity::find_by_id(*id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Validation(format!("category {id} not found")))?;
        let in_locale = resolve_for_locale(db, &found, locale).await?;
        if !resolved.contains(&in_locale.id) {
            resolved.push(in_locale.id);
        }
    }

    post_category::Entity::delete_many()
        .filter(post_category::Column::PostId.eq(post_id))
        .exec(db)
        .await?;

    if !resolved.is_empty() {
        let rows = resolved.iter().map(|id| post_category::ActiveModel {
            post_id: Set(post_id),
            category_id: Set(*id),
        });
        post_category::Entity::insert_many(rows).exec(db).await?;
    }
    Ok(resolved)
}

/// Replaces a post's tags with exactly `tag_names`, resolved within the post's
/// language so a German post never picks up an English tag.
async fn sync_tags(
    db: &impl ConnectionTrait,
    post_id: Uuid,
    locale: &str,
    tag_names: &[String],
) -> AppResult<()> {
    let mut tag_ids = Vec::with_capacity(tag_names.len());
    for name in tag_names {
        if name.trim().is_empty() {
            continue;
        }
        let id = get_or_create_tag(db, name, locale).await?.0.id;
        if !tag_ids.contains(&id) {
            tag_ids.push(id);
        }
    }

    post_tag::Entity::delete_many()
        .filter(post_tag::Column::PostId.eq(post_id))
        .exec(db)
        .await?;

    if !tag_ids.is_empty() {
        let rows = tag_ids.iter().map(|id| post_tag::ActiveModel {
            post_id: Set(post_id),
            tag_id: Set(*id),
        });
        post_tag::Entity::insert_many(rows).exec(db).await?;
    }
    Ok(())
}

async fn siblings_of(
    db: &impl ConnectionTrait,
    post: &post::Model,
) -> AppResult<Vec<PostTranslation>> {
    let rows = post::Entity::find()
        .filter(post::Column::TranslationGroupId.eq(post.translation_group_id))
        .filter(post::Column::Id.ne(post.id))
        .all(db)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| PostTranslation {
            id: row.id,
            locale: row.locale,
            title: row.title,
            slug: row.slug,
            status: PostStatus::from_str_lenient(&row.status),
        })
        .collect())
}

/// List posts. Published ones unless `status` says otherwise, every language
/// unless `locale` narrows it —
/// silently defaulting to one language would be a breaking change for API
/// consumers building their own front-ends.
#[utoipa::path(
    get,
    path = "/posts",
    tag = "posts",
    params(
        ("locale" = Option<String>, Query, description = "Comma-separated language codes"),
        ("slug" = Option<String>, Query, description = "Exact address to look for"),
        ("limit" = Option<u64>, Query, description = "How many posts to return (default 50, max 200)"),
        ("offset" = Option<u64>, Query, description = "How many to skip"),
        ("include" = Option<String>, Query, description = "Comma-separated extras: `content` adds the post body to each item, `seo` adds seo_title, seo_description and canonical"),
        ("q" = Option<String>, Query, description = "Free text to search for in the title, excerpt and body"),
        ("status" = Option<String>, Query, description = "Comma-separated statuses. Published only when left out"),
        ("If-None-Match" = Option<String>, Header, description = "An ETag from a previous answer. Unchanged listings come back 304 with no body."),
    ),
    responses(
        (status = 200, description = "A page of posts", body = PostPage,
            headers(("ETag" = String, description = "Send it back as If-None-Match next time"))),
        (status = 304, description = "Unchanged since the ETag the caller sent"),
    )
)]
pub async fn list_posts(
    Site(state): Site,
    Query(query): Query<LocaleQuery>,
    headers: axum::http::HeaderMap,
) -> AppResult<axum::response::Response> {
    crate::etag::conditional(&headers, &page(state.db(), &query).await?)
}

/// A page of posts, as the listing computes it.
///
/// Separate from the handler because the same question is asked over MCP,
/// and a second implementation of "which posts, in what order, with what
/// counted" is how the two answers start disagreeing.
pub async fn page(db: &sea_orm::DatabaseConnection, query: &LocaleQuery) -> AppResult<PostPage> {
    // A whole archive in one response is a multi-megabyte payload that every
    // consumer has to hold in memory, so the page is bounded whether or not
    // one was asked for.
    let limit = query.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let offset = query.offset.unwrap_or(0);
    let extras = crate::dto::post::Extras::from_include(query.include.as_deref());

    let mut find = post::Entity::find();
    if let Some(codes) = query.codes() {
        find = find.filter(post::Column::Locale.is_in(codes));
    }
    find = find.filter(post::Column::Kind.is_in(crate::content::known(db, &query.kinds()).await?));
    if let Some(slug) = query
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        find = find.filter(post::Column::Slug.eq(slug));
    }
    if let Some(term) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // `%` and `_` are wildcards in LIKE; someone searching for "100%"
        // means the character, not "anything".
        let escaped = term
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{}%", escaped.to_lowercase());
        // Lowercased on both sides so the match does not depend on how the
        // author capitalised the word. Databases fold ASCII for free but not
        // every alphabet, so an all-caps Turkish word can still be missed.
        // The escape character has to be declared, or the backslashes above
        // are matched literally and a search for "100%" finds nothing.
        let matches = |column| {
            Expr::expr(Func::lower(Expr::col(column))).like(LikeExpr::new(&pattern).escape('\\'))
        };
        find = find.filter(
            Condition::any()
                .add(matches(post::Column::Title))
                .add(matches(post::Column::Excerpt))
                .add(matches(post::Column::ContentHtml)),
        );
    }
    // Counted before the status filter and in the database rather than from the
    // page: a dashboard tallying the rows it received would report the size of
    // the page, and one tallying after the filter would report "12 published,
    // 0 drafts" to a screen whose whole job is to say how many drafts there
    // are.
    let mut counts = StatusCounts::default();
    for (status, n) in find
        .clone()
        .select_only()
        .column(post::Column::Status)
        .column_as(post::Column::Id.count(), "n")
        .group_by(post::Column::Status)
        .into_tuple::<(String, i64)>()
        .all(db)
        .await?
    {
        counts.add(&status, Ord::max(n, 0) as u64);
    }

    // Published unless asked otherwise. The other way round for years, and the
    // failure it invites is the one a publishing system cannot have: a build
    // that forgets to say puts every draft on the internet. Saying nothing now
    // means seeing less than you expected, which somebody notices; the panel,
    // which is the one caller that wants everything, asks for everything.
    find = find.filter(
        post::Column::Status.is_in(
            query
                .statuses()?
                .unwrap_or_else(|| vec![PostStatus::Published.as_str().to_string()]),
        ),
    );
    let total = find.clone().count(db).await?;
    // Ordered here rather than where the filters are built: an ORDER BY that
    // travels into the grouped count above is a column Postgres will not let
    // out of a GROUP BY, and the count does not care what order it counts in.
    let posts = find
        .order_by_desc(post::Column::CreatedAt)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?;

    // Two grouped queries rather than two per post: this used to issue one
    // category lookup per row, and adding translations would have tripled it.
    let post_ids: Vec<Uuid> = posts.iter().map(|p| p.id).collect();
    let group_ids: Vec<Uuid> = posts.iter().map(|p| p.translation_group_id).collect();

    let mut categories_by_post: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    if !post_ids.is_empty() {
        for row in post_category::Entity::find()
            .filter(post_category::Column::PostId.is_in(post_ids))
            .all(db)
            .await?
        {
            categories_by_post
                .entry(row.post_id)
                .or_default()
                .push(row.category_id);
        }
    }

    let mut locales_by_group: HashMap<Uuid, Vec<String>> = HashMap::new();
    if !group_ids.is_empty() {
        for row in post::Entity::find()
            .filter(post::Column::TranslationGroupId.is_in(group_ids))
            .all(db)
            .await?
        {
            locales_by_group
                .entry(row.translation_group_id)
                .or_default()
                .push(row.locale);
        }
    }

    let responses = posts
        .into_iter()
        .map(|post| {
            let category_ids = categories_by_post.remove(&post.id).unwrap_or_default();
            let locales = locales_by_group
                .get(&post.translation_group_id)
                .cloned()
                .unwrap_or_default();
            PostSummary::from_model(post, category_ids, locales, extras)
        })
        .collect();

    Ok(PostPage {
        items: responses,
        total,
        limit,
        offset,
        counts,
    })
}

/// Fetch a single post, including its sibling language versions.
#[utoipa::path(
    get,
    path = "/posts/{id}",
    tag = "posts",
    params(("id" = Uuid, Path, description = "Post id")),
    responses(
        (status = 200, description = "Post found", body = PostResponse),
        (status = 404, description = "Post not found", body = crate::error::ErrorBody),
    )
)]
pub async fn get_post(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<Json<PostResponse>> {
    let db = state.db();
    let post = post::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("post {id}")))?;

    Ok(Json(detail(db, post).await?))
}

/// One post with everything that is not on its own row: which categories it
/// is in, and the languages it exists in. See [`page`] for why this is not
/// inside the handler.
pub async fn detail(
    db: &sea_orm::DatabaseConnection,
    post: post::Model,
) -> AppResult<PostResponse> {
    let category_ids = category_ids_for(db, post.id).await?;
    let translations = siblings_of(db, &post).await?;
    let mut locales: Vec<String> = translations.iter().map(|t| t.locale.clone()).collect();
    locales.push(post.locale.clone());

    Ok(PostResponse::from_model(
        post,
        category_ids,
        locales,
        translations,
    ))
}

/// Create a post, optionally as the translation of an existing one.
#[utoipa::path(
    post,
    path = "/posts",
    tag = "posts",
    request_body = CreatePostRequest,
    responses((status = 201, description = "Post created", body = PostResponse))
)]
pub async fn create_post(
    Site(state): Site,
    Json(payload): Json<CreatePostRequest>,
) -> AppResult<(StatusCode, Json<PostResponse>)> {
    Ok((
        StatusCode::CREATED,
        Json(create(state.db(), payload).await?),
    ))
}

/// Writing a post, as the endpoint does it. See [`page`] for why this is not
/// inside the handler.
pub async fn create(
    db: &sea_orm::DatabaseConnection,
    payload: CreatePostRequest,
) -> AppResult<PostResponse> {
    if payload.title.trim().is_empty() {
        return Err(AppError::Validation("title must not be empty".to_string()));
    }

    // One place decides what an address looks like, and it is this one.
    let slug = crate::slug::slugify_or(
        payload
            .slug
            .as_deref()
            .map(str::trim)
            .filter(|given| !given.is_empty())
            .unwrap_or(&payload.title),
        "post",
    );

    let locale = resolve(db, payload.locale.as_deref()).await?;

    let mut translation_group_id = Uuid::now_v7();
    if let Some(sibling_id) = payload.translation_of {
        let sibling = post::Entity::find_by_id(sibling_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Validation("translation_of post not found".to_string()))?;
        // Checked up front so the common "add the German version twice" case
        // gets a useful message instead of a bare unique-index conflict.
        if post::Entity::find()
            .filter(post::Column::TranslationGroupId.eq(sibling.translation_group_id))
            .filter(post::Column::Locale.eq(&locale))
            .one(db)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "a {locale} version of that post already exists"
            )));
        }
        translation_group_id = sibling.translation_group_id;
    }

    scheduled_needs_a_time(payload.status.as_str(), payload.publish_at.is_some())?;

    // Checked up front so a re-run over content that is already there says
    // which post is in the way, instead of a bare unique-index conflict.
    let wanted_kind = payload
        .kind
        .clone()
        .unwrap_or_else(|| crate::content::POST.to_string());
    let of_kind = crate::content::by_slug(db, &wanted_kind).await?;
    let values = crate::content::checked_values(
        &crate::content::fields_of(&of_kind.fields)?,
        payload.fields.clone(),
        crate::content::is_finished(&payload.status),
    )?;
    if post::Entity::find()
        .filter(post::Column::Locale.eq(&locale))
        .filter(post::Column::Kind.eq(&wanted_kind))
        .filter(post::Column::Slug.eq(&slug))
        .one(db)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(format!(
            "a {locale} {wanted_kind} with the address \"{slug}\" already exists"
        )));
    }

    let txn = db.begin().await?;
    let now = Utc::now().fixed_offset();
    let id = Uuid::now_v7();

    let model = post::ActiveModel {
        id: Set(id),
        kind: Set(wanted_kind.clone()),
        fields: Set(values),
        title: Set(payload.title),
        slug: Set(slug),
        excerpt: Set(payload.excerpt),
        status: Set(payload.status.as_str().to_string()),
        publish_at: Set(payload.publish_at.map(|value| value.fixed_offset())),
        author: Set(payload.author),
        category: Set(payload.category),
        tags: Set(serde_json::to_value(&payload.tags).unwrap_or_default()),
        cover_url: Set(payload.cover_url),
        seo_title: Set(payload.seo_title),
        seo_description: Set(payload.seo_description),
        canonical: Set(payload.canonical),
        featured: Set(payload.featured),
        allow_comments: Set(payload.allow_comments),
        content_html: Set(rendered(&payload.content_markdown, payload.content_html)),
        content_markdown: Set(payload.content_markdown),
        locale: Set(locale.clone()),
        translation_group_id: Set(translation_group_id),
        created_at: Set(payload.created_at.map_or(now, |value| value.fixed_offset())),
        updated_at: Set(now),
    };

    let saved = model.insert(&txn).await?;
    let category_ids = sync_categories(&txn, id, &locale, &payload.category_ids).await?;
    sync_tags(&txn, id, &locale, &payload.tags).await?;
    txn.commit().await?;

    let translations = siblings_of(db, &saved).await?;
    let mut locales: Vec<String> = translations.iter().map(|t| t.locale.clone()).collect();
    locales.push(saved.locale.clone());

    Ok(PostResponse::from_model(
        saved,
        category_ids,
        locales,
        translations,
    ))
}

/// Partially update an existing post. Only the fields present in the body are changed.
#[utoipa::path(
    put,
    path = "/posts/{id}",
    tag = "posts",
    params(("id" = Uuid, Path, description = "Post id")),
    request_body = UpdatePostRequest,
    responses(
        (status = 200, description = "Post updated", body = PostResponse),
        (status = 404, description = "Post not found", body = crate::error::ErrorBody),
    )
)]
pub async fn update_post(
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdatePostRequest>,
) -> AppResult<Json<PostResponse>> {
    Ok(Json(update(state.db(), id, payload).await?))
}

/// Changing a post, as the endpoint does it. See [`page`].
pub async fn update(
    db: &sea_orm::DatabaseConnection,
    id: Uuid,
    payload: UpdatePostRequest,
) -> AppResult<PostResponse> {
    let existing = post::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("post {id}")))?;
    // A post's language is fixed once created: changing it could collide with
    // a sibling and would silently move the content between languages.
    let locale = existing.locale.clone();
    // Read before the change, so that "was it already out" can be asked after.
    let was_published =
        crate::content::is_finished(&PostStatus::from_str_lenient(&existing.status));

    let txn = db.begin().await?;
    let mut model: post::ActiveModel = existing.into();

    if let Some(title) = payload.title {
        if title.trim().is_empty() {
            return Err(AppError::Validation("title must not be empty".to_string()));
        }
        model.title = Set(title);
    }
    if let Some(slug) = payload.slug {
        if slug.trim().is_empty() {
            return Err(AppError::Validation("slug must not be empty".to_string()));
        }
        model.slug = Set(slug);
    }
    if let Some(excerpt) = payload.excerpt {
        model.excerpt = Set(excerpt);
    }
    if let Some(status) = payload.status {
        model.status = Set(status.as_str().to_string());
    }
    if let Some(publish_at) = payload.publish_at {
        model.publish_at = Set(publish_at.map(|value| value.fixed_offset()));
    }
    if let Some(author) = payload.author {
        model.author = Set(author);
    }
    if let Some(category) = payload.category {
        model.category = Set(category);
    }
    if let Some(tags) = &payload.tags {
        model.tags = Set(serde_json::to_value(tags).unwrap_or_default());
    }
    if let Some(cover_url) = payload.cover_url {
        model.cover_url = Set(cover_url);
    }
    if let Some(seo_title) = payload.seo_title {
        model.seo_title = Set(seo_title);
    }
    if let Some(seo_description) = payload.seo_description {
        model.seo_description = Set(seo_description);
    }
    if let Some(canonical) = payload.canonical {
        model.canonical = Set(canonical);
    }
    if let Some(featured) = payload.featured {
        model.featured = Set(featured);
    }
    if let Some(allow_comments) = payload.allow_comments {
        model.allow_comments = Set(allow_comments);
    }
    // Markdown is the canonical form, so a change to it that arrives without
    // the HTML renders one rather than leaving the old body on the page.
    match (payload.content_html, &payload.content_markdown) {
        (Some(html), _) => model.content_html = Set(html),
        (None, Some(markdown)) => model.content_html = Set(crate::markdown::to_html(markdown)),
        (None, None) => {}
    }
    if let Some(content_markdown) = payload.content_markdown {
        model.content_markdown = Set(Some(content_markdown));
    }
    // Against the status the post is about to have, which the change above may
    // just have altered.
    let going_out =
        crate::content::is_finished(&PostStatus::from_str_lenient(model.status.as_ref()));
    // Two moments matter, and only two. New values are checked as they arrive.
    // And a piece of content on its way out is checked whole, even when the
    // request only said "publish" — otherwise a course goes online with no
    // price by the simple method of not mentioning the price.
    //
    // Deliberately not on every save: a kind that gains a required field
    // afterwards would otherwise make every existing piece unfixable until
    // somebody filled the new field in, which is a strange thing to be told
    // while correcting a typo.
    let becoming_finished = going_out && payload.status.is_some();
    if payload.fields.is_some() || becoming_finished {
        // Checked against the kind the post actually is, not one the request
        // could name: a post does not change kind.
        let of_kind = crate::content::by_slug(db, model.kind.as_ref()).await?;
        let given = match payload.fields {
            Some(given) => Some(given),
            // Nothing new to store, so what is already there is what is being
            // published.
            None => Some(crate::content::read_values(model.fields.as_ref())),
        };
        model.fields = Set(crate::content::checked_values(
            &crate::content::fields_of(&of_kind.fields)?,
            given,
            going_out,
        )?);
    }
    model.updated_at = Set(Utc::now().fixed_offset());

    // Asked of what the post will be, not of what the request said: a request
    // that only clears the date leaves the status alone, and a request that
    // only sets the status leaves the date alone. Either one on its own can
    // arrive at a schedule with no time in it.
    scheduled_needs_a_time(model.status.as_ref(), model.publish_at.as_ref().is_some())?;

    let saved = model.update(&txn).await?;

    if let Some(category_ids) = &payload.category_ids {
        sync_categories(&txn, id, &locale, category_ids).await?;
    }
    if let Some(tags) = &payload.tags {
        sync_tags(&txn, id, &locale, tags).await?;
    }
    txn.commit().await?;

    // After the commit, and only on the crossing: a post saved again while
    // already published has not been published again, and a flow that emails
    // a mailing list does not want to hear about a corrected typo.
    let now_published = crate::content::is_finished(&PostStatus::from_str_lenient(&saved.status));
    if now_published
        && !was_published
        && let Err(err) = crate::flows::fire(
            db,
            crate::flows::trigger::POST_PUBLISHED,
            serde_json::json!({
                "id": saved.id.to_string(),
                "title": saved.title,
                "slug": saved.slug,
                "kind": saved.kind,
                "locale": saved.locale,
            }),
        )
        .await
    {
        tracing::warn!(post = %saved.slug, error = %err, "could not start the flows for this post");
    }

    let category_ids = category_ids_for(db, id).await?;
    let translations = siblings_of(db, &saved).await?;
    let mut locales: Vec<String> = translations.iter().map(|t| t.locale.clone()).collect();
    locales.push(saved.locale.clone());

    Ok(PostResponse::from_model(
        saved,
        category_ids,
        locales,
        translations,
    ))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TranslationGroupRequest {
    /// Id of a post whose translation group this post should join.
    #[serde(default)]
    pub join: Option<Uuid>,
    /// Split this post out into a group of its own.
    #[serde(default)]
    pub detach: bool,
}

/// Re-link a post's translation group. Needed to fix bad groupings and, later,
/// to link posts that a WordPress import could not group automatically.
#[utoipa::path(
    patch,
    path = "/posts/{id}/translation-group",
    tag = "posts",
    params(("id" = Uuid, Path, description = "Post id")),
    request_body = TranslationGroupRequest,
    responses(
        (status = 200, description = "Group updated", body = PostResponse),
        (status = 404, description = "Post not found", body = crate::error::ErrorBody),
    )
)]
pub async fn set_translation_group(
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<TranslationGroupRequest>,
) -> AppResult<Json<PostResponse>> {
    let db = state.db();
    let existing = post::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("post {id}")))?;
    let locale = existing.locale.clone();

    let group = match (payload.join, payload.detach) {
        (Some(target_id), _) => {
            let target = post::Entity::find_by_id(target_id)
                .one(db)
                .await?
                .ok_or_else(|| AppError::Validation("post to join not found".to_string()))?;

            if post::Entity::find()
                .filter(post::Column::TranslationGroupId.eq(target.translation_group_id))
                .filter(post::Column::Locale.eq(&locale))
                .filter(post::Column::Id.ne(id))
                .one(db)
                .await?
                .is_some()
            {
                return Err(AppError::Conflict(format!(
                    "that group already has a {locale} version"
                )));
            }
            target.translation_group_id
        }
        (None, true) => Uuid::now_v7(),
        (None, false) => {
            return Err(AppError::Validation(
                "provide either join or detach".to_string(),
            ));
        }
    };

    let mut model: post::ActiveModel = existing.into();
    model.translation_group_id = Set(group);
    let saved = model.update(db).await?;

    let category_ids = category_ids_for(db, id).await?;
    let translations = siblings_of(db, &saved).await?;
    let mut locales: Vec<String> = translations.iter().map(|t| t.locale.clone()).collect();
    locales.push(saved.locale.clone());

    Ok(Json(PostResponse::from_model(
        saved,
        category_ids,
        locales,
        translations,
    )))
}

/// Delete a post.
#[utoipa::path(
    delete,
    path = "/posts/{id}",
    tag = "posts",
    params(("id" = Uuid, Path, description = "Post id")),
    responses(
        (status = 204, description = "Post deleted"),
        (status = 404, description = "Post not found", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_post(
    Site(state): Site,
    user: axum::Extension<crate::entities::user::Model>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();
    let existing = post::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("post {id}")))?;

    // The rows that only exist because this post does. They go with it and
    // come back with it; leaving them behind would restore a post with no
    // categories and no tags, which is not the post that was deleted.
    let categories: Vec<Uuid> = post_category::Entity::find()
        .filter(post_category::Column::PostId.eq(id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.category_id)
        .collect();
    let tags: Vec<Uuid> = post_tag::Entity::find()
        .filter(post_tag::Column::PostId.eq(id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.tag_id)
        .collect();

    // Its own kind, whatever the site calls it. A course in the bin listed as
    // "post" is a small lie, and the one that matters is the count that stops
    // a kind being removed while there is still something of it in here.
    let kind = existing.kind.clone();
    crate::trash::keep(
        db,
        &kind,
        id,
        &existing.title,
        serde_json::json!({
            "post": existing,
            "categories": categories,
            "tags": tags,
        }),
        &user.username,
    )
    .await?;

    post::Entity::delete_by_id(id).exec(db).await?;

    // Deliberately not dropping the images the post used. They are what the
    // post looked like, and a restore that hands back the words without them
    // is not the post coming back. They go when the bin is emptied.

    Ok(StatusCode::NO_CONTENT)
}

/// Every media address the post pointed at: its cover and everything in its
/// body.
pub(crate) fn referenced_media(post: &post::Model) -> Vec<String> {
    let mut urls: Vec<String> = MEDIA_IN_HTML
        .captures_iter(&post.content_html)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect();
    if !post.cover_url.trim().is_empty() {
        urls.push(post.cover_url.trim().to_string());
    }
    urls.sort();
    urls.dedup();
    urls
}

#[cfg(test)]
mod tests {
    use super::{PostStatus, page, scheduled_needs_a_time};
    use crate::dto::taxonomy::LocaleQuery;
    use crate::entities::post;
    use chrono::Utc;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database, DatabaseConnection};
    use uuid::Uuid;

    async fn site_with_posts() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        for (title, status) in [
            ("Yayinda", "published"),
            ("Yayinda ikinci", "published"),
            ("Taslak", "draft"),
            ("Sirada", "scheduled"),
        ] {
            post::ActiveModel {
                id: Set(Uuid::now_v7()),
                title: Set(title.to_string()),
                slug: Set(title.to_lowercase().replace(' ', "-")),
                status: Set(status.to_string()),
                kind: Set(crate::content::POST.to_string()),
                locale: Set("tr".to_string()),
                translation_group_id: Set(Uuid::now_v7()),
                excerpt: Set(String::new()),
                author: Set(String::new()),
                category: Set(String::new()),
                tags: Set(serde_json::json!([])),
                cover_url: Set(String::new()),
                seo_title: Set(String::new()),
                seo_description: Set(String::new()),
                canonical: Set(String::new()),
                featured: Set(false),
                allow_comments: Set(true),
                content_html: Set(String::new()),
                content_markdown: Set(None),
                fields: Set(String::new()),
                publish_at: Set(None),
                created_at: Set(Utc::now().fixed_offset()),
                updated_at: Set(Utc::now().fixed_offset()),
            }
            .insert(&db)
            .await
            .unwrap();
        }
        db
    }

    fn asking(status: Option<&str>) -> LocaleQuery {
        LocaleQuery {
            status: status.map(str::to_string),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn saying_nothing_asks_for_what_is_published() {
        // The other way round for years. A build that forgets to say put every
        // draft on the internet, and forgetting is the normal case.
        let db = site_with_posts().await;
        let found = page(&db, &asking(None)).await.unwrap();

        assert_eq!(found.total, 2);
        assert!(
            found
                .items
                .iter()
                .all(|p| p.status == PostStatus::Published)
        );
    }

    #[tokio::test]
    async fn what_is_not_published_is_there_for_the_asking() {
        let db = site_with_posts().await;
        let found = page(&db, &asking(Some("draft,scheduled"))).await.unwrap();
        assert_eq!(found.total, 2);
    }

    #[tokio::test]
    async fn the_tally_counts_the_archive_and_not_the_filter() {
        // The screen that says how many drafts there are asks for drafts. If
        // the tally came from the same filter it would answer "one draft, no
        // published" and the other tabs would read zero.
        let db = site_with_posts().await;
        let found = page(&db, &asking(Some("draft"))).await.unwrap();

        assert_eq!(found.total, 1);
        assert_eq!(found.counts.draft, 1);
        assert_eq!(found.counts.published, 2);
        assert_eq!(found.counts.scheduled, 1);
    }

    #[test]
    fn a_schedule_needs_a_time() {
        assert!(scheduled_needs_a_time(PostStatus::Scheduled.as_str(), false).is_err());
        assert!(scheduled_needs_a_time(PostStatus::Scheduled.as_str(), true).is_ok());
    }

    /// A draft is where an unfinished thought lives, date or no date.
    #[test]
    fn nothing_else_needs_one() {
        for status in [PostStatus::Draft, PostStatus::Review, PostStatus::Published] {
            assert!(scheduled_needs_a_time(status.as_str(), false).is_ok());
        }
    }
}
