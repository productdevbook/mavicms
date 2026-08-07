//! Publishing a site: what to build, and what was built.
//!
//! A site's pages are its own project — its own repository, its own design,
//! its own build. The CMS holds the content and knows how to ask for that
//! project to be rebuilt; it does not know what the project looks like, and
//! nothing here assumes it.
//!
//! This module is the record. A build is a row that a builder somewhere picks
//! up, and the row is the only thing the two sides share: the panel writes
//! `queued` and reads back what happened, the builder takes `queued` rows and
//! writes what it did. Neither has to be running for the other to work.

use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    crypto::SecretBox,
    error::{AppError, AppResult},
};

/// What a build does when the site has not said otherwise.
const DEFAULT_BRANCH: &str = "main";
const DEFAULT_COMMAND: &str = "bun install --frozen-lockfile && bun run build";
const DEFAULT_OUTPUT: &str = "dist";

/// How much of a build's output is kept.
///
/// The tail rather than the head: a build that failed says why at the end, and
/// the thousand lines of dependency resolution before it are noise.
pub const LOG_LIMIT: usize = 64 * 1024;

/// Splits an environment's names from its encrypted contents. A unit separator
/// because it cannot occur in either half.
const SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BuildConfig {
    /// Where the site's project lives.
    pub repository: String,
    pub branch: String,
    pub build_command: String,
    /// The folder the build writes, relative to the project.
    pub output_dir: String,
    /// Whether an access token is stored for a private repository. The token
    /// itself never comes back out.
    pub has_token: bool,
    /// The names of the variables the build runs with. Their values do not
    /// come back out either — they are passwords more often than not.
    pub environment_keys: Vec<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            repository: String::new(),
            branch: DEFAULT_BRANCH.to_string(),
            build_command: DEFAULT_COMMAND.to_string(),
            output_dir: DEFAULT_OUTPUT.to_string(),
            has_token: false,
            environment_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveBuildConfig {
    pub repository: String,
    pub branch: Option<String>,
    pub build_command: Option<String>,
    pub output_dir: Option<String>,
    /// Left out to keep the stored token, empty to remove it.
    pub token: Option<String>,
    /// Replaces every variable the build runs with. Left out to keep them.
    pub environment: Option<std::collections::BTreeMap<String, String>>,
    /// Adds these, replacing any of the same name and leaving the rest alone.
    /// This is what a panel offering "add a variable" sends, because replacing
    /// the whole set to change one of them is how a build quietly stops
    /// working.
    pub environment_set: Option<std::collections::BTreeMap<String, String>>,
    /// Removes these by name.
    pub environment_remove: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Build {
    pub id: String,
    pub status: String,
    pub log: String,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

pub const QUEUED: &str = "queued";
pub const RUNNING: &str = "running";
pub const SUCCEEDED: &str = "succeeded";
pub const FAILED: &str = "failed";

fn parameter(backend: DatabaseBackend, position: u8) -> String {
    match backend {
        DatabaseBackend::Postgres => format!("${position}"),
        _ => "?".to_string(),
    }
}

fn parameters(backend: DatabaseBackend, count: u8) -> String {
    (1..=count)
        .map(|n| parameter(backend, n))
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn create_tables(db: &DatabaseConnection) -> AppResult<()> {
    for statement in [
        "CREATE TABLE IF NOT EXISTS site_builds (
            tenant_id TEXT PRIMARY KEY,
            repository TEXT NOT NULL,
            branch TEXT NOT NULL,
            build_command TEXT NOT NULL,
            output_dir TEXT NOT NULL,
            token TEXT NOT NULL,
            environment TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS builds (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            status TEXT NOT NULL,
            log TEXT NOT NULL,
            requested_at TEXT NOT NULL,
            started_at TEXT NULL,
            finished_at TEXT NULL
        )",
        "CREATE INDEX IF NOT EXISTS idx_builds_tenant ON builds (tenant_id, requested_at)",
    ] {
        db.execute_raw(Statement::from_string(db.get_database_backend(), statement))
            .await?;
    }
    Ok(())
}

/// What building this site involves, if anyone has said.
pub async fn config(db: &DatabaseConnection, tenant_id: Uuid) -> AppResult<Option<BuildConfig>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT repository, branch, build_command, output_dir, token, environment \
                 FROM site_builds WHERE tenant_id = {}",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;

    row.map(|row| {
        Ok(BuildConfig {
            repository: row.try_get("", "repository")?,
            branch: row.try_get("", "branch")?,
            build_command: row.try_get("", "build_command")?,
            output_dir: row.try_get("", "output_dir")?,
            has_token: !row.try_get::<String>("", "token")?.is_empty(),
            environment_keys: keys_of(&row.try_get::<String>("", "environment")?),
        })
    })
    .transpose()
}

/// The access token for a private repository, in the clear.
///
/// A site's own comes first and the agency's stands behind it, so an agency
/// keeping one token for all of its repositories does not stop a site whose
/// repository is somewhere else from carrying its own.
///
/// Only the builder has any business calling this, and only at the moment it
/// clones. Both are stored encrypted with the server's key, so a copy of the
/// database on its own does not hand over anybody's repositories.
pub async fn token(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    tenant_id: Uuid,
    organization_id: Option<Uuid>,
) -> AppResult<String> {
    let own = site_token(db, secrets, tenant_id).await?;
    if !own.is_empty() {
        return Ok(own);
    }
    match organization_id {
        Some(organization) => crate::console::build_token(db, secrets, organization).await,
        None => Ok(String::new()),
    }
}

/// What this site was given, ignoring what the agency has.
async fn site_token(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    tenant_id: Uuid,
) -> AppResult<String> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT token FROM site_builds WHERE tenant_id = {}",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;

    let Some(row) = row else {
        return Ok(String::new());
    };
    let stored: String = row.try_get("", "token")?;
    if stored.is_empty() {
        return Ok(String::new());
    }
    secrets.decrypt(&stored)
}

/// The names in a stored environment, without decrypting it.
///
/// The panel shows which variables a build runs with; it has no business
/// showing what they are, and the names are enough to see that a password is
/// set without printing it.
fn keys_of(stored: &str) -> Vec<String> {
    // The names are kept beside the encrypted values precisely so that reading
    // them does not need the key.
    stored
        .split(SEPARATOR)
        .next()
        .filter(|names| !names.is_empty())
        .map(|names| names.split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

/// The variables a build runs with, in the clear. The builder's business only.
pub async fn environment(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    tenant_id: Uuid,
) -> AppResult<std::collections::BTreeMap<String, String>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT environment FROM site_builds WHERE tenant_id = {}",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;

    let Some(row) = row else {
        return Ok(Default::default());
    };
    let stored: String = row.try_get("", "environment")?;
    let Some((_, encrypted)) = stored.split_once(SEPARATOR) else {
        return Ok(Default::default());
    };
    if encrypted.is_empty() {
        return Ok(Default::default());
    }

    serde_json::from_str(&secrets.decrypt(encrypted)?)
        .map_err(|err| AppError::Internal(format!("unreadable build environment: {err}")))
}

/// Stores an environment as its names, then its encrypted contents.
///
/// The names are in the clear so the panel can list them without the key
/// leaving the server; the values are not, because they are passwords.
fn store_environment(
    secrets: &SecretBox,
    values: &std::collections::BTreeMap<String, String>,
) -> AppResult<String> {
    if values.is_empty() {
        return Ok(String::new());
    }
    for name in values.keys() {
        let usable = !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit());
        if !usable {
            return Err(AppError::Validation(format!(
                "{name} is not a name an environment variable can have"
            )));
        }
    }

    let names = values.keys().cloned().collect::<Vec<_>>().join(",");
    let encrypted = secrets.encrypt(&serde_json::to_string(values).map_err(|err| {
        AppError::Internal(format!("could not store the build environment: {err}"))
    })?)?;

    Ok(format!("{names}{SEPARATOR}{encrypted}"))
}

pub async fn save_config(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    tenant_id: Uuid,
    payload: SaveBuildConfig,
) -> AppResult<BuildConfig> {
    let backend = db.get_database_backend();
    let repository = payload.repository.trim().to_string();

    if repository.is_empty() {
        return Err(AppError::Validation(
            "the site needs a repository to build from".to_string(),
        ));
    }
    // Anything else would be a scheme the builder cannot fetch, or — with
    // ssh:// and a file path — a way to read the builder's own disk.
    if !repository.starts_with("https://") {
        return Err(AppError::Validation(
            "the repository address has to start with https://".to_string(),
        ));
    }

    // The site's own, not what token() would fall back to: leaving the field
    // untouched must not copy the agency's token into this row, where rotating
    // the agency's would then leave a stale one behind.
    let existing = site_token(db, secrets, tenant_id).await.unwrap_or_default();
    let stored_token = match payload.token.as_deref().map(str::trim) {
        None => existing,
        Some("") => String::new(),
        Some(value) => value.to_string(),
    };
    let stored_token = if stored_token.is_empty() {
        String::new()
    } else {
        secrets.encrypt(&stored_token)?
    };

    let stored_environment = match (
        payload.environment,
        payload.environment_set,
        payload.environment_remove,
    ) {
        (Some(values), _, _) => store_environment(secrets, &values)?,
        (None, None, None) => current_environment(db, tenant_id).await?,
        (None, added, removed) => {
            let mut values = environment(db, secrets, tenant_id).await?;
            for name in removed.unwrap_or_default() {
                values.remove(name.trim());
            }
            values.extend(added.unwrap_or_default());
            store_environment(secrets, &values)?
        }
    };

    // A field left out keeps what is stored, the way the token and the
    // variables do. Falling back to the default instead means adding one
    // variable silently rewrites the build command — which is how this site's
    // publishing stopped working once already.
    let current = config(db, tenant_id).await?;
    let kept = |value: Option<String>, stored: Option<&String>, fallback: &str| {
        non_empty(value, stored.map(String::as_str).unwrap_or(fallback))
    };

    let config = BuildConfig {
        repository,
        branch: kept(
            payload.branch,
            current.as_ref().map(|c| &c.branch),
            DEFAULT_BRANCH,
        ),
        build_command: kept(
            payload.build_command,
            current.as_ref().map(|c| &c.build_command),
            DEFAULT_COMMAND,
        ),
        output_dir: clean_output(Some(kept(
            payload.output_dir,
            current.as_ref().map(|c| &c.output_dir),
            DEFAULT_OUTPUT,
        )))?,
        has_token: !stored_token.is_empty(),
        environment_keys: keys_of(&stored_environment),
    };

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "DELETE FROM site_builds WHERE tenant_id = {}",
            parameter(backend, 1)
        ),
        [tenant_id.to_string().into()],
    ))
    .await?;

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO site_builds \
             (tenant_id, repository, branch, build_command, output_dir, token, \
              environment, updated_at) \
             VALUES ({})",
            parameters(backend, 8)
        ),
        [
            tenant_id.to_string().into(),
            config.repository.clone().into(),
            config.branch.clone().into(),
            config.build_command.clone().into(),
            config.output_dir.clone().into(),
            stored_token.into(),
            stored_environment.into(),
            Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;

    Ok(config)
}

