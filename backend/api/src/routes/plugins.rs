use axum::{Json, http::StatusCode};

use crate::{
    dto::plugins::{
        BackupSettingsResponse, ConnectionTestResponse, PluginSummary, S3SettingsRequest,
        S3SettingsResponse, UpdateBackupRequest,
    },
    error::{AppError, AppResult},
    plugins::{S3_PLUGIN, load_s3, save_s3},
    state::AppState,
    storage::S3Config,
    tenants::Site,
};

/// List the built-in integrations and whether each is switched on.
#[utoipa::path(
    get,
    path = "/plugins",
    tag = "plugins",
    responses((status = 200, description = "Available plugins", body = Vec<PluginSummary>))
)]
pub async fn list_plugins(Site(state): Site) -> AppResult<Json<Vec<PluginSummary>>> {
    let stored = load_s3(state.db(), &state.secrets).await.ok().flatten();
    let email = crate::plugins::load::<crate::email::EmailConfig>(
        state.db(),
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await
    .ok()
    .flatten();
    let backup = crate::plugins::load::<crate::backup::BackupConfig>(
        state.db(),
        &state.secrets,
        crate::plugins::BACKUP_PLUGIN,
    )
    .await
    .ok()
    .flatten();

    Ok(Json(vec![
        PluginSummary {
            id: S3_PLUGIN.to_string(),
            name: "S3 compatible storage".to_string(),
            description:
                "Store uploaded media in an S3 bucket (AWS S3, Cloudflare R2, MinIO, DigitalOcean Spaces) instead of the local disk."
                    .to_string(),
            enabled: stored.as_ref().is_some_and(|s| s.enabled),
            configured: stored.is_some(),
        },
        PluginSummary {
            id: crate::plugins::EMAIL_PLUGIN.to_string(),
            name: "Amazon SES".to_string(),
            description:
                "Send mail through Amazon SES — a notification when somebody fills in one of this site's forms."
                    .to_string(),
            enabled: email.as_ref().is_some_and(|s| s.enabled),
            configured: email.is_some(),
        },
        PluginSummary {
            id: crate::plugins::BACKUP_PLUGIN.to_string(),
            name: "Backups".to_string(),
            description:
                "Take the database, and the uploaded files if you want them, into a single archive — on a schedule, to the disk or to your S3 bucket."
                    .to_string(),
            enabled: backup.as_ref().is_some_and(|s| s.enabled),
            configured: backup.is_some(),
        },
    ]))
}

/// Backup settings, what exists, and whether S3 is available as a destination.
#[utoipa::path(
    get,
    path = "/plugins/backup",
    tag = "plugins",
    responses((status = 200, description = "Backup settings", body = BackupSettingsResponse))
)]
pub async fn get_backup_settings(Site(state): Site) -> AppResult<Json<BackupSettingsResponse>> {
    Ok(Json(backup_settings_of(&state).await?))
}

/// The same, for whoever has the site's state rather than the request that
/// resolved it — which is how the console reaches a site it owns.
pub async fn backup_settings_of(state: &AppState) -> AppResult<BackupSettingsResponse> {
    let state = state.clone();
    let (enabled, config) = crate::backup::config(&state).await?;
    let s3 = load_s3(state.db(), &state.secrets).await?;

    Ok(BackupSettingsResponse {
        backups: crate::backup::list(&state, &config)
            .await
            .unwrap_or_default(),
        // Offered as a destination only once the bucket is set up, so the
        // choice cannot be made before it can work.
        s3_available: s3.is_some(),
        s3_bucket: s3.map(|stored| stored.config.bucket),
        enabled,
        config,
    })
}

/// Save the backup settings.
#[utoipa::path(
    put,
    path = "/plugins/backup",
    tag = "plugins",
    request_body = UpdateBackupRequest,
    responses((status = 200, description = "Saved", body = BackupSettingsResponse))
)]
pub async fn update_backup_settings(
    Site(state): Site,
    Json(payload): Json<UpdateBackupRequest>,
) -> AppResult<Json<BackupSettingsResponse>> {
    Ok(Json(save_backup_of(&state, payload).await?))
}

