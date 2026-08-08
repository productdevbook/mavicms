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
        // Said here as well as at every call, so somebody typing an address
        // the server will not talk to is told now rather than at the first
        // upload.
        config.ensure_reachable().await?;
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
            senders: stored.config.senders,
            events_token: stored.config.events_token,
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
            senders: Vec::new(),
            events_token: String::new(),
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
        senders: payload
            .senders
            .iter()
            .filter(|sender| crate::email::looks_like_an_address(&sender.address))
            .cloned()
            .collect(),
        // Never from the panel, and made the first time settings are saved
        // rather than when the pipeline is built: the address Amazon posts to
        // should not depend on whether a call to SNS succeeded, and somebody
        // wiring the topic up by hand needs it before anything else exists.
        events_token: stored
            .as_ref()
            .map(|stored| stored.config.events_token.clone())
            .filter(|token| !token.is_empty())
            .unwrap_or_else(|| uuid::Uuid::now_v7().simple().to_string()),
        events_topic_arn: stored
            .as_ref()
            .map(|stored| stored.config.events_topic_arn.clone())
            .unwrap_or_default(),
        // Empty for a site's own account: there is one user of it. It is
        // filled in only where the server lends its own.
        tenant: String::new(),
    })
}

/// Whether these settings could send.
///
/// `lent` is the server having lent this site its account: then the keys and
/// the region are the server's and the site supplies only the name on the
/// letter. Demanding keys of it would leave the arrangement unfinishable — the
/// site could add its domain, watch it verify, and never be able to say it
/// wanted to send from it.
fn usable(config: &crate::email::EmailConfig, lent: bool) -> AppResult<()> {
    if !lent {
        if config.region.is_empty() {
            return Err(AppError::Validation("a region is needed".to_string()));
        }
        if config.access_key_id.is_empty() || config.secret_access_key.is_empty() {
            return Err(AppError::Validation(
                "an access key and its secret are needed".to_string(),
            ));
        }
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
        // Asked before the settings are saved, so it reflects the account the
        // site has now rather than the one it is in the middle of describing.
        let lent = borrowing(state).await.ok().flatten().is_some();
        usable(&config, lent)?;
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
    // The account the site would really send with, so the test is a test of
    // the thing rather than of a copy of it — and so it works for a site
    // borrowing the server's account, which has no keys of its own to load.
    let post = crate::outbound::how(state).await?;
    usable(&post.config, false)?;

    // A test is a message like any other and comes out of the day's allowance.
    // A site with nought left should find that out here rather than from a
    // contact form nobody was watching.
    crate::outbound::may_send(&post, state.db_or_unavailable()?, 1).await?;

    let outcome = crate::email::send(
        &post.config,
        crate::email::Message {
            to: &payload.to,
            subject: "Mavi CMS test",
            text: "This is the test message from your site's mail settings. \
                   If you are reading it, sending works.",
            html: None,
            from: None,
            unsubscribe_url: None,
            tags: &[],
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
/// The account these screens act on.
///
/// The same answer the sending path uses, so a site the server lends its
/// account to can add its domain and read its own sending — which it could
/// not when this only looked at the site's own keys, leaving every one of
/// these screens dead for exactly the sites the arrangement exists for.
async fn email_config_of(state: &AppState) -> AppResult<crate::email::EmailConfig> {
    Ok(crate::outbound::how(state).await?.config)
}

/// The account for the screens that act on the account itself rather than on
/// one of its senders.
///
/// A borrowed account is somebody else's. Its quota, its sandbox status, its
/// reputation reports, its support cases and its event pipeline belong to
/// whoever runs the server — and its suppression list is every other site's
/// correspondents by name, which is the one that would be a breach rather
/// than a nuisance.
async fn own_account_of(state: &AppState) -> AppResult<crate::email::EmailConfig> {
    let post = crate::outbound::how(state).await?;
    if !post.own {
        return Err(AppError::Validation(
            "this site sends through the server's account, and that account's own settings \
             belong to whoever runs the server"
                .to_string(),
        ));
    }
    Ok(post.config)
}

/// The control plane and site, when this site is on the server's account.
///
/// `None` for a site sending with its own keys: there is nobody else on that
/// account to protect it from.
async fn borrowing(
    state: &AppState,
) -> AppResult<Option<(&sea_orm::DatabaseConnection, uuid::Uuid)>> {
    if crate::outbound::how(state).await?.own {
        return Ok(None);
    }
    Ok(match (state.control.as_ref(), state.tenant_id) {
        (Some(control), Some(tenant_id)) => Some((control, tenant_id)),
        _ => None,
    })
}

/// Refuses a sender this site did not add.
///
/// The list on a shared account is the whole server's, so without this a site
/// could delete the domain of any other site on the machine by naming it.
async fn its_own_sender(state: &AppState, name: &str) -> AppResult<()> {
    let Some((control, tenant_id)) = borrowing(state).await? else {
        return Ok(());
    };
    if crate::platform::owns_identity(control, tenant_id, name).await? {
        return Ok(());
    }
    Err(AppError::Validation(
        "this site did not add that sender".to_string(),
    ))
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SendingAllowance {
    /// "own" when this site has its own Amazon account, "shared" when the
    /// server lends its.
    pub sends: String,
    /// Messages a day. Absent when the site sends with its own account, which
    /// the server does not meter.
    pub a_day: Option<i64>,
    pub sent_today: u64,
    /// The address this site's mail actually leaves as.
    pub sender: String,
    /// Whether that address is the server's rather than the site's. The site
    /// sends either way; this is what the screen warns about.
    pub as_the_server: bool,
}

/// How this site sends, and how much of today is left.
///
/// Answered for a site whether or not it has settings of its own, because the
/// most useful case is the one where it has none: it needs to be told that it
/// can send anyway, and how much.
#[utoipa::path(
    get,
    path = "/plugins/email/allowance",
    tag = "plugins",
    responses((status = 200, description = "How this site sends", body = SendingAllowance))
)]
pub async fn get_email_allowance(Site(state): Site) -> AppResult<Json<SendingAllowance>> {
    let db = state.db_or_unavailable()?;
    let sent_today = crate::outbound::sent_today(db).await.unwrap_or(0);

    // The same question the sending path asks, so the screen cannot disagree
    // with what actually happens.
    Ok(Json(match crate::outbound::how(&state).await {
        Ok(post) if post.own => SendingAllowance {
            sends: "own".to_string(),
            a_day: None,
            sent_today,
            sender: post.config.from_address,
            as_the_server: false,
        },
        Ok(post) => SendingAllowance {
            sends: "shared".to_string(),
            a_day: post.a_day,
            sent_today,
            sender: post.config.from_address,
            as_the_server: post.as_the_server,
        },
        // Nothing set up either way. Not an error: the screen it appears on is
        // where somebody goes to fix exactly that.
        Err(_) => SendingAllowance {
            sends: "none".to_string(),
            a_day: None,
            sent_today,
            sender: String::new(),
            as_the_server: false,
        },
    }))
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
    crate::email::account(&own_account_of(state).await?).await
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
    crate::email::request_production_access(&own_account_of(state).await?, payload).await
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
    let all = crate::email::identities(&email_config_of(state).await?).await?;

    let Some((control, tenant_id)) = borrowing(state).await? else {
        return Ok(all);
    };

    let mut mine = Vec::new();
    for one in all {
        if crate::platform::owns_identity(control, tenant_id, &one.name).await? {
            mine.push(one);
        }
    }
    Ok(mine)
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
    if let Some((control, tenant_id)) = borrowing(state).await?
        && crate::platform::identity_taken(control, tenant_id, name).await?
    {
        // Two sites cannot both answer for one domain, and the first to add it
        // is the one Amazon gave the records to.
        return Err(AppError::Validation(
            "another site on this server has already added that sender".to_string(),
        ));
    }

    crate::email::add_identity(&email_config_of(state).await?, name).await?;

    if let Some((control, tenant_id)) = borrowing(state).await? {
        crate::platform::remember_identity(control, tenant_id, name).await?;
    }
    Ok(())
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
    its_own_sender(state, name).await?;
    crate::email::remove_identity(&email_config_of(state).await?, name).await?;

    if let Some((control, tenant_id)) = borrowing(state).await? {
        crate::platform::forget_identity(control, tenant_id, name).await?;
    }
    Ok(())
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
    crate::email::suppressed(&own_account_of(state).await?).await
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
    crate::email::unsuppress(&own_account_of(state).await?, address).await
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
    crate::email::health(&own_account_of(state).await?).await
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
    crate::email::set_sending(&own_account_of(state).await?, enabled).await
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
    crate::email::configuration_sets(&own_account_of(state).await?).await
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
    crate::email::request_quota_increase(&own_account_of(state).await?, payload).await
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
    crate::email::support_cases(&own_account_of(state).await?).await
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
    its_own_sender(state, identity).await?;
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
    crate::email::create_configuration_set(&own_account_of(state).await?, name).await
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct PipelineResponse {
    pub configuration_set: String,
    pub topic_arn: String,
    pub endpoint: String,
    pub confirmed: bool,
}

/// Build the whole path an event travels, in one press.
///
/// A configuration set, Amazon's deliverability manager on it, a topic, a
/// subscription pointing back here, and the event destination joining them.
/// Every one is a screen in the AWS console and the order matters.
#[utoipa::path(
    post,
    path = "/plugins/email/events/setup",
    tag = "plugins",
    responses((status = 200, description = "What was built", body = PipelineResponse))
)]
pub async fn setup_email_events(
    _admin: crate::auth::Administrator,
    axum::Extension(resolved): axum::Extension<crate::tenants::Resolved>,
    Site(state): Site,
) -> AppResult<Json<PipelineResponse>> {
    let host = match &resolved {
        crate::tenants::Resolved::Tenant(tenant) => tenant.host.clone(),
        crate::tenants::Resolved::Host | crate::tenants::Resolved::Unknown => {
            return Err(AppError::Validation(
                "events belong to a hosted site".to_string(),
            ));
        }
    };

    Ok(Json(setup_events_of(&state, &host).await?))
}

pub async fn setup_events_of(state: &AppState, host: &str) -> AppResult<PipelineResponse> {
    // The pipeline is the account's, not a sender's: one configuration set and
    // one SNS topic serve everything the account sends.
    own_account_of(state).await?;

    let stored = crate::plugins::load::<crate::email::EmailConfig>(
        state.db(),
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await?
    .ok_or_else(|| AppError::Validation("mail is not set up yet".to_string()))?;

    let mut config = stored.config;

    // Made once and kept: the address Amazon posts to must not change every
    // time this is run, or a subscription made last week stops working.
    if config.events_token.is_empty() {
        config.events_token = uuid::Uuid::now_v7().simple().to_string();
    }
    let set_name = if config.configuration_set.trim().is_empty() {
        format!("mavicms-{}", crate::slug::slugify_or(host, "site"))
    } else {
        config.configuration_set.trim().to_string()
    };

    let endpoint = format!("https://{host}/api/mail/events/{}", config.events_token);
    let built = crate::email::build_event_pipeline(&config, &set_name, &endpoint).await?;

    // Every send from now on carries the configuration set, which is what
    // makes the events arrive at all.
    config.configuration_set = built.configuration_set.clone();
    config.events_topic_arn = built.topic_arn.clone();

    crate::plugins::save(
        state.db(),
        &state.secrets,
        crate::plugins::EMAIL_PLUGIN,
        stored.enabled,
        &config,
    )
    .await?;

    Ok(PipelineResponse {
        configuration_set: built.configuration_set,
        topic_arn: built.topic_arn,
        endpoint: built.endpoint,
        confirmed: built.confirmed,
    })
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct DaysQuery {
    pub days: Option<i64>,
}

/// Amazon's own figures on how the mail is doing.
#[utoipa::path(
    get,
    path = "/plugins/email/deliverability",
    tag = "plugins",
    params(DaysQuery),
    responses((status = 200, description = "Deliverability", body = crate::email::Deliverability))
)]
pub async fn get_email_deliverability(
    Site(state): Site,
    axum::extract::Query(query): axum::extract::Query<DaysQuery>,
) -> AppResult<Json<crate::email::Deliverability>> {
    Ok(Json(
        deliverability_of(&state, query.days.unwrap_or(30)).await?,
    ))
}

pub async fn deliverability_of(
    state: &AppState,
    days: i64,
) -> AppResult<crate::email::Deliverability> {
    crate::email::deliverability(&own_account_of(state).await?, days).await
}