async fn current_environment(db: &DatabaseConnection, tenant_id: Uuid) -> AppResult<String> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT environment FROM site_builds WHERE tenant_id = {}",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;

    row.map(|row| row.try_get::<String>("", "environment"))
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(Into::into)
}

fn non_empty(value: Option<String>, fallback: &str) -> String {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

/// The build's output folder, which is joined onto the checkout and then read
/// from — so it has to stay inside it.
fn clean_output(value: Option<String>) -> AppResult<String> {
    let output = non_empty(value, DEFAULT_OUTPUT);
    let refused = output.starts_with('/')
        || output.contains("..")
        || output.contains('\\')
        || output.contains(':');

    if refused {
        return Err(AppError::Validation(
            "the output folder has to be a path inside the project".to_string(),
        ));
    }
    Ok(output)
}

/// Asks for this site to be built.
///
/// A site with a build already waiting gets that one back rather than a
/// second: pressing the button twice means "publish what I have", not "publish
/// it twice", and a queue that grows a row per press would still be building
/// yesterday's content tomorrow.
pub async fn request(db: &DatabaseConnection, tenant_id: Uuid) -> AppResult<Build> {
    let backend = db.get_database_backend();

    if config(db, tenant_id).await?.is_none() {
        return Err(AppError::Validation(
            "tell the site where its pages come from first".to_string(),
        ));
    }

    if let Some(waiting) = latest(db, tenant_id, 1)
        .await?
        .into_iter()
        .find(|build| build.status == QUEUED)
    {
        return Ok(waiting);
    }

    let build = Build {
        id: Uuid::now_v7().to_string(),
        status: QUEUED.to_string(),
        log: String::new(),
        requested_at: Utc::now().to_rfc3339(),
        started_at: None,
        finished_at: None,
    };

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO builds (id, tenant_id, status, log, requested_at) VALUES ({})",
            parameters(backend, 5)
        ),
        [
            build.id.clone().into(),
            tenant_id.to_string().into(),
            build.status.clone().into(),
            String::new().into(),
            build.requested_at.clone().into(),
        ],
    ))
    .await?;

    Ok(build)
}

