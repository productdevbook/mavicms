//! Builds a site's pages and puts them where they can be served.
//!
//! Sites are not one design with a hundred settings — each is its own project,
//! its own repository, its own build. So this does the only thing that works
//! for all of them: fetches the project, runs whatever it says builds it, and
//! copies what came out to the bucket the pages are served from.
//!
//! It takes work from a table rather than from a queue server, because the
//! table is already there and already backed up, and a build that was
//! requested while this was restarting is still waiting when it comes back.
//! Claiming is a conditional update, so running more than one of these is a
//! matter of raising the replica count.
//!
//! The checkout is kept between builds. A bespoke site is mostly its
//! dependencies, and installing them again every time would make publishing a
//! two-minute wait instead of a ten-second one.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use mavicms_api::{
    crypto::SecretBox,
    db,
    error::{AppError, AppResult},
    publish::{self, LOG_LIMIT},
    storage::{MediaStorage, S3Config},
    tenants::Tenant,
};
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use tokio::process::Command;
use uuid::Uuid;

/// How long to wait before asking for work again when there was none.
const IDLE_POLL: Duration = Duration::from_secs(5);

/// How long a single build may take before it is given up on.
///
/// A build that hangs holds the worker for as long as it hangs, and a site
/// whose build never finishes would stop every other site from publishing.
const BUILD_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// Everything one build needs, so that running it is a call rather than eight
/// arguments nobody can read at the call site.
struct Run<'a> {
    checkout: &'a Path,
    tenant: &'a Tenant,
    command: &'a str,
    environment: &'a std::collections::BTreeMap<String, String>,
    /// A token the build reads the site with, when one could be minted.
    read_token: Option<&'a str>,
    /// The bun the project pinned, when it pinned one this could fetch.
    toolchain: Option<&'a Path>,
}

struct Builder {
    control: DatabaseConnection,
    /// The address of that database, used to reach a site's own schema when a
    /// build needs a token to read it with.
    base_url: String,
    secrets: SecretBox,
    /// Where checkouts live between builds.
    workspace: PathBuf,
    /// Where built pages go. Each site has a folder of its own inside it.
    published: MediaStorage,
    /// The address the built site should read the CMS through. Sites are
    /// reached by their own hostname, so this is a template.
    site_scheme: String,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let builder = Builder::from_env()
        .await
        .unwrap_or_else(|err| panic!("could not start: {err}"));

    tracing::info!("builder waiting for work");
    loop {
        match builder.take_one().await {
            Ok(true) => continue,
            Ok(false) => tokio::time::sleep(IDLE_POLL).await,
            Err(err) => {
                tracing::error!(error = %err, "could not reach the queue");
                tokio::time::sleep(IDLE_POLL).await;
            }
        }
    }
}

