//! Many sites on one server.
//!
//! Each site gets its own database, its own media folder and its own master
//! key. That is a deliberate choice over a shared set of tables keyed by a
//! site id: every one of this codebase's queries would then have to filter by
//! that key, and the day one of them does not is the day one customer reads
//! another's posts. A separate database cannot leak that way, and it makes a
//! site something you can back up, move or delete by handling one directory.
//!
//! Where that database lives is data, not a compile-time choice. A small blog
//! runs happily on a SQLite file; a busy one can be handed a Postgres URL
//! without anything else changing.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    crypto::SecretBox,
    error::{AppError, AppResult},
    state::AppState,
};

/// How many sites may have their database open at once.
///
/// Sites are opened when they are first asked for, and the least recently used
/// is closed once this many are open. Together with the connection limits in
/// [`crate::db`] this is what keeps the memory a server uses a function of how
/// many sites are *busy* rather than how many exist.
const MAX_OPEN: usize = 64;

/// How long a site is kept open after its last request.
///
/// The cap above only reclaims memory once sixty-four sites are open, which on
/// a server running eighty quiet sites never happens. This closes a site that
/// nobody has visited for a while whether or not there is pressure, so the
/// steady cost is the sites people are actually reading.
const IDLE_FOR: Duration = Duration::from_secs(10 * 60);

/// A site.
#[derive(Debug, Clone)]
pub struct Tenant {
    pub id: Uuid,
    /// The host this site answers on. One host, one site.
    pub host: String,
    /// A short name used for its folder on disk.
    pub slug: String,
    /// Where its database lives. Empty means the SQLite file in its folder.
    pub database_url: String,
    pub active: bool,
}

impl Tenant {
    pub fn root(&self, sites_dir: &std::path::Path) -> PathBuf {
        sites_dir.join(&self.slug)
    }

    fn resolved_database_url(&self, sites_dir: &std::path::Path) -> String {
        if !self.database_url.trim().is_empty() {
            return self.database_url.clone();
        }
        let path = self.root(sites_dir).join("data.db");
        format!("sqlite://{}?mode=rwc", path.display())
    }
}

struct Open {
    state: AppState,
    used_at: Instant,
}

/// Holds the list of sites and the databases currently open.
pub struct Registry {
    /// Where the list of sites is kept — its own database, so that losing or
    /// moving one site's data says nothing about the others.
    control: DatabaseConnection,
    sites_dir: PathBuf,
    open: Mutex<Vec<(Uuid, Open)>>,
}

impl Registry {
    pub async fn new(control: DatabaseConnection, sites_dir: PathBuf) -> AppResult<Self> {
        create_table(&control).await?;
        Ok(Self {
            control,
            sites_dir,
            open: Mutex::new(Vec::new()),
        })
    }

    /// The site answering on this host, if there is one.
    ///
    /// Every request that arrives asks this question, so it asks the database
    /// for the one row rather than reading the table and looking through it:
    /// on a server with five hundred sites that is the difference between
    /// parsing one row per request and five hundred.
    pub async fn find_by_host(&self, host: &str) -> AppResult<Option<Tenant>> {
        // The port is the server's business, not the site's.
        let host = host.split(':').next().unwrap_or(host).trim().to_lowercase();

        let row = self
            .control
            .query_one_raw(Statement::from_sql_and_values(
                self.control.get_database_backend(),
                "SELECT id, host, slug, database_url, active FROM tenants WHERE host = ?",
                [host.into()],
            ))
            .await?;

        row.as_ref().map(tenant_from_row).transpose()
    }

    pub async fn all(&self) -> AppResult<Vec<Tenant>> {
        let rows = self
            .control
            .query_all_raw(Statement::from_string(
                self.control.get_database_backend(),
                "SELECT id, host, slug, database_url, active FROM tenants ORDER BY host",
            ))
            .await?;

        rows.iter().map(tenant_from_row).collect()
    }