pub async fn latest(db: &DatabaseConnection, tenant_id: Uuid, limit: u8) -> AppResult<Vec<Build>> {
    let backend = db.get_database_backend();
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id, status, log, requested_at, started_at, finished_at FROM builds \
                 WHERE tenant_id = {} ORDER BY requested_at DESC LIMIT {limit}",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(Build {
                id: row.try_get("", "id")?,
                status: row.try_get("", "status")?,
                log: row.try_get("", "log")?,
                requested_at: row.try_get("", "requested_at")?,
                started_at: row.try_get("", "started_at")?,
                finished_at: row.try_get("", "finished_at")?,
            })
        })
        .collect()
}

/// The oldest build nobody has started, claimed for whoever asks first.
///
/// Claiming is the same statement as finding: two builders polling together
/// must not both take the same row, so the update carries the `queued`
/// condition and only the one that changes a row has the work.
pub async fn claim(db: &DatabaseConnection) -> AppResult<Option<(String, Uuid)>> {
    let backend = db.get_database_backend();

    let row = db
        .query_one_raw(Statement::from_string(
            backend,
            format!(
                "SELECT id, tenant_id FROM builds WHERE status = '{QUEUED}' \
                 ORDER BY requested_at LIMIT 1"
            ),
        ))
        .await?;

    let Some(row) = row else { return Ok(None) };
    let id: String = row.try_get("", "id")?;
    let tenant_id = Uuid::parse_str(&row.try_get::<String>("", "tenant_id")?)
        .map_err(|err| AppError::Internal(format!("bad tenant id: {err}")))?;

    let taken = db
        .execute_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "UPDATE builds SET status = '{RUNNING}', started_at = {} \
                 WHERE id = {} AND status = '{QUEUED}'",
                parameter(backend, 1),
                parameter(backend, 2)
            ),
            [Utc::now().to_rfc3339().into(), id.clone().into()],
        ))
        .await?;

    if taken.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some((id, tenant_id)))
}

