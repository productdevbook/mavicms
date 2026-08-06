use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use serde::Deserialize;

use crate::{
    dto::languages::{
        CreateLanguageRequest, LanguageResponse, LanguageUsage, UpdateLanguageRequest,
    },
    entities::{category, language, post, tag},
    error::{AppError, AppResult},
    languages::{normalize_code, validate_code, well_known_name},
    tenants::Site,
};

fn validate_direction(value: &str) -> AppResult<String> {
    match value {
        "ltr" | "rtl" => Ok(value.to_string()),
        _ => Err(AppError::Validation(
            "direction must be ltr or rtl".to_string(),
        )),
    }
}

/// List the content languages.
#[utoipa::path(
    get,
    path = "/languages",
    tag = "languages",
    responses((status = 200, description = "Languages", body = Vec<LanguageResponse>))
)]
pub async fn list_languages(Site(state): Site) -> AppResult<Json<Vec<LanguageResponse>>> {
    let rows = crate::languages::all(state.db()).await?;
    Ok(Json(rows.into_iter().map(LanguageResponse::from).collect()))
}

/// Add a content language.
#[utoipa::path(
    post,
    path = "/languages",
    tag = "languages",
    request_body = CreateLanguageRequest,
    responses(
        (status = 201, description = "Language added", body = LanguageResponse),
        (status = 409, description = "Already exists", body = crate::error::ErrorBody),
    )
)]
pub async fn create_language(
    Site(state): Site,
    Json(payload): Json<CreateLanguageRequest>,
) -> AppResult<(StatusCode, Json<LanguageResponse>)> {
    let db = state.db();
    let code = normalize_code(&payload.code);
    validate_code(&code)?;

    if language::Entity::find_by_id(code.clone())
        .one(db)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(format!("{code} already exists")));
    }

    let (fallback_name, fallback_native) = well_known_name(&code);
    let name = if payload.name.trim().is_empty() {
        fallback_name.to_string()
    } else {
        payload.name.trim().to_string()
    };
    let native_name = if payload.native_name.trim().is_empty() {
        fallback_native.to_string()
    } else {
        payload.native_name.trim().to_string()
    };
    let direction = validate_direction(payload.direction.as_deref().unwrap_or("ltr"))?;

    let saved = language::ActiveModel {
        code: Set(code),
        name: Set(name),
        native_name: Set(native_name),
        direction: Set(direction),
        // A newly added language never displaces the current default.
        is_default: Set(false),
        is_active: Set(true),
        sort_order: Set(payload.sort_order),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok((StatusCode::CREATED, Json(saved.into())))
}

/// Update a language. The code itself cannot change.
#[utoipa::path(
    put,
    path = "/languages/{code}",
    tag = "languages",
    params(("code" = String, Path, description = "Language code")),
    request_body = UpdateLanguageRequest,
    responses(
        (status = 200, description = "Language updated", body = LanguageResponse),
        (status = 404, description = "Not found", body = crate::error::ErrorBody),
    )
)]
pub async fn update_language(
    Site(state): Site,
    Path(code): Path<String>,
    Json(payload): Json<UpdateLanguageRequest>,
) -> AppResult<Json<LanguageResponse>> {
    let db = state.db();
    let code = normalize_code(&code);
    let existing = language::Entity::find_by_id(code.clone())
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("language {code}")))?;

    let was_default = existing.is_default;
    let mut model: language::ActiveModel = existing.into();

    if let Some(name) = payload.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation("name must not be empty".to_string()));
        }
        model.name = Set(name.trim().to_string());
    }
    if let Some(native_name) = payload.native_name {
        model.native_name = Set(native_name.trim().to_string());
    }
    if let Some(direction) = payload.direction {
        model.direction = Set(validate_direction(&direction)?);
    }
    if let Some(sort_order) = payload.sort_order {
        model.sort_order = Set(sort_order);
    }
    if let Some(is_active) = payload.is_active {
        if !is_active && was_default {
            return Err(AppError::Validation(
                "the default language cannot be deactivated".to_string(),
            ));
        }
        model.is_active = Set(is_active);
    }

    let txn = db.begin().await?;

    // Exactly one default, enforced here rather than by a partial unique index
    // because MySQL 8 has none.
    if payload.is_default == Some(true) {
        language::Entity::update_many()
            .col_expr(language::Column::IsDefault, false.into())
            .filter(language::Column::IsDefault.eq(true))
            .exec(&txn)
            .await?;
        model.is_default = Set(true);
        model.is_active = Set(true);
    } else if payload.is_default == Some(false) && was_default {
        return Err(AppError::Validation(
            "promote another language instead of clearing the default".to_string(),
        ));
    }

    let saved = model.update(&txn).await?;
    txn.commit().await?;

    Ok(Json(saved.into()))
}