    /// Adds a site: its row, its folder, and its database with the schema in
    /// place, so that the first request it serves finds a working site.
    pub async fn create(&self, host: &str, slug: &str, database_url: &str) -> AppResult<Tenant> {
        let host = host.split(':').next().unwrap_or(host).trim().to_lowercase();
        let slug = clean_slug(slug)?;

        if self.find_by_host(&host).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "a site already answers on {host}"
            )));
        }
        if self.all().await?.iter().any(|tenant| tenant.slug == slug) {
            return Err(AppError::Conflict(format!("{slug} is already taken")));
        }

        let tenant = Tenant {
            id: Uuid::new_v4(),
            host,
            slug,
            database_url: database_url.trim().to_string(),
            active: true,
        };

        tokio::fs::create_dir_all(tenant.root(&self.sites_dir).join("media"))
            .await
            .map_err(|err| AppError::Internal(format!("could not create the site folder: {err}")))?;

        self.control
            .execute_raw(Statement::from_sql_and_values(
                self.control.get_database_backend(),
                "INSERT INTO tenants (id, host, slug, database_url, active, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                [
                    tenant.id.to_string().into(),
                    tenant.host.clone().into(),
                    tenant.slug.clone().into(),
                    tenant.database_url.clone().into(),
                    1.into(),
                    chrono::Utc::now().to_rfc3339().into(),
                ],
            ))
            .await?;

        // Opening runs the migrations, so a failure here is visible now rather
        // than to whoever first visits the site.
        self.state_for(&tenant).await?;

        Ok(tenant)
    }

    /// The state a request against this site should run with, opening the
    /// database if it is not already open.
    pub async fn state_for(&self, tenant: &Tenant) -> AppResult<AppState> {
        let mut open = self.open.lock().await;

        // Requests are the only clock this needs: a server nobody is using has
        // nothing to reclaim, and one that is being used sweeps constantly.
        let now = Instant::now();
        open.retain(|(id, entry)| *id == tenant.id || now.duration_since(entry.used_at) < IDLE_FOR);

        if let Some(entry) = open.iter_mut().find(|(id, _)| *id == tenant.id) {
            entry.1.used_at = Instant::now();
            return Ok(entry.1.state.clone());
        }

        let root = tenant.root(&self.sites_dir);
        tokio::fs::create_dir_all(root.join("media"))
            .await
            .map_err(|err| AppError::Internal(format!("could not create the site folder: {err}")))?;

        let db = crate::db::connect(&tenant.resolved_database_url(&self.sites_dir))
            .await
            .map_err(|err| {
                AppError::Internal(format!("could not open the site's database: {err}"))
            })?;

        let secrets = SecretBox::load_or_create(&root)
            .map_err(|err| AppError::Internal(format!("could not open the site's key: {err}")))?;

        let state = AppState {
            db: Some(db),
            data_dir: root.clone(),
            media_root: root.join("media"),
            secrets: Arc::new(secrets),
        };

        if open.len() >= MAX_OPEN
            && let Some(index) = oldest(&open)
        {
            open.remove(index);
        }
        open.push((
            tenant.id,
            Open {
                state: state.clone(),
                used_at: Instant::now(),
            },
        ));

        Ok(state)
    }

}

fn tenant_from_row(row: &sea_orm::QueryResult) -> AppResult<Tenant> {
    Ok(Tenant {
        id: Uuid::parse_str(&row.try_get::<String>("", "id")?)
            .map_err(|err| AppError::Internal(format!("bad tenant id: {err}")))?,
        host: row.try_get("", "host")?,
        slug: row.try_get("", "slug")?,
        database_url: row.try_get("", "database_url")?,
        active: row.try_get::<i32>("", "active")? != 0,
    })
}

fn oldest(open: &[(Uuid, Open)]) -> Option<usize> {
    open.iter()
        .enumerate()
        .min_by_key(|(_, (_, entry))| entry.used_at)
        .map(|(index, _)| index)
}

/// A folder name, and nothing that could point outside the sites directory.
fn clean_slug(slug: &str) -> AppResult<String> {
    let cleaned: String = slug
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    if cleaned.is_empty() {
        return Err(AppError::Validation(
            "the site needs a name made of letters, digits or dashes".to_string(),
        ));
    }
    Ok(cleaned)
}

async fn create_table(db: &DatabaseConnection) -> AppResult<()> {
    // Written by hand rather than through the migrator: the migrator belongs
    // to a site's schema, and this table is the thing that knows what sites
    // there are.
    db.execute_raw(Statement::from_string(
        db.get_database_backend(),
        "CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY,
            host TEXT NOT NULL UNIQUE,
            slug TEXT NOT NULL UNIQUE,
            database_url TEXT NOT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
    ))
    .await?;
    Ok(())
}