impl Builder {
    async fn from_env() -> AppResult<Self> {
        let required = |name: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| AppError::Internal(format!("{name} is not set")))
        };

        let base_url = required("DATABASE_URL")?;
        let control = db::connect_plain(&base_url)
            .await
            .map_err(|err| AppError::Internal(format!("could not open the database: {err}")))?;

        let data_dir = PathBuf::from(
            std::env::var("MAVICMS_DATA_DIR").unwrap_or_else(|_| "/data".to_string()),
        );
        let secrets = SecretBox::load_or_create(&data_dir)
            .map_err(|err| AppError::Internal(format!("could not open the key: {err}")))?;

        let workspace = PathBuf::from(
            std::env::var("MAVICMS_WORKSPACE").unwrap_or_else(|_| "/workspace".to_string()),
        );
        tokio::fs::create_dir_all(&workspace)
            .await
            .map_err(|err| AppError::Internal(format!("could not make the workspace: {err}")))?;

        Ok(Self {
            control,
            base_url,
            secrets,
            workspace,
            published: published_storage()?,
            site_scheme: std::env::var("PUBLISH_SITE_SCHEME")
                .unwrap_or_else(|_| "https".to_string()),
        })
    }

    /// Takes one build if there is one, and says whether there was.
    async fn take_one(&self) -> AppResult<bool> {
        let Some((id, tenant_id)) = publish::claim(&self.control).await? else {
            return Ok(false);
        };

        let tenant = self.tenant(tenant_id).await?;
        tracing::info!(build = %id, site = %tenant.host, "building");

        let mut log = String::new();
        let outcome = self.build(&tenant, &mut log).await;

        match &outcome {
            Ok(count) => {
                log.push_str(&format!("\n{count} files published\n"));
                tracing::info!(build = %id, site = %tenant.host, files = count, "published");
            }
            Err(err) => {
                log.push_str(&format!("\n{err}\n"));
                tracing::error!(build = %id, site = %tenant.host, error = %err, "build failed");
            }
        }

        publish::finish(&self.control, &id, outcome.is_ok(), &log).await?;
        Ok(true)
    }

    async fn tenant(&self, id: Uuid) -> AppResult<Tenant> {
        let backend = self.control.get_database_backend();
        let row = self
            .control
            .query_one_raw(Statement::from_sql_and_values(
                backend,
                "SELECT host, slug, schema_name, database_url FROM tenants WHERE id = $1"
                    .to_string(),
                [id.to_string().into()],
            ))
            .await?
            .ok_or_else(|| AppError::NotFound("site".to_string()))?;

        Ok(Tenant {
            id,
            host: row.try_get("", "host")?,
            slug: row.try_get("", "slug")?,
            schema: row.try_get("", "schema_name")?,
            database_url: row.try_get("", "database_url")?,
            organization_id: None,
            active: true,
        })
    }

    async fn build(&self, tenant: &Tenant, log: &mut String) -> AppResult<usize> {
        let config = publish::config(&self.control, tenant.id)
            .await?
            .ok_or_else(|| AppError::Validation("the site has no project".to_string()))?;
        let token = publish::token(&self.control, &self.secrets, tenant.id).await?;
        let environment = publish::environment(&self.control, &self.secrets, tenant.id).await?;

        let checkout = self.workspace.join(&tenant.slug);
        self.fetch(&checkout, &config.repository, &config.branch, &token, log)
            .await?;

        // Minted here and torn down below: the build reads the site with a
        // token that exists for as long as the build does, rather than with
        // somebody's password kept in a settings box.
        let reader = self.build_token(tenant).await?;

        let toolchain = self.toolchain(&checkout, log).await?;
        let outcome = self
            .run_build(
                Run {
                    checkout: &checkout,
                    tenant,
                    command: &config.build_command,
                    environment: &environment,
                    read_token: reader.as_ref().map(|(_, token)| token.as_str()),
                    toolchain: toolchain.as_deref(),
                },
                log,
            )
            .await;

        if let Some((db, token)) = reader {
            spend_token(&db, &token).await;
        }
        outcome?;

        let output = checkout.join(&config.output_dir);
        if !output.is_dir() {
            return Err(AppError::Validation(format!(
                "the build did not write {}",
                config.output_dir
            )));
        }
        self.upload(&output, &tenant.slug, log).await
    }

    /// Clones, or brings an existing checkout to the branch's current state.
    ///
    /// `reset --hard` rather than `pull`: the checkout is a cache, not
    /// somebody's working copy, and a merge conflict in a cache is a build
    /// that fails for no reason anybody can act on.
    async fn fetch(
        &self,
        checkout: &Path,
        repository: &str,
        branch: &str,
        token: &str,
        log: &mut String,
    ) -> AppResult<()> {
        // The token goes in the URL, which is where git wants it and also
        // where it would end up in the log — so the log gets the address
        // without it.
        let authenticated = if token.is_empty() {
            repository.to_string()
        } else {
            repository.replacen("https://", &format!("https://x-access-token:{token}@"), 1)
        };

        if checkout.join(".git").is_dir() {
            log.push_str(&format!("$ git fetch {repository}\n"));
            self.git(
                checkout,
                &["remote", "set-url", "origin", &authenticated],
                log,
            )
            .await?;
            self.git(checkout, &["fetch", "--depth", "1", "origin", branch], log)
                .await?;
            self.git(checkout, &["reset", "--hard", "FETCH_HEAD"], log)
                .await?;
            self.git(checkout, &["clean", "-fd", "-e", "node_modules"], log)
                .await?;
        } else {
            log.push_str(&format!("$ git clone {repository} ({branch})\n"));
            tokio::fs::create_dir_all(checkout).await.map_err(|err| {
                AppError::Internal(format!("could not make the checkout folder: {err}"))
            })?;
            self.git(
                checkout,
                &[
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    branch,
                    &authenticated,
                    ".",
                ],
                log,
            )
            .await?;
        }

        // Not left behind in .git/config, where the next person to look at the
        // workspace would find it.
        self.git(checkout, &["remote", "set-url", "origin", repository], log)
            .await
    }

    async fn git(&self, checkout: &Path, args: &[&str], log: &mut String) -> AppResult<()> {
        let mut command = Command::new("git");
        command.current_dir(checkout).args(args);
        run(command, log, "git").await
    }

    /// The bun the project asked for, installed if it is not here yet.
    ///
    /// A project says which one it was written against in `packageManager`,
    /// and it means it: a lockfile written by a newer bun is one an older bun
    /// refuses to read. Honouring the field is what makes a thousand projects
    /// with a thousand opinions all build on one worker.
    async fn toolchain(&self, checkout: &Path, log: &mut String) -> AppResult<Option<PathBuf>> {
        let Ok(manifest) = tokio::fs::read_to_string(checkout.join("package.json")).await else {
            return Ok(None);
        };
        let Some(version) = bun_version(&manifest) else {
            return Ok(None);
        };

        let root = self.workspace.join(".bun").join(&version);
        let binary = root.join("bin").join("bun");
        if binary.is_file() {
            return Ok(Some(root.join("bin")));
        }

        log.push_str(&format!("\n$ installing bun {version}\n"));
        let mut command = Command::new("sh");
        command
            .current_dir(checkout)
            .arg("-c")
            .arg("curl -fsSL https://bun.sh/install | bash -s \"bun-v$BUN_VERSION\"")
            .env("BUN_VERSION", &version)
            .env("BUN_INSTALL", &root);

        // A pin that cannot be fetched — a version that was never released, a
        // registry that is down — is a warning, not a failed publish. The
        // build carries on with the bun this image has, and says so.
        if let Err(err) = run(command, log, "installing bun").await {
            log.push_str(&format!(
                "\ncould not install bun {version} ({err}); using the one in the image\n"
            ));
            return Ok(None);
        }

        Ok(Some(root.join("bin")))
    }

    /// A session on the site that lasts as long as the build.
    ///
    /// It belongs to an account with the builder role, which can read the site
    /// and nothing else — so a token that turns up in a log is a token that
    /// can look at published posts, rather than one that can delete them.
    async fn build_token(
        &self,
        tenant: &Tenant,
    ) -> AppResult<Option<(sea_orm::DatabaseConnection, String)>> {
        let url = if tenant.database_url.trim().is_empty() {
            &self.base_url
        } else {
            &tenant.database_url
        };

        let db = match mavicms_api::db::connect_in_schema(url, &tenant.schema).await {
            Ok(db) => db,
            Err(err) => {
                // A site whose database will not open is a build that will
                // fail on its own, and more usefully.
                tracing::warn!(site = %tenant.host, error = %err, "no build token this time");
                return Ok(None);
            }
        };

        let user = reader_account(&db).await?;
        let token = Uuid::new_v4();

        db.execute_raw(Statement::from_sql_and_values(
            db.get_database_backend(),
            "INSERT INTO sessions (id, user_id, expires_at, created_at) \
             VALUES ($1::uuid, $2::uuid, $3::timestamptz, $4::timestamptz)",
            [
                token.to_string().into(),
                user.to_string().into(),
                (chrono::Utc::now() + chrono::Duration::hours(2))
                    .to_rfc3339()
                    .into(),
                chrono::Utc::now().to_rfc3339().into(),
            ],
        ))
        .await?;

        Ok(Some((db, token.to_string())))
    }

    async fn run_build(&self, what: Run<'_>, log: &mut String) -> AppResult<()> {
        let Run {
            checkout,
            tenant,
            command: build_command,
            environment,
            read_token,
            toolchain,
        } = what;

        log.push_str(&format!("\n$ {build_command}\n"));

        let mut command = Command::new("sh");
        command
            .current_dir(checkout)
            .arg("-c")
            .arg(build_command)
            // What the project reads to find its content. The site is reached
            // by its own hostname, and the CMS answers on it.
            .env(
                "CMS_API_URL",
                format!("{}://{}/api", self.site_scheme, tenant.host),
            )
            .env(
                "SITE_URL",
                format!("{}://{}", self.site_scheme, tenant.host),
            )
            .env("CI", "true");

        if let Some(token) = read_token {
            command.env("CMS_TOKEN", token);
        }

        // In front of the image's own bun, so the version the project asked
        // for is the one that runs.
        if let Some(bin) = toolchain {
            let existing = std::env::var("PATH").unwrap_or_default();
            command.env("PATH", format!("{}:{existing}", bin.display()));
        }

        // The site's own variables last, so a project that needs a different
        // address or a different account than the defaults can say so.
        for (name, value) in environment {
            command.env(name, value);
        }

        run(command, log, "build").await
    }

    /// Copies the built pages into the bucket under this site's own prefix.
    async fn upload(&self, output: &Path, slug: &str, log: &mut String) -> AppResult<usize> {
        let mut files = Vec::new();
        collect(output, output, &mut files).await?;

        log.push_str(&format!("\nuploading {} files\n", files.len()));

        for relative in &files {
            let path = output.join(relative);
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|err| AppError::Internal(format!("could not read {relative}: {err}")))?;

            self.published
                .put(&format!("{slug}/{relative}"), &bytes, mime_of(relative))
                .await?;
        }

        Ok(files.len())
    }
}