pub async fn save_backup_of(
    state: &AppState,
    payload: UpdateBackupRequest,
) -> AppResult<BackupSettingsResponse> {
    let state = state.clone();
    let (_, existing) = crate::backup::config(&state).await?;

    if matches!(payload.config.destination, crate::backup::Destination::S3)
        && load_s3(state.db(), &state.secrets).await?.is_none()
    {
        return Err(AppError::Validation(
            "set up the S3 plugin before sending backups there".to_string(),
        ));
    }

    let config = crate::backup::BackupConfig {
        // Not the client's to set: these say what happened, not what should.
        last_run_at: existing.last_run_at,
        last_error: existing.last_error,
        ..payload.config
    };
    crate::backup::store(&state, payload.enabled, &config).await?;

    backup_settings_of(&state).await
}

/// Take a backup now.
#[utoipa::path(
    post,
    path = "/plugins/backup/run",
    tag = "plugins",
    responses(
        (status = 200, description = "Backup written", body = crate::backup::BackupFile),
        (status = 400, description = "The backup could not be written", body = crate::error::ErrorBody),
    )
)]
pub async fn run_backup(Site(state): Site) -> AppResult<Json<crate::backup::BackupFile>> {
    Ok(Json(crate::backup::run(&state).await?))
}

/// Delete one archive.
#[utoipa::path(
    delete,
    path = "/plugins/backup/{name}",
    tag = "plugins",
    params(("name" = String, Path, description = "Archive file name")),
    responses((status = 204, description = "Deleted"))
)]
pub async fn delete_backup(
    Site(state): Site,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> AppResult<axum::http::StatusCode> {
    let (_, config) = crate::backup::config(&state).await?;
    crate::backup::delete(&state, &config, &name).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Current S3 settings. The secret access key is never returned.
#[utoipa::path(
    get,
    path = "/plugins/s3",
    tag = "plugins",
    responses((status = 200, description = "S3 settings", body = S3SettingsResponse))
)]
pub async fn get_s3_settings(Site(state): Site) -> AppResult<Json<S3SettingsResponse>> {
    Ok(Json(s3_settings_of(&state).await?))
}

/// The same, for whoever has the site's state rather than the request that
/// resolved it.
pub async fn s3_settings_of(state: &AppState) -> AppResult<S3SettingsResponse> {
    let state = state.clone();
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

    Ok(response)
}

/// Merges the request over the stored config, keeping the existing secret when
/// the field was left blank.
async fn resolve_config(state: &AppState, payload: &S3SettingsRequest) -> AppResult<S3Config> {
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
    Site(state): Site,
    Json(payload): Json<S3SettingsRequest>,
) -> AppResult<Json<S3SettingsResponse>> {
    Ok(Json(save_s3_of(&state, payload).await?))
}

pub async fn save_s3_of(
    state: &AppState,
    payload: S3SettingsRequest,
) -> AppResult<S3SettingsResponse> {
    let config = resolve_config(state, &payload).await?;
    if payload.enabled {
        config.validate()?;
    }

    save_s3(state.db(), &state.secrets, payload.enabled, &config).await?;
    s3_settings_of(state).await
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
    Site(state): Site,
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

/// Put a site back the way an archive has it.
///
/// One of the archives this site has already written, by name. Uploading a
/// file from elsewhere is a separate thing and deliberately not this: an
/// archive that arrived from somewhere else is a file nobody has checked, and
/// this replaces everything the site has.
#[utoipa::path(
    post,
    path = "/plugins/backup/{name}/restore",
    tag = "plugins",
    params(("name" = String, Path, description = "Archive name")),
    responses(
        (status = 200, description = "What was put back", body = crate::backup::RestoreReport),
        (status = 400, description = "Not an archive this can read", body = crate::error::ErrorBody),
        (status = 404, description = "No such archive", body = crate::error::ErrorBody),
    )
)]
pub async fn restore_backup(
    crate::auth::Administrator(who): crate::auth::Administrator,
    Site(state): Site,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> AppResult<Json<crate::backup::RestoreReport>> {
    let (_, config) = crate::backup::config(&state).await?;
    let bytes = crate::backup::read(&state, &config, &name).await?;

    // Worth a line in the log: this replaces every row the site has, and a
    // week later somebody will want to know who did it and when.
    tracing::warn!(by = %who.username, archive = %name, "restoring a backup");

    Ok(Json(crate::backup::restore(&state, &bytes).await?))
}

/// Put a site back from an archive sent with the request.
///
/// This is how a site moves between servers: take a backup there, upload it
/// here. Everything the site has is replaced by what the file holds.
#[utoipa::path(
    post,
    path = "/plugins/backup/import",
    tag = "plugins",
    request_body(content = Vec<u8>, content_type = "application/gzip"),
    responses(
        (status = 200, description = "What was put back", body = crate::backup::RestoreReport),
        (status = 400, description = "Not an archive this can read", body = crate::error::ErrorBody),
    )
)]
pub async fn import_backup(
    crate::auth::Administrator(who): crate::auth::Administrator,
    Site(state): Site,
    body: axum::body::Bytes,
) -> AppResult<Json<crate::backup::RestoreReport>> {
    tracing::warn!(by = %who.username, bytes = body.len(), "restoring an uploaded archive");

    Ok(Json(crate::backup::restore(&state, &body).await?))
}

