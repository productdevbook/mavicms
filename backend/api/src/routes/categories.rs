use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::{
    dto::taxonomy::{CategoryResponse, CreateCategoryRequest, LocaleQuery, UpdateCategoryRequest},
    entities::category,
    error::{AppError, AppResult},
    languages::resolve,
    slug::slugify_or,
    state::AppState,
};

/// Rejects a parent that would create a loop. Walking up from the proposed
/// parent must never reach the category being edited — checking only for
/// direct self-parenting misses A→B→A, which then makes the tree unwalkable.
async fn ensure_no_cycle(
    db: &impl ConnectionTrait,
    id: Uuid,
    proposed_parent: Uuid,
) -> AppResult<()> {
    let mut cursor = Some(proposed_parent);
    // Bounded so a pre-existing loop in the data can't spin forever.
    for _ in 0..64 {
        let Some(current) = cursor else { return Ok(()) };
        if current == id {
            return Err(AppError::Validation(
                "that parent would create a loop".to_string(),
            ));
        }
        cursor = category::Entity::find_by_id(current)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Validation("parent category not found".to_string()))?
            .parent_id;
    }

    Err(AppError::Validation(
        "category hierarchy is too deep".to_string(),
    ))
}

/// Returns the version of `category` that belongs to `locale`, creating a stub
/// in that language if there isn't one yet.
///
/// Posts may only link categories in their own language — otherwise "how many
/// German posts in Technology" has no answer. But rejecting a foreign-locale
/// category outright would leave an admin who just added a language staring at
/// an empty dropdown, so the write path resolves instead of refusing, and the
/// stub is flagged `needs_translation` for the panel to surface.
pub async fn resolve_for_locale(
    db: &impl ConnectionTrait,
    category: &category::Model,
    locale: &str,
) -> AppResult<category::Model> {
    if category.locale == locale {
        return Ok(category.clone());
    }

    if let Some(sibling) = category::Entity::find()
        .filter(category::Column::TranslationGroupId.eq(category.translation_group_id))
        .filter(category::Column::Locale.eq(locale))
        .one(db)
        .await?
    {
        return Ok(sibling);
    }

    // The parent has to exist in this language too, or the stub would point at
    // a category from another language.
    let parent_id = match category.parent_id {
        Some(parent_id) => match category::Entity::find_by_id(parent_id).one(db).await? {
            Some(parent) => Some(Box::pin(resolve_for_locale(db, &parent, locale)).await?.id),
            None => None,
        },
        None => None,
    };

    let stub = category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(category.name.clone()),
        slug: Set(slugify_or(&category.name, "category")),
        parent_id: Set(parent_id),
        description: Set(String::new()),
        locale: Set(locale.to_string()),
        translation_group_id: Set(category.translation_group_id),
        translation_status: Set("needs_translation".to_string()),
        created_at: Set(Utc::now().fixed_offset()),
    };
    Ok(stub.insert(db).await?)
}