/// How much of the beginning is kept when a log is too long.
///
/// The beginning is where a build says what it is building *with* — which
/// toolchain it found, which one the project asked for, whether it could
/// fetch it. Keeping only the end threw all of that away, and left an
/// operator reading "Ignoring lockfile" with no way to learn why. The middle
/// is the thousand lines of dependency resolution nobody reads.
const LOG_HEAD: usize = 8 * 1024;

/// The log as it is kept: the whole of it, or its beginning and its end.
///
/// Used both while a build runs, to bound what is held in memory, and again
/// when it is stored. One function because two of them disagreed: the builder
/// dropped the oldest lines as it went, so by the time the writer tried to
/// keep the beginning there was none left to keep.
pub fn trimmed(log: &str) -> std::borrow::Cow<'_, str> {
    if log.len() <= LOG_LIMIT {
        return std::borrow::Cow::Borrowed(log);
    }

    // Both cuts move outwards to a character boundary. A byte-wise cut through
    // UTF-8 makes a string the database will not take, and keeping slightly
    // less than the limit is the harmless direction to be wrong in.
    let mut head = LOG_HEAD;
    while !log.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = log.len() - (LOG_LIMIT - LOG_HEAD);
    while !log.is_char_boundary(tail) {
        tail += 1;
    }

    // No byte count: this runs repeatedly as a build talks, and a number
    // from the last pass alone would understate what has gone altogether.
    std::borrow::Cow::Owned(format!(
        "{}\n\n… the middle of this build's output is not kept …\n\n{}",
        &log[..head],
        &log[tail..]
    ))
}

