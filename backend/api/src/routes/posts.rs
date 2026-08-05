use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    dto::{
        post::{CreatePostRequest, PostResponse, PostStatus, PostTranslation, UpdatePostRequest},
        taxonomy::LocaleQuery,
    },
    entities::{category, post, post_category, post_tag},
    error::{AppError, AppResult},
    languages::resolve,
    routes::{categories::resolve_for_locale, tags::get_or_create_tag},
    state::AppState,
};

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

/// List posts. Every language is returned unless `locale` narrows it —
/// silently defaulting to one language would be a breaking change for API
/// consumers building their own front-ends.
#[utoipa::path(
    get,
    path = "/posts",
    tag = "posts",
    params(("locale" = Option<String>, Query, description = "Comma-separated language codes")),
    responses((status = 200, description = "List of posts", body = Vec<PostResponse>))
)]
pub async fn list_posts(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> AppResult<Json<Vec<PostResponse>>> {
    let db = state.db();

    let mut find = post::Entity::find().order_by_desc(post::Column::CreatedAt);
    if let Some(codes) = query.codes() {
        find = find.filter(post::Column::Locale.is_in(codes));
    }
    let posts = find.all(db).await?;

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
            PostResponse::from_model(post, category_ids, locales, Vec::new())
        })
        .collect();

    Ok(Json(responses))
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
pub async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostResponse>> {
    let db = state.db();
    let post = post::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("post {id}")))?;

    let category_ids = category_ids_for(db, post.id).await?;
    let translations = siblings_of(db, &post).await?;
    let mut locales: Vec<String> = translations.iter().map(|t| t.locale.clone()).collect();
    locales.push(post.locale.clone());

    Ok(Json(PostResponse::from_model(
        post,
        category_ids,
        locales,
        translations,
    )))
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
    State(state): State<AppState>,
    Json(payload): Json<CreatePostRequest>,
) -> AppResult<(StatusCode, Json<PostResponse>)> {
    if payload.title.trim().is_empty() {
        return Err(AppError::Validation("title must not be empty".to_string()));
    }
    if payload.slug.trim().is_empty() {
        return Err(AppError::Validation("slug must not be empty".to_string()));
    }

    let db = state.db();
    let locale = resolve(db, payload.locale.as_deref()).await?;

    let mut translation_group_id = Uuid::new_v4();
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

    let txn = db.begin().await?;
    let now = Utc::now().fixed_offset();
    let id = Uuid::new_v4();

    let model = post::ActiveModel {
        id: Set(id),
        title: Set(payload.title),
        slug: Set(payload.slug),
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
        content_html: Set(payload.content_html),
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

    Ok((
        StatusCode::CREATED,
        Json(PostResponse::from_model(
            saved,
            category_ids,
            locales,
            translations,
        )),
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
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdatePostRequest>,
) -> AppResult<Json<PostResponse>> {
    let db = state.db();
    let existing = post::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("post {id}")))?;
    // A post's language is fixed once created: changing it could collide with
    // a sibling and would silently move the content between languages.
    let locale = existing.locale.clone();

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
    if let Some(content_html) = payload.content_html {
        model.content_html = Set(content_html);
    }
    model.updated_at = Set(Utc::now().fixed_offset());

    let saved = model.update(&txn).await?;

    if let Some(category_ids) = &payload.category_ids {
        sync_categories(&txn, id, &locale, category_ids).await?;
    }
    if let Some(tags) = &payload.tags {
        sync_tags(&txn, id, &locale, tags).await?;
    }
    txn.commit().await?;

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
    State(state): State<AppState>,
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
        (None, true) => Uuid::new_v4(),
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
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let result = post::Entity::delete_by_id(id).exec(state.db()).await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("post {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}