/// The plugin id that mail settings are stored under.
pub const EMAIL_PLUGIN: &str = crate::plugins::EMAIL_PLUGIN;

pub async fn email_settings_of(state: &AppState) -> AppResult<crate::email::EmailSettingsResponse> {
    let stored =
        crate::plugins::load::<crate::email::EmailConfig>(state.db(), &state.secrets, EMAIL_PLUGIN)
            .await?;

    Ok(match stored {
        Some(stored) => crate::email::EmailSettingsResponse {
            enabled: stored.enabled,
            region: stored.config.region,
            access_key_id: stored.config.access_key_id,
            from_address: stored.config.from_address,
            from_name: stored.config.from_name,
            reply_to: stored.config.reply_to,
            configuration_set: stored.config.configuration_set,
            has_secret_access_key: !stored.config.secret_access_key.is_empty(),
        },
        None => crate::email::EmailSettingsResponse {
            enabled: false,
            region: String::new(),
            access_key_id: String::new(),
            from_address: String::new(),
            from_name: String::new(),
            reply_to: String::new(),
            configuration_set: String::new(),
            has_secret_access_key: false,
        },
    })
}

/// How mail is sent from this site.
#[utoipa::path(
    get,
    path = "/plugins/email",
    tag = "plugins",
    responses((status = 200, description = "Mail settings", body = crate::email::EmailSettingsResponse))
)]
pub async fn get_email_settings(
    Site(state): Site,
) -> AppResult<Json<crate::email::EmailSettingsResponse>> {
    Ok(Json(email_settings_of(&state).await?))
}

/// Turns what the panel sent into what is stored, keeping the secret when the
/// field came back empty — the panel never receives it, so an untouched form
/// would otherwise erase it on every save.
async fn resolve_email(
    state: &AppState,
    payload: &crate::email::EmailSettingsRequest,
) -> AppResult<crate::email::EmailConfig> {
    let stored =
        crate::plugins::load::<crate::email::EmailConfig>(state.db(), &state.secrets, EMAIL_PLUGIN)
            .await?;

    // Absent means keep what is stored — the panel never receives the secret,
    // so an untouched form leaves the field out and must not erase it.
    // Present and empty means clear it, which is the only way to take a key
    // back from a site without deleting everything else about its mail.
    let secret = match payload.secret_access_key.as_deref() {
        Some(given) => given.trim().to_string(),
        None => stored
            .as_ref()
            .map(|stored| stored.config.secret_access_key.clone())
            .unwrap_or_default(),
    };

    Ok(crate::email::EmailConfig {
        region: payload.region.trim().to_string(),
        access_key_id: payload.access_key_id.trim().to_string(),
        secret_access_key: secret,
        from_address: payload.from_address.trim().to_string(),
        from_name: payload.from_name.trim().to_string(),
        reply_to: payload.reply_to.trim().to_string(),
        configuration_set: payload.configuration_set.trim().to_string(),
    })
}

