use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::{
    crypto::SecretBox,
    entities::plugin_setting,
    error::AppResult,
    state::AppState,
    storage::{MediaStorage, S3Config},
};

pub const S3_PLUGIN: &str = "s3_storage";

pub struct StoredPlugin {
    pub enabled: bool,
    pub config: S3Config,
}

/// Reads and decrypts the S3 plugin row. `None` means it has never been saved.
pub async fn load_s3(
    db: &impl ConnectionTrait,
    secrets: &SecretBox,
) -> AppResult<Option<StoredPlugin>> {
    let Some(row) = plugin_setting::Entity::find()
        .filter(plugin_setting::Column::Plugin.eq(S3_PLUGIN))
        .one(db)
        .await?
    else {
        return Ok(None);
    };

    let json = secrets.decrypt(&row.config)?;
    let config: S3Config = serde_json::from_str(&json)
        .map_err(|err| crate::error::AppError::Internal(format!("corrupt plugin config: {err}")))?;

    Ok(Some(StoredPlugin {
        enabled: row.enabled,
        config,
    }))
}

/// Encrypts and upserts the S3 plugin row.
pub async fn save_s3(
    db: &impl ConnectionTrait,
    secrets: &SecretBox,
    enabled: bool,
    config: &S3Config,
) -> AppResult<()> {
    let json = serde_json::to_string(config).map_err(|err| {
        crate::error::AppError::Internal(format!("failed to encode config: {err}"))
    })?;
    let encrypted = secrets.encrypt(&json)?;
    let now = Utc::now().fixed_offset();

    match plugin_setting::Entity::find()
        .filter(plugin_setting::Column::Plugin.eq(S3_PLUGIN))
        .one(db)
        .await?
    {
        Some(existing) => {
            let mut model: plugin_setting::ActiveModel = existing.into();
            model.enabled = Set(enabled);
            model.config = Set(encrypted);
            model.updated_at = Set(now);
            model.update(db).await?;
        }
        None => {
            plugin_setting::ActiveModel {
                id: Set(Uuid::new_v4()),
                plugin: Set(S3_PLUGIN.to_string()),
                enabled: Set(enabled),
                config: Set(encrypted),
                updated_at: Set(now),
            }
            .insert(db)
            .await?;
        }
    }

    Ok(())
}

/// The backend new uploads should go to.
pub async fn active_storage(state: &AppState) -> AppResult<MediaStorage> {
    if let Some(stored) = load_s3(state.db(), &state.secrets).await?
        && stored.enabled
    {
        return Ok(MediaStorage::S3(Box::new(stored.config)));
    }

    Ok(MediaStorage::Local {
        root: state.media_root.clone(),
    })
}

/// The backend a already-stored file lives on, which may differ from the
/// active one (e.g. S3 was switched off after some files went there).
pub async fn storage_for(state: &AppState, backend: &str) -> AppResult<MediaStorage> {
    if backend == crate::storage::S3
        && let Some(stored) = load_s3(state.db(), &state.secrets).await?
    {
        return Ok(MediaStorage::S3(Box::new(stored.config)));
    }

    Ok(MediaStorage::Local {
        root: state.media_root.clone(),
    })
}
