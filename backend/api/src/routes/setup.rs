use std::{path::Path, time::Duration};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use axum::{Json, http::StatusCode};
use chrono::Utc;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::{
    auth::create_session,
    db,
    dto::setup::{
        SetupDatabaseRequest, SetupDatabaseResponse, SetupRequest, SetupResponse,
        SetupStatusResponse,
    },
    entities::{site_settings, user},
    error::{AppError, AppResult},
    tenants::{Operator, Resolved, Site},
};

/// Report whether the site has completed first-run setup.
#[utoipa::path(
    get,
    path = "/setup/status",
    tag = "setup",
    responses((status = 200, description = "Setup status", body = SetupStatusResponse)),
    // Answers without a credential.
    security(())
)]
pub async fn setup_status(
    Site(state): Site,
    axum::Extension(resolved): axum::Extension<Resolved>,
) -> AppResult<Json<SetupStatusResponse>> {
    let server = resolved.is_host();

    let Some(db) = state.db.as_ref() else {
        return Ok(Json(SetupStatusResponse {
            database_configured: false,
            installed: false,
            site_title: None,
            server,
        }));
    };

    let settings = site_settings::Entity::find().one(db).await?;
    Ok(Json(SetupStatusResponse {
        database_configured: true,
        installed: settings.is_some() || somebody_is_here(db).await?,
        site_title: settings.map(|s| s.site_title),
        server,
    }))
}

/// Whether this site already belongs to somebody.
///
/// The settings row is the usual sign, and it is not the only one: a site
/// opened from the console and walked into by its agency has an administrator
/// and no settings, because the hand-off from the console creates the account
/// directly and the wizard never runs. Reading only the settings row leaves
/// such a site offering first-run setup on its own address, to anyone.
///
/// Builders do not count. They are accounts a build mints for itself, they
/// appear before anybody has set the site up, and treating one as an owner
/// would leave the real one unable to.
async fn somebody_is_here(db: &sea_orm::DatabaseConnection) -> AppResult<bool> {
    Ok(user::Entity::find()
        .filter(user::Column::Role.ne(crate::auth::BUILDER))
        .one(db)
        .await?
        .is_some())
}

/// Configure the database connection. Tests the connection (and runs
/// migrations against it) before persisting anything, then restarts the
/// process so the next boot picks up the newly configured `DATABASE_URL`
/// from disk — the container/pod's restart policy brings it back up.
///
/// Restarting is why this is the server operator's alone. A hosted site
/// reaching it would take every other site on the machine down with it, and it
/// has no reason to: a hosted site is given its database when it is created.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    post,
    path = "/setup/database",
    tag = "setup",
    request_body = SetupDatabaseRequest,
    responses(
        (status = 200, description = "Database configured, server is restarting", body = SetupDatabaseResponse),
        (status = 400, description = "Invalid or unreachable database", body = crate::error::ErrorBody),
        (status = 403, description = "Not the server's own address", body = crate::error::ErrorBody),
        (status = 409, description = "Database is already configured", body = crate::error::ErrorBody),
    )
)]
pub async fn configure_database(
    _operator: Operator,
    Site(state): Site,
    Json(payload): Json<SetupDatabaseRequest>,
) -> AppResult<Json<SetupDatabaseResponse>> {
    if state.db.is_some() {
        return Err(AppError::Conflict(
            "database is already configured".to_string(),
        ));
    }

    let url = build_connection_url(&payload)?;

    db::connect(&url)
        .await
        .map_err(|err| AppError::Validation(format!("could not connect to database: {err}")))?;

    let path = state.data_dir.join("database_url");
    write_private(&path, &url).await.map_err(|err| {
        AppError::Internal(format!("failed to persist database configuration: {err}"))
    })?;

    tracing::info!("database configured, restarting to apply it");
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        std::process::exit(0);
    });

    Ok(Json(SetupDatabaseResponse {
        database_configured: true,
    }))
}

fn build_connection_url(payload: &SetupDatabaseRequest) -> AppResult<String> {
    if let Some(url) = payload.url.as_deref().map(str::trim)
        && !url.is_empty()
    {
        return Ok(url.to_string());
    }

    match payload.engine.as_deref() {
        Some("sqlite") => {
            let path = require_field(&payload.database, "database (file path)")?;
            Ok(format!("sqlite://{path}?mode=rwc"))
        }
        Some(engine @ ("postgres" | "mysql")) => {
            let host = require_field(&payload.host, "host")?;
            let database = require_field(&payload.database, "database")?;
            let username = require_field(&payload.username, "username")?;
            let password = payload.password.as_deref().unwrap_or_default();
            let port = payload
                .port
                .unwrap_or(if engine == "postgres" { 5432 } else { 3306 });
            let user_enc = utf8_percent_encode(username, NON_ALPHANUMERIC);
            let pass_enc = utf8_percent_encode(password, NON_ALPHANUMERIC);
            Ok(format!("{engine}://{user_enc}:{pass_enc}@{host}:{port}/{database}"))
        }
        _ => Err(AppError::Validation(
            "provide a connection url, or engine (postgres, mysql or sqlite) with connection details"
                .to_string(),
        )),
    }
}

fn require_field<'a>(value: &'a Option<String>, name: &str) -> AppResult<&'a str> {
    match value.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(AppError::Validation(format!("{name} is required"))),
    }
}

