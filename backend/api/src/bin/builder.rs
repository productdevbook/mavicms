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

struct Builder {
    control: DatabaseConnection,
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

        let control = db::connect_plain(&required("DATABASE_URL")?)
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
                "SELECT host, slug FROM tenants WHERE id = $1".to_string(),
                [id.to_string().into()],
            ))
            .await?
            .ok_or_else(|| AppError::NotFound("site".to_string()))?;

        Ok(Tenant {
            id,
            host: row.try_get("", "host")?,
            slug: row.try_get("", "slug")?,
            schema: String::new(),
            database_url: String::new(),
            organization_id: None,
            active: true,
        })
    }

    async fn build(&self, tenant: &Tenant, log: &mut String) -> AppResult<usize> {
        let config = publish::config(&self.control, tenant.id)
            .await?
            .ok_or_else(|| AppError::Validation("the site has no project".to_string()))?;
        let token = publish::token(&self.control, &self.secrets, tenant.id).await?;
        let environment =
            publish::environment(&self.control, &self.secrets, tenant.id).await?;

        let checkout = self.workspace.join(&tenant.slug);
        self.fetch(&checkout, &config.repository, &config.branch, &token, log)
            .await?;
        self.run_build(&checkout, tenant, &config.build_command, &environment, log)
            .await?;

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
            self.git(checkout, &["remote", "set-url", "origin", &authenticated], log)
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
                &["clone", "--depth", "1", "--branch", branch, &authenticated, "."],
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

    async fn run_build(
        &self,
        checkout: &Path,
        tenant: &Tenant,
        build_command: &str,
        environment: &std::collections::BTreeMap<String, String>,
        log: &mut String,
    ) -> AppResult<()> {
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
            .env("SITE_URL", format!("{}://{}", self.site_scheme, tenant.host))
            .env("CI", "true");

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

        let Some(next) = pending.pop() else { return Ok(()) };
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
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

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

    let (status, output) = tokio::time::timeout(
        BUILD_TIMEOUT,
        async { tokio::join!(child.wait(), collect_output) },
    )
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
        let root = PathBuf::from(
            read("PUBLISH_DIR").unwrap_or_else(|| "/published".to_string()),
        );
        std::fs::create_dir_all(&root)
            .map_err(|err| AppError::Internal(format!("could not make {root:?}: {err}")))?;
        return Ok(MediaStorage::Local { root });
    };

    let required = |name: &str| {
        read(name).ok_or_else(|| AppError::Internal(format!("{name} is not set")))
    };
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
