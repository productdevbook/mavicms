use axum::{Json, extract::State, http::StatusCode};

use crate::{
    dto::plugins::{
        ConnectionTestResponse, PluginSummary, S3SettingsRequest, S3SettingsResponse,
    },
    error::AppResult,
    plugins::{S3_PLUGIN, load_s3, save_s3},
    state::AppState,
    storage::S3Config,
};

/// List the built-in integrations and whether each is switched on.
#[utoipa::path(
    get,
    path = "/plugins",
    tag = "plugins",
    responses((status = 200, description = "Available plugins", body = Vec<PluginSummary>))
)]
pub async fn list_plugins(State(state): State<AppState>) -> AppResult<Json<Vec<PluginSummary>>> {
    let stored = load_s3(state.db(), &state.secrets).await.ok().flatten();

    Ok(Json(vec![PluginSummary {
        id: S3_PLUGIN.to_string(),
        name: "S3 compatible storage".to_string(),
        description:
            "Store uploaded media in an S3 bucket (AWS S3, Cloudflare R2, MinIO, DigitalOcean Spaces) instead of the local disk."
                .to_string(),
        enabled: stored.as_ref().is_some_and(|s| s.enabled),
        configured: stored.is_some(),
    }]))
}

/// Current S3 settings. The secret access key is never returned.
#[utoipa::path(
    get,
    path = "/plugins/s3",
    tag = "plugins",
    responses((status = 200, description = "S3 settings", body = S3SettingsResponse))
)]
pub async fn get_s3_settings(
    State(state): State<AppState>,
) -> AppResult<Json<S3SettingsResponse>> {
    let stored = load_s3(state.db(), &state.secrets).await?;

    let response = match stored {
        Some(stored) => S3SettingsResponse {
            enabled: stored.enabled,
            endpoint: stored.config.endpoint,
            region: stored.config.region,
            bucket: stored.config.bucket,
            access_key_id: stored.config.access_key_id,
            public_base_url: stored.config.public_base_url,
            path_prefix: stored.config.path_prefix,
            has_secret_access_key: !stored.config.secret_access_key.is_empty(),
        },
        None => S3SettingsResponse {
            enabled: false,
            endpoint: String::new(),
            region: String::new(),
            bucket: String::new(),
            access_key_id: String::new(),
            public_base_url: String::new(),
            path_prefix: String::new(),
            has_secret_access_key: false,
        },
    };

    Ok(Json(response))
}

/// Merges the request over the stored config, keeping the existing secret when
/// the field was left blank.
async fn resolve_config(
    state: &AppState,
    payload: &S3SettingsRequest,
) -> AppResult<S3Config> {
    let stored = load_s3(state.db(), &state.secrets).await?;
    let secret = match payload.secret_access_key.as_deref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => stored
            .map(|s| s.config.secret_access_key)
            .unwrap_or_default(),
    };

    Ok(S3Config {
        endpoint: payload.endpoint.trim().to_string(),
        region: payload.region.trim().to_string(),
        bucket: payload.bucket.trim().to_string(),
        access_key_id: payload.access_key_id.trim().to_string(),
        secret_access_key: secret,
        public_base_url: payload.public_base_url.trim().to_string(),
        path_prefix: payload.path_prefix.trim().to_string(),
    })
}

/// Save S3 settings. Enabling requires a complete, valid configuration.
#[utoipa::path(
    put,
    path = "/plugins/s3",
    tag = "plugins",
    request_body = S3SettingsRequest,
    responses(
        (status = 200, description = "Settings saved", body = S3SettingsResponse),
        (status = 400, description = "Invalid configuration", body = crate::error::ErrorBody),
    )
)]
pub async fn update_s3_settings(
    State(state): State<AppState>,
    Json(payload): Json<S3SettingsRequest>,
) -> AppResult<Json<S3SettingsResponse>> {
    let config = resolve_config(&state, &payload).await?;
    if payload.enabled {
        config.validate()?;
    }

    save_s3(state.db(), &state.secrets, payload.enabled, &config).await?;
    get_s3_settings(State(state)).await
}

/// Try the given (or stored) credentials by writing and deleting a small
/// object, so write permission is verified too.
#[utoipa::path(
    post,
    path = "/plugins/s3/test",
    tag = "plugins",
    request_body = S3SettingsRequest,
    responses((status = 200, description = "Test result", body = ConnectionTestResponse))
)]
pub async fn test_s3_settings(
    State(state): State<AppState>,
    Json(payload): Json<S3SettingsRequest>,
) -> AppResult<(StatusCode, Json<ConnectionTestResponse>)> {
    let config = resolve_config(&state, &payload).await?;

    let result = match config.validate() {
        Ok(()) => config.test_connection().await,
        Err(err) => Err(err),
    };

    // A failed connection is a valid answer to "does this work?", so it is
    // reported in the body rather than as an HTTP error.
    let response = match result {
        Ok(()) => ConnectionTestResponse {
            ok: true,
            message: "Connection succeeded.".to_string(),
        },
        // Surfaced verbatim (minus the error-kind prefix) — the underlying S3
        // message is what tells an admin *why* their bucket config is wrong.
        Err(crate::error::AppError::Validation(message)) => {
            ConnectionTestResponse { ok: false, message }
        }
        Err(err) => ConnectionTestResponse {
            ok: false,
            message: err.to_string(),
        },
    };

    Ok((StatusCode::OK, Json(response)))
}
