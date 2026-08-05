use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::{
    entities::language,
    error::{AppError, AppResult},
};

/// Canonical form for a BCP-47 tag: lowercase language, uppercase region
/// ("pt-BR"). MySQL's default collation is case-insensitive while Postgres and
/// SQLite are not, so codes are normalized on the way in rather than relying on
/// the database to consider "EN" and "en" the same.
pub fn normalize_code(input: &str) -> String {
    let trimmed = input.trim().replace('_', "-");
    let mut parts = trimmed.split('-');

    let Some(primary) = parts.next().filter(|p| !p.is_empty()) else {
        return String::new();
    };

    let mut out = primary.to_lowercase();
    for part in parts {
        out.push('-');
        if part.len() == 2 {
            out.push_str(&part.to_uppercase());
        } else {
            out.push_str(&part.to_lowercase());
        }
    }
    out
}

pub fn validate_code(code: &str) -> AppResult<()> {
    if code.is_empty() || code.len() > 35 {
        return Err(AppError::Validation(
            "language code must be 1-35 characters".to_string(),
        ));
    }
    if !code
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::Validation(
            "language code may only contain letters, digits and -".to_string(),
        ));
    }
    Ok(())
}

pub async fn all(db: &impl ConnectionTrait) -> AppResult<Vec<language::Model>> {
    Ok(language::Entity::find()
        .order_by_asc(language::Column::SortOrder)
        .order_by_asc(language::Column::Code)
        .all(db)
        .await?)
}

/// The locale new content gets when the client doesn't say. Falls back to
/// "en" only if the table is somehow empty (it is seeded at setup).
pub async fn default_code(db: &impl ConnectionTrait) -> AppResult<String> {
    Ok(language::Entity::find()
        .filter(language::Column::IsDefault.eq(true))
        .one(db)
        .await?
        .map(|row| row.code)
        .unwrap_or_else(|| "en".to_string()))
}

/// Resolves a caller-supplied locale, rejecting unknown or inactive ones so a
/// typo can't silently create content in a language that doesn't exist.
pub async fn resolve(db: &impl ConnectionTrait, requested: Option<&str>) -> AppResult<String> {
    let Some(requested) = requested.map(normalize_code).filter(|c| !c.is_empty()) else {
        return default_code(db).await;
    };

    let found = language::Entity::find_by_id(requested.clone())
        .one(db)
        .await?
        .ok_or_else(|| AppError::Validation(format!("unknown language: {requested}")))?;

    if !found.is_active {
        return Err(AppError::Validation(format!(
            "language {requested} is not active"
        )));
    }
    Ok(found.code)
}

/// Adds the first language during first-run setup. The migration seeds this
/// for existing installs, but on a fresh database it runs before
/// `site_settings` exists, so setup has to do it.
pub async fn ensure_seeded(db: &impl ConnectionTrait, locale: &str) -> AppResult<()> {
    if language::Entity::find().one(db).await?.is_some() {
        return Ok(());
    }

    let code = normalize_code(locale);
    let code = if code.is_empty() {
        "en".to_string()
    } else {
        code
    };
    let (name, native_name) = well_known_name(&code);

    language::ActiveModel {
        code: Set(code),
        name: Set(name.to_string()),
        native_name: Set(native_name.to_string()),
        direction: Set("ltr".to_string()),
        is_default: Set(true),
        is_active: Set(true),
        sort_order: Set(0),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(())
}

/// Names for the languages the panel itself ships in, so first-run setup
/// doesn't leave a row labelled "Language". Anything else is named by the admin.
pub fn well_known_name(code: &str) -> (&'static str, &'static str) {
    match code.split('-').next().unwrap_or("") {
        "tr" => ("Turkish", "Türkçe"),
        "en" => ("English", "English"),
        "de" => ("German", "Deutsch"),
        "fr" => ("French", "Français"),
        "es" => ("Spanish", "Español"),
        "ar" => ("Arabic", "العربية"),
        _ => ("Language", "Language"),
    }
}