/// Every file under `root`, as paths relative to it.
async fn collect(root: &Path, directory: &Path, into: &mut Vec<String>) -> AppResult<()> {
    let mut entries = tokio::fs::read_dir(directory)
        .await
        .map_err(|err| AppError::Internal(format!("could not read the build output: {err}")))?;

    // Written as a stack rather than recursion: an async function that calls
    // itself needs boxing, and this is the same thing without it.
    let mut pending = Vec::new();
    loop {
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| AppError::Internal(format!("could not read a folder: {err}")))?
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                into.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }

        let Some(next) = pending.pop() else {
            return Ok(());
        };
        entries = tokio::fs::read_dir(&next)
            .await
            .map_err(|err| AppError::Internal(format!("could not read a folder: {err}")))?;
    }
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "pdf" => "application/pdf",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// Runs a command, putting everything it says into the log.
///
/// Output goes in as it arrives rather than being collected at the end, so a
/// build that times out still shows how far it got.
async fn run(mut command: tokio::process::Command, log: &mut String, what: &str) -> AppResult<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|err| AppError::Internal(format!("could not start {what}: {err}")))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let collect_output = async {
        let mut buffer = Vec::new();
        if let Some(mut out) = stdout {
            tokio::io::copy(&mut out, &mut buffer).await.ok();
        }
        if let Some(mut err) = stderr {
            tokio::io::copy(&mut err, &mut buffer).await.ok();
        }
        String::from_utf8_lossy(&buffer).into_owned()
    };

    let (status, output) = tokio::time::timeout(BUILD_TIMEOUT, async {
        tokio::join!(child.wait(), collect_output)
    })
    .await
    .map_err(|_| AppError::Validation(format!("{what} took too long and was stopped")))?;

    push_bounded(log, &output);

    let status =
        status.map_err(|err| AppError::Internal(format!("could not wait for {what}: {err}")))?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "{what} failed ({})",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