/// What the router is built with once the server serves more than one site:
/// the registry, and the state to fall back on.
#[derive(Clone)]
pub struct Hosting {
    pub registry: Arc<Registry>,
    /// The site a request that matches no host is served with. This is the
    /// installation that existed before there were tenants — keeping it means
    /// turning multi-tenancy on changes nothing for a server hosting one site.
    pub default_state: AppState,
}

/// Which of the sites on this server a request turned out to be for.
#[derive(Clone)]
pub enum Resolved {
    /// The installation the server itself runs — whoever set the server up.
    Host,
    /// One of the sites being hosted.
    Tenant(Box<Tenant>),
}

impl Resolved {
    /// What to call this site in the log. With one site the logs are obviously
    /// about that site; with two hundred, a line that does not say which site
    /// it came from is close to useless.
    /// Whether this is the server's own installation rather than a site it is
    /// hosting.
    pub fn is_host(&self) -> bool {
        matches!(self, Resolved::Host)
    }

    pub fn name(&self) -> &str {
        match self {
            Resolved::Host => "host",
            Resolved::Tenant(tenant) => &tenant.slug,
        }
    }
}

impl Hosting {
    /// The state a request should run with, chosen by the host it asked for.
    pub async fn resolve_host(&self, host: Option<&str>) -> AppResult<(AppState, Resolved)> {
        let Some(host) = host else {
            return Ok((self.default_state.clone(), Resolved::Host));
        };
        match self.registry.find_by_host(host).await? {
            Some(tenant) if tenant.active => Ok((
                self.registry.state_for(&tenant).await?,
                Resolved::Tenant(Box::new(tenant)),
            )),
            Some(_) => Err(AppError::Unavailable(
                "this site has been switched off".to_string(),
            )),
            None => Ok((self.default_state.clone(), Resolved::Host)),
        }
    }
}

/// Resolves the site a request is for and hands its state to the handlers.
///
/// Handlers take `Site` rather than `State<AppState>`: which site they are
/// working on is a property of the request, not of the server.
pub async fn resolve(
    axum::extract::State(hosting): axum::extract::State<Hosting>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    match hosting.resolve_host(host.as_deref()).await {
        Ok((state, resolved)) => {
            use tracing::Instrument;

            let span = tracing::info_span!("site", name = resolved.name());
            request.extensions_mut().insert(state);
            request.extensions_mut().insert(resolved);
            next.run(request).instrument(span).await
        }
        Err(err) => err.into_response(),
    }
}

/// The site this request is for.
pub struct Site(pub AppState);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Site {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AppState>()
            .cloned()
            .map(Site)
            .ok_or_else(|| {
                AppError::Internal("the site was not resolved for this request".to_string())
            })
    }
}

/// Proof that a request came in on the server's own address rather than on one
/// of the hosted sites.
///
/// Which sites exist is the server operator's business. An administrator of a
/// hosted site is an administrator of *that* site: without this, their session
/// would let them list every other customer on the machine and add sites of
/// their own. A hosted site's administrator cannot get here, because reaching
/// the server's own address means being checked against the server's own
/// accounts, which they do not have.
pub struct Operator;

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Operator {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<Resolved>() {
            Some(Resolved::Host) => Ok(Operator),
            _ => Err(AppError::Forbidden(
                "sites are managed from the server's own address".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::clean_slug;

    #[test]
    fn keeps_an_ordinary_name() {
        assert_eq!(clean_slug("example-com").unwrap(), "example-com");
        assert_eq!(clean_slug("  Example_COM  ").unwrap(), "example_com");
    }

    #[test]
    fn a_name_cannot_climb_out_of_the_sites_directory() {
        // Whatever is written here becomes a path under the sites directory,
        // so separators and dots are not folded into something safe — they are
        // dropped, and what is left has nowhere else to point.
        assert_eq!(clean_slug("../../etc").unwrap(), "etc");
        assert_eq!(clean_slug("a/../../b").unwrap(), "ab");
        assert_eq!(clean_slug("..\\windows").unwrap(), "windows");
        assert_eq!(clean_slug("site.test/../other").unwrap(), "sitetestother");
    }

    #[test]
    fn a_name_that_is_only_punctuation_is_refused() {
        // ".." surviving as an empty string would put the site in the sites
        // directory itself.
        assert!(clean_slug("..").is_err());
        assert!(clean_slug("/").is_err());
        assert!(clean_slug("   ").is_err());
    }
}