/// List categories, optionally filtered by language.
#[utoipa::path(
    get,
    path = "/categories",
    tag = "taxonomy",
    params(("locale" = Option<String>, Query, description = "Comma-separated language codes")),
    responses((status = 200, description = "List of categories", body = Vec<CategoryResponse>))
)]
pub async fn list_categories(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> AppResult<Json<Vec<CategoryResponse>>> {
    let mut find = category::Entity::find();
    if let Some(codes) = query.codes() {
        find = find.filter(category::Column::Locale.is_in(codes));
    }
    let categories = find.all(state.db()).await?;
    Ok(Json(
        categories.into_iter().map(CategoryResponse::from).collect(),
    ))
}

/// Create a category.
#[utoipa::path(
    post,
    path = "/categories",
    tag = "taxonomy",
    request_body = CreateCategoryRequest,
    responses(
        (status = 201, description = "Category created", body = CategoryResponse),
        (status = 409, description = "A category with this name already exists", body = crate::error::ErrorBody),
    )
)]
pub async fn create_category(
    State(state): State<AppState>,
    Json(payload): Json<CreateCategoryRequest>,
) -> AppResult<(StatusCode, Json<CategoryResponse>)> {
    let db = state.db();
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("name must not be empty".to_string()));
    }

    let locale = resolve(db, payload.locale.as_deref()).await?;

    let mut translation_group_id = Uuid::new_v4();
    if let Some(sibling_id) = payload.translation_of {
        let sibling = category::Entity::find_by_id(sibling_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Validation("translation_of category not found".to_string()))?;
        if sibling.locale == locale {
            return Err(AppError::Validation(
                "a category cannot be a translation of another in the same language".to_string(),
            ));
        }
        translation_group_id = sibling.translation_group_id;
    }

    if let Some(parent_id) = payload.parent_id {
        let parent = category::Entity::find_by_id(parent_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Validation("parent category not found".to_string()))?;
        if parent.locale != locale {
            return Err(AppError::Validation(
                "the parent category must be in the same language".to_string(),
            ));
        }
    }

    let slug = slugify_or(name, "category");
    if category::Entity::find()
        .filter(category::Column::Locale.eq(&locale))
        .filter(category::Column::Slug.eq(&slug))
        .one(db)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(
            "a category with this name already exists".to_string(),
        ));
    }

    let model = category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name.to_string()),
        slug: Set(slug),
        parent_id: Set(payload.parent_id),
        description: Set(payload.description),
        locale: Set(locale),
        translation_group_id: Set(translation_group_id),
        translation_status: Set("complete".to_string()),
        created_at: Set(Utc::now().fixed_offset()),
    };

    let saved = model.insert(db).await?;
    Ok((StatusCode::CREATED, Json(saved.into())))
}

/// Update a category.
#[utoipa::path(
    put,
    path = "/categories/{id}",
    tag = "taxonomy",
    params(("id" = Uuid, Path, description = "Category id")),
    request_body = UpdateCategoryRequest,
    responses(
        (status = 200, description = "Category updated", body = CategoryResponse),
        (status = 404, description = "Category not found", body = crate::error::ErrorBody),
    )
)]
pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateCategoryRequest>,
) -> AppResult<Json<CategoryResponse>> {
    let db = state.db();
    let existing = category::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("category {id}")))?;
    let locale = existing.locale.clone();

    let mut model: category::ActiveModel = existing.into();

    if let Some(name) = payload.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::Validation("name must not be empty".to_string()));
        }
        let slug = slugify_or(name, "category");
        if category::Entity::find()
            .filter(category::Column::Locale.eq(&locale))
            .filter(category::Column::Slug.eq(&slug))
            .filter(category::Column::Id.ne(id))
            .one(db)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict(
                "a category with this name already exists".to_string(),
            ));
        }
        model.slug = Set(slug);
        model.name = Set(name.to_string());
        // Renaming a stub is exactly the act of translating it.
        model.translation_status = Set("complete".to_string());
    }
    if let Some(parent_id) = payload.parent_id {
        if let Some(parent_id) = parent_id {
            let parent = category::Entity::find_by_id(parent_id)
                .one(db)
                .await?
                .ok_or_else(|| AppError::Validation("parent category not found".to_string()))?;
            if parent.locale != locale {
                return Err(AppError::Validation(
                    "the parent category must be in the same language".to_string(),
                ));
            }
            ensure_no_cycle(db, id, parent_id).await?;
        }
        model.parent_id = Set(parent_id);
    }
    if let Some(description) = payload.description {
        model.description = Set(description);
    }
    if let Some(status) = payload.translation_status {
        match status.as_str() {
            "complete" | "needs_translation" => model.translation_status = Set(status),
            _ => {
                return Err(AppError::Validation(
                    "translation_status must be complete or needs_translation".to_string(),
                ));
            }
        }
    }

    let saved = model.update(db).await?;
    Ok(Json(saved.into()))
}

/// Delete a category.
#[utoipa::path(
    delete,
    path = "/categories/{id}",
    tag = "taxonomy",
    params(("id" = Uuid, Path, description = "Category id")),
    responses(
        (status = 204, description = "Category deleted"),
        (status = 404, description = "Category not found", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let result = category::Entity::delete_by_id(id).exec(state.db()).await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("category {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}