/// Adds to the log, dropping the oldest once it is long enough that nobody
/// would read further anyway.
fn push_bounded(log: &mut String, addition: &str) {
    log.push_str(addition);
    if log.len() > LOG_LIMIT {
        let keep = log
            .char_indices()
            .map(|(index, _)| index)
            .find(|index| log.len() - index <= LOG_LIMIT)
            .unwrap_or(0);
        *log = log[keep..].to_string();
    }
}

/// Where published pages are kept.
///
/// A bucket when one is configured, and a folder otherwise. The folder is
/// enough for a single server and needs no credentials; the bucket is what
/// makes more than one server possible. Nothing else in the builder or the
/// edge knows which it got.
fn published_storage() -> AppResult<MediaStorage> {
    let read = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    let Some(bucket) = read("PUBLISH_S3_BUCKET") else {
        let root = PathBuf::from(read("PUBLISH_DIR").unwrap_or_else(|| "/published".to_string()));
        std::fs::create_dir_all(&root)
            .map_err(|err| AppError::Internal(format!("could not make {root:?}: {err}")))?;
        return Ok(MediaStorage::Local { root });
    };

    let required =
        |name: &str| read(name).ok_or_else(|| AppError::Internal(format!("{name} is not set")));
    let config = S3Config {
        endpoint: required("PUBLISH_S3_ENDPOINT")?,
        region: read("PUBLISH_S3_REGION").unwrap_or_else(|| "auto".to_string()),
        bucket,
        access_key_id: required("PUBLISH_S3_ACCESS_KEY_ID")?,
        secret_access_key: required("PUBLISH_S3_SECRET_ACCESS_KEY")?,
        public_base_url: String::new(),
        path_prefix: read("PUBLISH_S3_PREFIX").unwrap_or_else(|| "sites".to_string()),
    };
    config.validate()?;

    Ok(MediaStorage::S3(Box::new(config)))
}