fn usable(config: &crate::email::EmailConfig) -> AppResult<()> {
    if config.region.is_empty() {
        return Err(AppError::Validation("a region is needed".to_string()));
    }
    if config.access_key_id.is_empty() || config.secret_access_key.is_empty() {
        return Err(AppError::Validation(
            "an access key and its secret are needed".to_string(),
        ));
    }
    if !crate::email::looks_like_an_address(&config.from_address) {
        return Err(AppError::Validation(
            "the address mail comes from is not an email address".to_string(),
        ));
    }
    if !config.reply_to.is_empty() && !crate::email::looks_like_an_address(&config.reply_to) {
        return Err(AppError::Validation(
            "the reply-to is not an email address".to_string(),
        ));
    }
    Ok(())
}

/// Set them.
#[utoipa::path(
    put,
    path = "/plugins/email",
    tag = "plugins",
    request_body = crate::email::EmailSettingsRequest,
    responses((status = 200, description = "Saved", body = crate::email::EmailSettingsResponse))
)]
pub async fn update_email_settings(
    Site(state): Site,
    Json(payload): Json<crate::email::EmailSettingsRequest>,
) -> AppResult<Json<crate::email::EmailSettingsResponse>> {
    Ok(Json(save_email_of(&state, payload).await?))
}

pub async fn save_email_of(
    state: &AppState,
    payload: crate::email::EmailSettingsRequest,
) -> AppResult<crate::email::EmailSettingsResponse> {
    let config = resolve_email(state, &payload).await?;
    // Checked only when it is switched on. Half-filled settings somebody is
    // still typing are worth keeping; ones that are supposed to be working
    // are not worth pretending about.
    if payload.enabled {
        usable(&config)?;
    }

    crate::plugins::save(
        state.db(),
        &state.secrets,
        EMAIL_PLUGIN,
        payload.enabled,
        &config,
    )
    .await?;
    email_settings_of(state).await
}

/// Send one message to an address, and say what SES said.
///
/// The only honest test of mail settings: a key with the wrong permissions, an
/// address SES has not verified and an account still in the sandbox all look
/// identical until something is actually sent.
#[utoipa::path(
    post,
    path = "/plugins/email/test",
    tag = "plugins",
    request_body = crate::email::TestEmailRequest,
    responses((status = 200, description = "Test result", body = ConnectionTestResponse))
)]
pub async fn test_email_settings(
    Site(state): Site,
    Json(payload): Json<crate::email::TestEmailRequest>,
) -> AppResult<Json<ConnectionTestResponse>> {
    Ok(Json(test_email_of(&state, payload).await?))
}

pub async fn test_email_of(
    state: &AppState,
    payload: crate::email::TestEmailRequest,
) -> AppResult<ConnectionTestResponse> {
    let stored =
        crate::plugins::load::<crate::email::EmailConfig>(state.db(), &state.secrets, EMAIL_PLUGIN)
            .await?
            .ok_or_else(|| AppError::Validation("mail is not set up yet".to_string()))?;

    usable(&stored.config)?;

    let outcome = crate::email::send(
        &stored.config,
        crate::email::Message {
            to: &payload.to,
            subject: "Mavi CMS test",
            text: "This is the test message from your site's mail settings. \
                   If you are reading it, sending works.",
            html: None,
        },
    )
    .await;

    Ok(match outcome {
        Ok(()) => ConnectionTestResponse {
            ok: true,
            message: format!("sent to {}", payload.to.trim()),
        },
        Err(err) => ConnectionTestResponse {
            ok: false,
            message: err.to_string(),
        },
    })
}

/// The stored settings, or a refusal saying they are not there yet.
async fn email_config_of(state: &AppState) -> AppResult<crate::email::EmailConfig> {
    crate::plugins::load::<crate::email::EmailConfig>(
        state.db(),
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await?
    .map(|stored| stored.config)
    .ok_or_else(|| AppError::Validation("mail is not set up yet".to_string()))
}

/// What Amazon says about the account these keys belong to.
#[utoipa::path(
    get,
    path = "/plugins/email/account",
    tag = "plugins",
    responses((status = 200, description = "Quota, sandbox and enforcement", body = crate::email::AccountStatus))
)]
pub async fn get_email_account(Site(state): Site) -> AppResult<Json<crate::email::AccountStatus>> {
    Ok(Json(email_account_of(&state).await?))
}

pub async fn email_account_of(state: &AppState) -> AppResult<crate::email::AccountStatus> {
    crate::email::account(&email_config_of(state).await?).await
}