/// Complete first-run setup: create the site record and the initial
/// administrator account. Fails once the site is already installed.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    post,
    path = "/setup",
    tag = "setup",
    request_body = SetupRequest,
    responses(
        (status = 201, description = "Site installed", body = SetupResponse),
        (status = 409, description = "Site is already installed", body = crate::error::ErrorBody),
    )
)]
pub async fn run_setup(
    Site(state): Site,
    cookies: Cookies,
    Json(payload): Json<SetupRequest>,
) -> AppResult<(StatusCode, Json<SetupResponse>)> {
    let db = state.db_or_unavailable()?;

    // Nothing above this asks who is calling: first-run setup cannot, because
    // there is nobody to be yet. That makes this check the whole of the lock on
    // a site's first administrator, and the settings row alone was not enough
    // of one — a site whose agency had already walked in from the console had
    // none, and would have handed an administrator account to whoever asked.
    if site_settings::Entity::find().one(db).await?.is_some() || somebody_is_here(db).await? {
        return Err(AppError::Conflict("site is already installed".to_string()));
    }

    validate_setup_request(&payload)?;

    let password_hash = hash_password(&payload.admin_password)?;
    let now = Utc::now().fixed_offset();
    let admin_id = Uuid::now_v7();
    let txn = db.begin().await?;

    let admin = user::ActiveModel {
        id: Set(admin_id),
        username: Set(payload.admin_username.clone()),
        email: Set(payload.admin_email.clone()),
        password_hash: Set(password_hash),
        role: Set("administrator".to_string()),
        created_at: Set(now),
    };
    admin.insert(&txn).await?;

    let settings = site_settings::ActiveModel {
        id: Set(Uuid::now_v7()),
        site_title: Set(payload.site_title.clone()),
        tagline: Set(payload.tagline.clone()),
        locale: Set(payload.locale.clone()),
        installed_at: Set(now),
    };
    settings.insert(&txn).await?;

    // The migration seeds this from site_settings for existing installs, but on
    // a fresh database it runs before this row exists — so the first content
    // language is created here.
    crate::languages::ensure_seeded(&txn, &payload.locale).await?;

    txn.commit().await?;

    create_session(db, &cookies, admin_id).await?;

    Ok((
        StatusCode::CREATED,
        Json(SetupResponse {
            site_title: payload.site_title,
            admin_username: payload.admin_username,
        }),
    ))
}

fn validate_setup_request(payload: &SetupRequest) -> AppResult<()> {
    if payload.site_title.trim().is_empty() {
        return Err(AppError::Validation(
            "site title must not be empty".to_string(),
        ));
    }

    let username = payload.admin_username.trim();
    if username.len() < 3
        || username.len() > 32
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "username must be 3-32 characters (letters, numbers, _ or -)".to_string(),
        ));
    }

    let email = payload.admin_email.trim();
    let is_valid_email = email
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    if !is_valid_email {
        return Err(AppError::Validation(
            "email address is not valid".to_string(),
        ));
    }

    if payload.admin_password.len() < 8 {
        return Err(AppError::Validation(
            "password must be at least 8 characters".to_string(),
        ));
    }

    Ok(())
}

fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::Validation(format!("failed to hash password: {err}")))
}

/// Writes a file only the owner can read. Used for the database URL, which
/// carries the database password.
async fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .await?;
        file.write_all(contents.as_bytes()).await?;
        file.sync_all().await?;
        // `mode` above only applies when the file is created, so an existing
        // file keeps whatever permissions it had.
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await
    }

    #[cfg(not(unix))]
    tokio::fs::write(path, contents).await
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::{somebody_is_here, write_private};
    use crate::entities::user;
    use chrono::Utc;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database, DatabaseConnection};
    use uuid::Uuid;

    async fn empty_site() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    async fn add(db: &DatabaseConnection, username: &str, role: &str) {
        user::ActiveModel {
            id: Set(Uuid::now_v7()),
            username: Set(username.to_string()),
            email: Set(format!("{username}@example.invalid")),
            password_hash: Set("x".to_string()),
            role: Set(role.to_string()),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_site_nobody_has_opened_is_free_to_set_up() {
        let db = empty_site().await;
        assert!(!somebody_is_here(&db).await.unwrap());
    }

    #[tokio::test]
    async fn a_build_having_run_does_not_claim_the_site() {
        // The builder mints itself an account before anybody has set the site
        // up. Counting it would leave the real owner unable to.
        let db = empty_site().await;
        add(&db, "build", crate::auth::BUILDER).await;
        assert!(!somebody_is_here(&db).await.unwrap());
    }

    #[tokio::test]
    async fn an_agency_that_walked_in_from_the_console_claims_the_site() {
        // No settings row is written by that hand-off, so this is the only
        // thing standing between a live site and a stranger completing its
        // first-run setup.
        let db = empty_site().await;
        add(&db, "agency-something", crate::auth::ADMINISTRATOR).await;
        assert!(somebody_is_here(&db).await.unwrap());
    }

    #[tokio::test]
    async fn database_url_is_only_readable_by_its_owner() {
        let dir = std::env::temp_dir().join(format!("mavicms-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("database_url");

        write_private(&path, "postgres://u:p@host/db")
            .await
            .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        // A file left world-readable by an older version is tightened on rewrite.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, "postgres://u:p2@host/db")
            .await
            .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "postgres://u:p2@host/db"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