/// The bun version a `package.json` pins, if it pins one.
fn bun_version(manifest: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let declared = value.get("packageManager")?.as_str()?;
    let version = declared.strip_prefix("bun@")?;

    // Only a plain version: this ends up in a URL, and a range or a hash is
    // not something the installer takes anyway.
    let usable = !version.is_empty()
        && version
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c.is_ascii_alphanumeric() || c == '-');
    usable.then(|| version.to_string())
}

/// The account a build reads as, made on first use.
///
/// It has no password: the only way to be it is a token the builder minted
/// minutes ago, so there is nothing here for anybody to guess or reuse.
async fn reader_account(db: &sea_orm::DatabaseConnection) -> AppResult<Uuid> {
    let existing = db
        .query_one_raw(Statement::from_sql_and_values(
            db.get_database_backend(),
            "SELECT id FROM users WHERE username = $1",
            ["build".into()],
        ))
        .await?;

    if let Some(row) = existing {
        return Uuid::parse_str(&row.try_get::<String>("", "id")?)
            .map_err(|err| AppError::Internal(format!("bad user id: {err}")));
    }

    let id = Uuid::new_v4();
    db.execute_raw(Statement::from_sql_and_values(
        db.get_database_backend(),
        "INSERT INTO users (id, username, email, password_hash, role, created_at) \
         VALUES ($1::uuid, $2, $3, $4, $5, $6::timestamptz)",
        [
            id.to_string().into(),
            "build".into(),
            "build@localhost".into(),
            String::new().into(),
            mavicms_api::auth::BUILDER.into(),
            chrono::Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;

    Ok(id)
}

/// Ends the build's session. Best effort — it expires on its own within hours,
/// and a build that succeeded is not undone because tidying up failed.
async fn spend_token(db: &sea_orm::DatabaseConnection, token: &str) {
    let statement = Statement::from_sql_and_values(
        db.get_database_backend(),
        "DELETE FROM sessions WHERE id = $1::uuid",
        [token.into()],
    );
    if let Err(err) = db.execute_raw(statement).await {
        tracing::warn!(error = %err, "could not end the build's session");
    }
}

#[cfg(test)]
mod tests {
    use super::bun_version;

    #[test]
    fn a_pinned_version_is_read() {
        assert_eq!(
            bun_version(r#"{"packageManager": "bun@1.4.0"}"#).unwrap(),
            "1.4.0"
        );
    }

    #[test]
    fn anything_else_is_left_to_the_image() {
        assert!(bun_version(r#"{}"#).is_none());
        assert!(bun_version(r#"{"packageManager": "pnpm@9"}"#).is_none());
        assert!(bun_version("not json").is_none());
    }

    #[test]
    fn a_version_that_is_not_one_is_refused() {
        // It goes into a URL the installer is handed, so it stays a version.
        assert!(bun_version(r#"{"packageManager": "bun@1.4.0; rm -rf /"}"#).is_none());
        assert!(bun_version(r#"{"packageManager": "bun@../../etc"}"#).is_none());
        assert!(bun_version(r#"{"packageManager": "bun@"}"#).is_none());
    }
}