pub async fn finish(
    db: &DatabaseConnection,
    id: &str,
    succeeded: bool,
    log: &str,
) -> AppResult<()> {
    let backend = db.get_database_backend();

    let tail = trimmed(log);

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "UPDATE builds SET status = {}, log = {}, finished_at = {} WHERE id = {}",
            parameter(backend, 1),
            parameter(backend, 2),
            parameter(backend, 3),
            parameter(backend, 4)
        ),
        [
            if succeeded { SUCCEEDED } else { FAILED }.into(),
            tail.as_ref().into(),
            Utc::now().to_rfc3339().into(),
            id.into(),
        ],
    ))
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{clean_output, token};
    use crate::console::set_build_token;
    use crate::crypto::SecretBox;
    use sea_orm::{ConnectionTrait, Database, Statement};
    use uuid::Uuid;

    fn secrets() -> SecretBox {
        let dir = std::env::temp_dir().join(format!("mavicms-publish-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        SecretBox::load_or_create(&dir).unwrap()
    }

    /// A control plane with one agency and one of its sites, and nothing else.
    async fn control(
        secrets: &SecretBox,
        site_token: &str,
    ) -> (sea_orm::DatabaseConnection, Uuid, Uuid) {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        crate::console::create_tables(&db).await.unwrap();
        super::create_tables(&db).await.unwrap();

        let organization = Uuid::now_v7();
        let tenant = Uuid::now_v7();
        for statement in [
            format!(
                "INSERT INTO organizations (id, name, site_limit, active, created_at, build_token) \
                 VALUES ('{organization}', 'An Agency', 10, 1, '', '')"
            ),
            format!(
                "INSERT INTO site_builds \
                 (tenant_id, repository, branch, build_command, output_dir, token, environment, updated_at) \
                 VALUES ('{tenant}', 'https://example.invalid/site.git', 'main', 'x', 'dist', '{}', '', '')",
                if site_token.is_empty() {
                    String::new()
                } else {
                    secrets.encrypt(site_token).unwrap()
                }
            ),
        ] {
            db.execute_raw(Statement::from_string(db.get_database_backend(), statement))
                .await
                .unwrap();
        }

        (db, tenant, organization)
    }

    #[tokio::test]
    async fn a_site_without_a_token_of_its_own_builds_with_the_agencys() {
        let secrets = secrets();
        let (db, tenant, organization) = control(&secrets, "").await;
        set_build_token(&db, &secrets, organization, "agency-token")
            .await
            .unwrap();

        assert_eq!(
            token(&db, &secrets, tenant, Some(organization))
                .await
                .unwrap(),
            "agency-token"
        );
    }

    #[tokio::test]
    async fn a_site_with_its_own_token_keeps_it() {
        // The site's repository can be somewhere the agency's token cannot
        // read, so its own has to win rather than merely exist.
        let secrets = secrets();
        let (db, tenant, organization) = control(&secrets, "site-token").await;
        set_build_token(&db, &secrets, organization, "agency-token")
            .await
            .unwrap();

        assert_eq!(
            token(&db, &secrets, tenant, Some(organization))
                .await
                .unwrap(),
            "site-token"
        );
    }

    #[tokio::test]
    async fn no_token_anywhere_is_no_token_rather_than_an_error() {
        // git then asks for a username, which in a container nobody answers —
        // but that is a build failing, not the server failing.
        let secrets = secrets();
        let (db, tenant, organization) = control(&secrets, "").await;

        assert_eq!(
            token(&db, &secrets, tenant, Some(organization))
                .await
                .unwrap(),
            ""
        );
        assert_eq!(token(&db, &secrets, tenant, None).await.unwrap(), "");
    }

    #[tokio::test]
    async fn taking_the_agencys_token_away_leaves_the_sites_own() {
        let secrets = secrets();
        let (db, tenant, organization) = control(&secrets, "site-token").await;
        set_build_token(&db, &secrets, organization, "agency-token")
            .await
            .unwrap();
        set_build_token(&db, &secrets, organization, "")
            .await
            .unwrap();

        assert_eq!(
            token(&db, &secrets, tenant, Some(organization))
                .await
                .unwrap(),
            "site-token"
        );
    }

    #[tokio::test]
    async fn the_agencys_token_is_not_stored_in_the_clear() {
        // A copy of the database on its own must not hand over the
        // repositories it can read.
        let secrets = secrets();
        let (db, _, organization) = control(&secrets, "").await;
        set_build_token(&db, &secrets, organization, "agency-token")
            .await
            .unwrap();

        let row = db
            .query_one_raw(Statement::from_string(
                db.get_database_backend(),
                format!("SELECT build_token FROM organizations WHERE id = '{organization}'"),
            ))
            .await
            .unwrap()
            .unwrap();
        let stored: String = row.try_get("", "build_token").unwrap();

        assert!(!stored.is_empty());
        assert!(!stored.contains("agency-token"));
    }

    #[test]
    fn an_output_folder_stays_inside_the_project() {
        assert_eq!(clean_output(None).unwrap(), "dist");
        assert_eq!(clean_output(Some("build".into())).unwrap(), "build");
        assert_eq!(clean_output(Some("  out  ".into())).unwrap(), "out");
    }

    #[test]
    fn an_output_folder_cannot_point_elsewhere() {
        // This is joined onto a checkout and then read from, so a path that
        // leaves the checkout reads the builder's own disk.
        assert!(clean_output(Some("/etc".into())).is_err());
        assert!(clean_output(Some("../../etc".into())).is_err());
        assert!(clean_output(Some("dist/../..".into())).is_err());
        assert!(clean_output(Some(r"..\windows".into())).is_err());
    }
}

#[cfg(test)]
mod log_tests {
    use super::{LOG_LIMIT, trimmed};

    #[test]
    fn a_short_log_is_kept_whole() {
        assert_eq!(trimmed("$ bun run build\nok\n"), "$ bun run build\nok\n");
    }

    #[test]
    fn a_long_log_keeps_both_ends() {
        let log = format!(
            "bun 1.4.0 in the image\n{}the reason it failed",
            "noise\n".repeat(LOG_LIMIT)
        );
        let kept = trimmed(&log);

        // The beginning says what it built with, the end says how it went.
        assert!(kept.starts_with("bun 1.4.0 in the image"));
        assert!(kept.ends_with("the reason it failed"));
        assert!(kept.contains("not kept"));
    }

    #[test]
    fn what_was_dropped_is_not_passed_off_as_the_whole_log() {
        let log = "noise\n".repeat(LOG_LIMIT);
        assert!(trimmed(&log).contains("not kept"));
        assert!(!trimmed("short").contains("not kept"));
    }

    #[test]
    fn a_cut_through_a_character_is_not_made() {
        // Every character two bytes, so a naive cut would land inside one.
        let log = format!("{}son", "ş".repeat(LOG_LIMIT));
        let kept = trimmed(&log);

        assert!(kept.ends_with("son"));
        assert!(kept.starts_with('ş'));
    }
}