/// Ask Amazon to take the account out of the sandbox.
#[utoipa::path(
    post,
    path = "/plugins/email/production-access",
    tag = "plugins",
    request_body = crate::email::ProductionAccessRequest,
    responses((status = 204, description = "Asked"))
)]
pub async fn request_email_production_access(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    Json(payload): Json<crate::email::ProductionAccessRequest>,
) -> AppResult<StatusCode> {
    request_production_access_of(&state, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn request_production_access_of(
    state: &AppState,
    payload: crate::email::ProductionAccessRequest,
) -> AppResult<()> {
    crate::email::request_production_access(&email_config_of(state).await?, payload).await
}

/// The addresses and domains SES will send from.
#[utoipa::path(
    get,
    path = "/plugins/email/identities",
    tag = "plugins",
    responses((status = 200, description = "Verified senders", body = Vec<crate::email::Identity>))
)]
pub async fn list_email_identities(
    Site(state): Site,
) -> AppResult<Json<Vec<crate::email::Identity>>> {
    Ok(Json(email_identities_of(&state).await?))
}

pub async fn email_identities_of(state: &AppState) -> AppResult<Vec<crate::email::Identity>> {
    crate::email::identities(&email_config_of(state).await?).await
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct IdentityRequest {
    /// An address, or a domain to sign for.
    pub name: String,
}

/// Ask SES to trust one.
#[utoipa::path(
    post,
    path = "/plugins/email/identities",
    tag = "plugins",
    request_body = IdentityRequest,
    responses((status = 204, description = "Asked"))
)]
pub async fn add_email_identity(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    Json(payload): Json<IdentityRequest>,
) -> AppResult<StatusCode> {
    add_email_identity_of(&state, &payload.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_email_identity_of(state: &AppState, name: &str) -> AppResult<()> {
    crate::email::add_identity(&email_config_of(state).await?, name).await
}

/// Stop trusting one.
#[utoipa::path(
    delete,
    path = "/plugins/email/identities/{name}",
    tag = "plugins",
    params(("name" = String, Path, description = "The address or domain")),
    responses((status = 204, description = "Removed"))
)]
pub async fn delete_email_identity(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> AppResult<StatusCode> {
    remove_email_identity_of(&state, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_email_identity_of(state: &AppState, name: &str) -> AppResult<()> {
    crate::email::remove_identity(&email_config_of(state).await?, name).await
}

/// The addresses SES has stopped writing to.
#[utoipa::path(
    get,
    path = "/plugins/email/suppressed",
    tag = "plugins",
    responses((status = 200, description = "Blocked addresses", body = Vec<crate::email::Suppressed>))
)]
pub async fn list_email_suppressed(
    Site(state): Site,
) -> AppResult<Json<Vec<crate::email::Suppressed>>> {
    Ok(Json(email_suppressed_of(&state).await?))
}

pub async fn email_suppressed_of(state: &AppState) -> AppResult<Vec<crate::email::Suppressed>> {
    crate::email::suppressed(&email_config_of(state).await?).await
}

/// Take one off that list.
#[utoipa::path(
    delete,
    path = "/plugins/email/suppressed/{address}",
    tag = "plugins",
    params(("address" = String, Path, description = "The address")),
    responses((status = 204, description = "Unblocked"))
)]
pub async fn delete_email_suppressed(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    axum::extract::Path(address): axum::extract::Path<String>,
) -> AppResult<StatusCode> {
    unsuppress_email_of(&state, &address).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unsuppress_email_of(state: &AppState, address: &str) -> AppResult<()> {
    crate::email::unsuppress(&email_config_of(state).await?, address).await
}

/// How the account has been behaving: the bounce and complaint rates Amazon
/// suspends accounts over.
#[utoipa::path(
    get,
    path = "/plugins/email/health",
    tag = "plugins",
    responses((status = 200, description = "Two weeks of sending", body = crate::email::SendingHealth))
)]
pub async fn get_email_health(Site(state): Site) -> AppResult<Json<crate::email::SendingHealth>> {
    Ok(Json(email_health_of(&state).await?))
}

