use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::{
    dto::taxonomy::{CreateTagRequest, LocaleQuery, TagResponse},
    entities::tag,
    error::{AppError, AppResult},
    languages::resolve,
    routes::posts::TranslationGroupRequest,
    slug::{slugify, slugify_or},
    tenants::Site,
};

/// Finds a tag by name **within one language**, or creates one. Returns
/// `(tag, created)`.
///
/// Two deliberate choices:
/// - Lookup is by name, not slug: `slugify_or` appends a random suffix for
///   names with nothing sluggable in them, so a slug lookup would miss and
///   every save would add another row.
/// - A new tag is never auto-grouped with a same-spelled tag in another
///   language. "rust" typed on a German post is a German tag, not the
///   translation of the English one (that would be "Rost") — and silently
///   merging EN "gift" with DE "Gift" (poison) is the kind of mistake nobody
///   discovers. Grouping only happens by explicit action.
pub async fn get_or_create_tag(
    db: &impl ConnectionTrait,
    name: &str,
    locale: &str,
) -> AppResult<(tag::Model, bool)> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("name must not be empty".to_string()));
    }

    // Identity is the slug, because that is what the database enforces. Going
    // by name instead let "Diyet" and "diyet" miss each other on a
    // case-sensitive database and then collide on the very same slug, which
    // surfaced as an unexplained conflict partway through an import.
    let slug = slugify(name);
    let mut find = tag::Entity::find().filter(tag::Column::Locale.eq(locale));
    find = if slug.is_empty() {
        // Nothing sluggable in the name — its slug carries a random suffix, so
        // the name is all there is to match on.
        find.filter(tag::Column::Name.eq(name))
    } else {
        find.filter(tag::Column::Slug.eq(&slug))
    };
    if let Some(existing) = find.one(db).await? {
        return Ok((existing, false));
    }

    let id = Uuid::now_v7();
    let model = tag::ActiveModel {
        id: Set(id),
        name: Set(name.to_string()),
        slug: Set(if slug.is_empty() {
            slugify_or(name, "tag")
        } else {
            slug
        }),
        locale: Set(locale.to_string()),
        translation_group_id: Set(id),
        translation_status: Set("complete".to_string()),
        created_at: Set(Utc::now().fixed_offset()),
    };
    Ok((model.insert(db).await?, true))
}

/// List tags, optionally filtered by language.
#[utoipa::path(
    get,
    path = "/tags",
    tag = "taxonomy",
    params(("locale" = Option<String>, Query, description = "Comma-separated language codes")),
    responses((status = 200, description = "List of tags", body = Vec<TagResponse>))
)]
pub async fn list_tags(
    Site(state): Site,
    Query(query): Query<LocaleQuery>,
) -> AppResult<Json<Vec<TagResponse>>> {
    let mut find = tag::Entity::find();
    if let Some(codes) = query.codes() {
        find = find.filter(tag::Column::Locale.is_in(codes));
    }
    let tags = find.all(state.db()).await?;
    Ok(Json(tags.into_iter().map(TagResponse::from).collect()))
}

/// Create a tag, or return the existing one with that name in that language.
#[utoipa::path(
    post,
    path = "/tags",
    tag = "taxonomy",
    request_body = CreateTagRequest,
    responses(
        (status = 200, description = "Existing tag returned", body = TagResponse),
        (status = 201, description = "Tag created", body = TagResponse),
    )
)]
pub async fn create_tag(
    Site(state): Site,
    Json(payload): Json<CreateTagRequest>,
) -> AppResult<(StatusCode, Json<TagResponse>)> {
    let db = state.db();
    let locale = resolve(db, payload.locale.as_deref()).await?;
    let (mut tag, created) = get_or_create_tag(db, &payload.name, &locale).await?;

    // Explicit linking: the client names a sibling tag and the server takes
    // that row's group.
    if let Some(sibling_id) = payload.translation_of {
        let sibling = tag::Entity::find_by_id(sibling_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Validation("translation_of tag not found".to_string()))?;

        if sibling.locale == tag.locale {
            return Err(AppError::Validation(
                "a tag cannot be a translation of another tag in the same language".to_string(),
            ));
        }

        let mut model: tag::ActiveModel = tag.into();
        model.translation_group_id = Set(sibling.translation_group_id);
        tag = model.update(db).await?;
    }

    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(tag.into())))
}

/// Delete a tag.
#[utoipa::path(
    delete,
    path = "/tags/{id}",
    tag = "taxonomy",
    params(("id" = Uuid, Path, description = "Tag id")),
    responses(
        (status = 204, description = "Tag deleted"),
        (status = 404, description = "Tag not found", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_tag(
    Site(state): Site,
    user: axum::Extension<crate::entities::user::Model>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();
    let existing = tag::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("tag {id}")))?;

    let posts = crate::entities::post_tag::Entity::find()
        .filter(crate::entities::post_tag::Column::TagId.eq(id))
        .all(db)
        .await?;

    crate::trash::keep(
        db,
        crate::trash::TAG,
        id,
        &existing.name,
        serde_json::json!({ "tag": existing, "posts": posts }),
        &user.username,
    )
    .await?;

    tag::Entity::delete_by_id(id).exec(db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Join a tag to another tag's translation group, or split it into its own.
#[utoipa::path(
    patch,
    path = "/tags/{id}/translation-group",
    tag = "taxonomy",
    params(("id" = Uuid, Path, description = "Tag id")),
    request_body = TranslationGroupRequest,
    responses(
        (status = 200, description = "Tag regrouped", body = TagResponse),
        (status = 404, description = "Tag not found", body = crate::error::ErrorBody),
        (status = 409, description = "Group already has this language", body = crate::error::ErrorBody),
    )
)]
pub async fn set_tag_translation_group(
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<TranslationGroupRequest>,
) -> AppResult<Json<TagResponse>> {
    let db = state.db();
    let existing = tag::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("tag {id}")))?;
    let locale = existing.locale.clone();

    let group = match (payload.join, payload.detach) {
        (Some(target_id), _) => {
            let target = tag::Entity::find_by_id(target_id)
                .one(db)
                .await?
                .ok_or_else(|| AppError::Validation("tag to join not found".to_string()))?;

            if tag::Entity::find()
                .filter(tag::Column::TranslationGroupId.eq(target.translation_group_id))
                .filter(tag::Column::Locale.eq(&locale))
                .filter(tag::Column::Id.ne(id))
                .one(db)
                .await?
                .is_some()
            {
                return Err(AppError::Conflict(format!(
                    "that group already has a {locale} tag"
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

    let mut model: tag::ActiveModel = existing.into();
    model.translation_group_id = Set(group);
    Ok(Json(model.update(db).await?.into()))
}