#[derive(Debug, Deserialize)]
pub struct DeleteLanguageQuery {
    /// `reassign` (needs `to`) or `delete_content`. Without it a language that
    /// still holds content is refused.
    #[serde(default)]
    pub force: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

/// Remove a language. Refuses while it still holds content unless explicitly
/// forced, and never removes the default or the last language.
#[utoipa::path(
    delete,
    path = "/languages/{code}",
    tag = "languages",
    params(("code" = String, Path, description = "Language code")),
    responses(
        (status = 204, description = "Language removed"),
        (status = 409, description = "Still in use", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_language(
    Site(state): Site,
    Path(code): Path<String>,
    Query(query): Query<DeleteLanguageQuery>,
) -> AppResult<StatusCode> {
    let db = state.db();
    let code = normalize_code(&code);
    let existing = language::Entity::find_by_id(code.clone())
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("language {code}")))?;

    if existing.is_default {
        return Err(AppError::Conflict(
            "promote another language before removing this one".to_string(),
        ));
    }
    if language::Entity::find().count(db).await? <= 1 {
        return Err(AppError::Conflict(
            "the last language cannot be removed".to_string(),
        ));
    }

    let usage = LanguageUsage {
        posts: post::Entity::find()
            .filter(post::Column::Locale.eq(&code))
            .count(db)
            .await?,
        categories: category::Entity::find()
            .filter(category::Column::Locale.eq(&code))
            .count(db)
            .await?,
        tags: tag::Entity::find()
            .filter(tag::Column::Locale.eq(&code))
            .count(db)
            .await?,
    };
    let in_use = usage.posts + usage.categories + usage.tags > 0;

    let txn = db.begin().await?;

    match query.force.as_deref() {
        _ if !in_use => {}
        Some("reassign") => {
            let target = normalize_code(query.to.as_deref().unwrap_or_default());
            if target.is_empty() || target == code {
                return Err(AppError::Validation(
                    "reassign needs a different target language".to_string(),
                ));
            }
            if language::Entity::find_by_id(target.clone())
                .one(db)
                .await?
                .is_none()
            {
                return Err(AppError::Validation(format!("unknown language: {target}")));
            }

            post::Entity::update_many()
                .col_expr(post::Column::Locale, target.clone().into())
                .filter(post::Column::Locale.eq(&code))
                .exec(&txn)
                .await?;
            category::Entity::update_many()
                .col_expr(category::Column::Locale, target.clone().into())
                .filter(category::Column::Locale.eq(&code))
                .exec(&txn)
                .await?;
            tag::Entity::update_many()
                .col_expr(tag::Column::Locale, target.into())
                .filter(tag::Column::Locale.eq(&code))
                .exec(&txn)
                .await?;
        }
        Some("delete_content") => {
            // Join rows go with their posts/categories via ON DELETE CASCADE.
            // Media is deliberately untouched: files are language-neutral.
            post::Entity::delete_many()
                .filter(post::Column::Locale.eq(&code))
                .exec(&txn)
                .await?;
            category::Entity::delete_many()
                .filter(category::Column::Locale.eq(&code))
                .exec(&txn)
                .await?;
            tag::Entity::delete_many()
                .filter(tag::Column::Locale.eq(&code))
                .exec(&txn)
                .await?;
        }
        _ => {
            return Err(AppError::Conflict(format!(
                "{code} still has {} posts, {} categories and {} tags",
                usage.posts, usage.categories, usage.tags
            )));
        }
    }

    language::Entity::delete_by_id(code).exec(&txn).await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}