pub async fn email_health_of(state: &AppState) -> AppResult<crate::email::SendingHealth> {
    crate::email::health(&email_config_of(state).await?).await
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct SendingSwitch {
    pub enabled: bool,
}

/// Stop or resume everything this account sends.
#[utoipa::path(
    post,
    path = "/plugins/email/sending",
    tag = "plugins",
    request_body = SendingSwitch,
    responses((status = 204, description = "Changed"))
)]
pub async fn set_email_sending(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    Json(payload): Json<SendingSwitch>,
) -> AppResult<StatusCode> {
    set_email_sending_of(&state, payload.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_email_sending_of(state: &AppState, enabled: bool) -> AppResult<()> {
    crate::email::set_sending(&email_config_of(state).await?, enabled).await
}

/// The configuration sets this account has.
#[utoipa::path(
    get,
    path = "/plugins/email/configuration-sets",
    tag = "plugins",
    responses((status = 200, description = "Their names", body = Vec<String>))
)]
pub async fn list_email_configuration_sets(Site(state): Site) -> AppResult<Json<Vec<String>>> {
    Ok(Json(email_configuration_sets_of(&state).await?))
}

pub async fn email_configuration_sets_of(state: &AppState) -> AppResult<Vec<String>> {
    crate::email::configuration_sets(&email_config_of(state).await?).await
}

/// Ask Amazon for a bigger quota.
#[utoipa::path(
    post,
    path = "/plugins/email/quota-increase",
    tag = "plugins",
    request_body = crate::email::QuotaIncreaseRequest,
    responses((status = 201, description = "Asked", body = crate::email::SupportCase))
)]
pub async fn request_email_quota_increase(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    Json(payload): Json<crate::email::QuotaIncreaseRequest>,
) -> AppResult<(StatusCode, Json<crate::email::SupportCase>)> {
    Ok((
        StatusCode::CREATED,
        Json(request_quota_increase_of(&state, payload).await?),
    ))
}

pub async fn request_quota_increase_of(
    state: &AppState,
    payload: crate::email::QuotaIncreaseRequest,
) -> AppResult<crate::email::SupportCase> {
    crate::email::request_quota_increase(&email_config_of(state).await?, payload).await
}

/// The requests this account has made about sending, and where they got to.
#[utoipa::path(
    get,
    path = "/plugins/email/requests",
    tag = "plugins",
    responses((status = 200, description = "Requests", body = Vec<crate::email::SupportCase>))
)]
pub async fn list_email_requests(
    Site(state): Site,
) -> AppResult<Json<Vec<crate::email::SupportCase>>> {
    Ok(Json(email_requests_of(&state).await?))
}

pub async fn email_requests_of(state: &AppState) -> AppResult<Vec<crate::email::SupportCase>> {
    crate::email::support_cases(&email_config_of(state).await?).await
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct MailFromRequest {
    /// A subdomain of the identity, such as `mail.example.com`. Empty puts it
    /// back to Amazon's own.
    pub subdomain: String,
}

/// Set the subdomain bounces come back to.
#[utoipa::path(
    post,
    path = "/plugins/email/identities/{name}/mail-from",
    tag = "plugins",
    params(("name" = String, Path, description = "The domain")),
    request_body = MailFromRequest,
    responses((status = 204, description = "Set"))
)]
pub async fn set_email_mail_from(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    axum::extract::Path(name): axum::extract::Path<String>,
    Json(payload): Json<MailFromRequest>,
) -> AppResult<StatusCode> {
    set_mail_from_of(&state, &name, &payload.subdomain).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_mail_from_of(state: &AppState, identity: &str, subdomain: &str) -> AppResult<()> {
    crate::email::set_mail_from(&email_config_of(state).await?, identity, subdomain).await
}

/// Make a configuration set.
#[utoipa::path(
    post,
    path = "/plugins/email/configuration-sets",
    tag = "plugins",
    request_body = IdentityRequest,
    responses((status = 204, description = "Made"))
)]
pub async fn create_email_configuration_set(
    _admin: crate::auth::Administrator,
    Site(state): Site,
    Json(payload): Json<IdentityRequest>,
) -> AppResult<StatusCode> {
    create_configuration_set_of(&state, &payload.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_configuration_set_of(state: &AppState, name: &str) -> AppResult<()> {
    crate::email::create_configuration_set(&email_config_of(state).await?, name).await
}
