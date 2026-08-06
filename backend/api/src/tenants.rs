//! Many sites on one server.
//!
//! Each site gets a Postgres schema of its own, its own media folder and its
//! own master key. That is a deliberate choice over a shared set of tables
//! keyed by a site id: every one of this codebase's queries would then have to
//! filter by that key, and the day one of them does not is the day one
//! customer reads another's posts. A schema cannot leak that way — the tables
//! a site's connection can see are only its own — and it costs nothing to
//! operate, because it is still one database server to run, back up and watch.
//!
//! A site can also be handed a database URL of its own, which is how one that
//! outgrows the shared server moves off it without anything else changing.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    crypto::SecretBox,
    error::{AppError, AppResult},
    state::AppState,
};

/// How many sites may have their database connection open at once.
///
/// Sites are opened when they are first asked for, and the least recently used
/// is closed once this many are open. With the per-site connection limit in
/// [`crate::db`] this bounds how many Postgres backends the whole server can
/// hold, which matters because Postgres accepts a hundred of them by default.
const MAX_OPEN: usize = 32;

/// How long a site is kept open after its last request.
///
/// The cap above only reclaims anything once thirty-two sites are open, which
/// on a server running eighty quiet sites never happens. This closes a site
/// nobody has visited for a while whether or not there is pressure, so the
/// steady cost is the sites people are actually reading.
const IDLE_FOR: Duration = Duration::from_secs(10 * 60);

/// Schema names Postgres has already given a meaning.
const RESERVED_SCHEMAS: [&str; 2] = ["public", "information_schema"];

/// A site.
#[derive(Debug, Clone)]
pub struct Tenant {
    pub id: Uuid,
    /// The host this site answers on. One host, one site.
    pub host: String,
    /// A short name used for its folder on disk and its schema.
    pub slug: String,
    /// The Postgres schema holding its tables.
    pub schema: String,
    /// A database server of its own. Empty — which is the normal case — means
    /// its schema on the server's own database.
    pub database_url: String,
    /// The agency this site belongs to. Absent on a site the server operator
    /// made directly, which belongs to nobody but the server.
    pub organization_id: Option<Uuid>,
    pub active: bool,
}

impl Tenant {
    pub fn root(&self, sites_dir: &std::path::Path) -> PathBuf {
        sites_dir.join(&self.slug)
    }
}

struct Open {
    state: AppState,
    used_at: Instant,
}

/// Holds the list of sites and the connections currently open.
pub struct Registry {
    /// The server's own database, which is also where the list of sites is
    /// kept — in `public`, beside the server's own tables, so that the list
    /// lives and is backed up with everything else rather than in a file
    /// somewhere that has to be remembered separately.
    control: DatabaseConnection,
    /// The address of that database, used to reach each site's schema.
    base_url: String,
    sites_dir: PathBuf,
    open: Mutex<Vec<(Uuid, Open)>>,
}

impl Registry {
    pub async fn new(
        control: DatabaseConnection,
        base_url: String,
        sites_dir: PathBuf,
    ) -> AppResult<Self> {
        create_table(&control).await?;
        crate::console::create_tables(&control).await?;
        crate::publish::create_tables(&control).await?;
        Ok(Self {
            control,
            base_url,
            sites_dir,
            open: Mutex::new(Vec::new()),
        })
    }

    /// The server's own database, where the list of sites and the agency
    /// accounts live.
    pub fn control(&self) -> &DatabaseConnection {
        &self.control
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
                format!(
                    "SELECT id, host, slug, schema_name, database_url, organization_id, active \
                     FROM tenants WHERE host = {}",
                    placeholder(self.control.get_database_backend(), 1)
                ),
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
                "SELECT id, host, slug, schema_name, database_url, organization_id, active \
                 FROM tenants ORDER BY host",
            ))
            .await?;

        rows.iter().map(tenant_from_row).collect()
    }

    /// Adds a site: its schema with the tables already in it, its folder, and
    /// the host it answers on — so the first request it serves finds a site
    /// that works.
    ///
    /// Anything this creates before something fails is taken back out again. A
    /// half-made site is worse than none: it holds a name nobody can use and
    /// looks like a site that is merely broken.
    pub async fn create(
        &self,
        host: &str,
        slug: &str,
        database_url: &str,
        organization_id: Option<Uuid>,
    ) -> AppResult<Tenant> {
        if self.control.get_database_backend() != DatabaseBackend::Postgres {
            return Err(AppError::Validation(
                "hosting more than one site needs Postgres — point DATABASE_URL at one".to_string(),
            ));
        }

        let host = host.split(':').next().unwrap_or(host).trim().to_lowercase();
        if host.is_empty() {
            return Err(AppError::Validation(
                "the site needs an address to answer on".to_string(),
            ));
        }
        let slug = clean_slug(slug)?;
        let schema = schema_name(&slug)?;

        if self.find_by_host(&host).await?.is_some() {
            return Err(AppError::Conflict(format!(
                "a site already answers on {host}"
            )));
        }
        if self.all().await?.iter().any(|tenant| tenant.slug == slug) {
            return Err(AppError::Conflict(format!("{slug} is already taken")));
        }

        let tenant = Tenant {
            id: Uuid::now_v7(),
            host,
            slug,
            schema,
            database_url: database_url.trim().to_string(),
            organization_id,
            active: true,
        };

        // Not `IF NOT EXISTS`: a schema already standing under this name
        // belongs to something else, and adopting it would put a new site's
        // tables in with whatever is already there.
        self.control
            .execute_raw(Statement::from_string(
                DatabaseBackend::Postgres,
                format!(r#"CREATE SCHEMA "{}""#, tenant.schema),
            ))
            .await
            .map_err(|err| {
                AppError::Conflict(format!("could not make room for the site: {err}"))
            })?;

        match self.finish_creating(&tenant).await {
            Ok(()) => Ok(tenant),
            Err(err) => {
                self.drop_schema(&tenant.schema).await;
                Err(err)
            }
        }
    }

    async fn finish_creating(&self, tenant: &Tenant) -> AppResult<()> {
        tokio::fs::create_dir_all(tenant.root(&self.sites_dir).join("media"))
            .await
            .map_err(|err| {
                AppError::Internal(format!("could not create the site folder: {err}"))
            })?;

        self.control
            .execute_raw(Statement::from_sql_and_values(
                self.control.get_database_backend(),
                format!(
                    "INSERT INTO tenants \
                     (id, host, slug, schema_name, database_url, organization_id, active, created_at) \
                     VALUES ({})",
                    (1..=8)
                        .map(|n| placeholder(self.control.get_database_backend(), n))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                [
                    tenant.id.to_string().into(),
                    tenant.host.clone().into(),
                    tenant.slug.clone().into(),
                    tenant.schema.clone().into(),
                    tenant.database_url.clone().into(),
                    tenant.organization_id.map(|id| id.to_string()).into(),
                    1.into(),
                    chrono::Utc::now().to_rfc3339().into(),
                ],
            ))
            .await?;

        // Opening runs the migrations, so a schema that will not take them is
        // a failure now rather than one for whoever first visits the site.
        self.state_for(tenant).await?;
        Ok(())
    }

    /// Switches a site on or off. Off means its address answers "switched off"
    /// and nothing else — the content is still there, and turning it back on is
    /// the whole of the undo.
    pub async fn set_active(&self, id: Uuid, active: bool) -> AppResult<()> {
        let backend = self.control.get_database_backend();
        self.control
            .execute_raw(Statement::from_sql_and_values(
                backend,
                format!(
                    "UPDATE tenants SET active = {} WHERE id = {}",
                    placeholder(backend, 1),
                    placeholder(backend, 2)
                ),
                [i32::from(active).into(), id.to_string().into()],
            ))
            .await?;

        self.forget(id).await;
        Ok(())
    }

    /// Removes a site: its row, its schema, its folder.
    ///
    /// There is no undo and this does not pretend otherwise. What it does do is
    /// go in the order that leaves nothing half-removed and reachable: the row
    /// first, so no request can find the site while its tables are going.
    pub async fn remove(&self, tenant: &Tenant) -> AppResult<()> {
        let backend = self.control.get_database_backend();

        self.control
            .execute_raw(Statement::from_sql_and_values(
                backend,
                format!("DELETE FROM tenants WHERE id = {}", placeholder(backend, 1)),
                [tenant.id.to_string().into()],
            ))
            .await?;
        self.forget(tenant.id).await;

        if tenant.database_url.trim().is_empty() {
            self.drop_schema(&tenant.schema).await;
        }

        let root = tenant.root(&self.sites_dir);
        if let Err(err) = tokio::fs::remove_dir_all(&root).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::error!(error = %err, site = %tenant.slug, "could not remove the site folder");
        }

        Ok(())
    }

    /// Closes a site's connection, so the next request opens it afresh —
    /// which is how a site that was switched off or removed stops being served
    /// from something already in memory.
    async fn forget(&self, id: Uuid) {
        self.open.lock().await.retain(|(open, _)| *open != id);
    }

    async fn drop_schema(&self, schema: &str) {
        let statement = Statement::from_string(
            DatabaseBackend::Postgres,
            format!(r#"DROP SCHEMA IF EXISTS "{schema}" CASCADE"#),
        );
        if let Err(err) = self.control.execute_raw(statement).await {
            tracing::error!(schema, error = %err, "could not clean up a half-made site");
        }
    }

    /// The state a request against this site should run with, opening its
    /// connection if it is not already open.
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
            .map_err(|err| {
                AppError::Internal(format!("could not create the site folder: {err}"))
            })?;

        let url = if tenant.database_url.trim().is_empty() {
            &self.base_url
        } else {
            &tenant.database_url
        };
        let db = crate::db::connect_in_schema(url, &tenant.schema)
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

/// A bound parameter, written the way this database spells them.
fn placeholder(backend: DatabaseBackend, position: u8) -> String {
    match backend {
        DatabaseBackend::Postgres => format!("${position}"),
        _ => "?".to_string(),
    }
}

fn tenant_from_row(row: &sea_orm::QueryResult) -> AppResult<Tenant> {
    Ok(Tenant {
        id: Uuid::parse_str(&row.try_get::<String>("", "id")?)
            .map_err(|err| AppError::Internal(format!("bad tenant id: {err}")))?,
        host: row.try_get("", "host")?,
        slug: row.try_get("", "slug")?,
        schema: row.try_get("", "schema_name")?,
        database_url: row.try_get("", "database_url")?,
        organization_id: row
            .try_get::<Option<String>>("", "organization_id")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
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

/// The schema a site's tables go in.
///
/// This ends up inside a quoted identifier in `CREATE SCHEMA`, so it is built
/// from a name that has already been reduced to letters, digits, dashes and
/// underscores rather than trusted — a quote in a schema name would end the
/// identifier and start SQL.
fn schema_name(slug: &str) -> AppResult<String> {
    let name = format!("site_{}", slug.replace('-', "_"));

    if name.len() > 63 {
        return Err(AppError::Validation(
            "the site's name is too long — Postgres allows 58 characters".to_string(),
        ));
    }
    if RESERVED_SCHEMAS.contains(&name.as_str()) {
        return Err(AppError::Validation(
            "that name is reserved by the database".to_string(),
        ));
    }
    Ok(name)
}

async fn create_table(db: &DatabaseConnection) -> AppResult<()> {
    // Written by hand rather than through the migrator: the migrator builds a
    // site's tables, and this is the table that knows what sites there are.
    // `schema` is a reserved word in enough places to be worth avoiding.
    db.execute_raw(Statement::from_string(
        db.get_database_backend(),
        "CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY,
            host TEXT NOT NULL UNIQUE,
            slug TEXT NOT NULL UNIQUE,
            schema_name TEXT NOT NULL,
            database_url TEXT NOT NULL,
            organization_id TEXT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
    ))
    .await?;
    Ok(())
}

/// What the router is built with: the registry, and the state to fall back on.
#[derive(Clone)]
pub struct Hosting {
    /// Absent until the server has a database of its own, since that database
    /// is where the list of sites lives.
    pub registry: Option<Arc<Registry>>,
    /// The site a request that matches no host is served with. This is the
    /// installation that was already there — keeping it means a server hosting
    /// one site behaves exactly as it did before it could host more.
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
    /// Whether this is the server's own installation rather than a site it is
    /// hosting.
    pub fn is_host(&self) -> bool {
        matches!(self, Resolved::Host)
    }

    /// What to call this site in the log. With one site the logs are obviously
    /// about that site; with two hundred, a line that does not say which site
    /// it came from is close to useless.
    pub fn name(&self) -> &str {
        match self {
            Resolved::Host => "host",
            Resolved::Tenant(tenant) => &tenant.slug,
        }
    }
}

impl Hosting {
    /// The server's own key, which is what encrypts anything the control
    /// plane holds — a site's key belongs to that site's data.
    pub fn secrets(&self) -> &crate::crypto::SecretBox {
        &self.default_state.secrets
    }

    /// The site the registry is needed for, or a plain explanation of why
    /// there is no registry to ask.
    pub fn registry(&self) -> AppResult<&Registry> {
        self.registry.as_deref().ok_or_else(|| {
            AppError::Unavailable(
                "the server has no database yet, so it cannot host sites".to_string(),
            )
        })
    }

    /// The state a request should run with, chosen by the host it asked for.
    pub async fn resolve_host(&self, host: Option<&str>) -> AppResult<(AppState, Resolved)> {
        let (Some(host), Some(registry)) = (host, self.registry.as_deref()) else {
            return Ok((self.default_state.clone(), Resolved::Host));
        };
        match registry.find_by_host(host).await? {
            Some(tenant) if tenant.active => Ok((
                registry.state_for(&tenant).await?,
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
    use super::{clean_slug, schema_name};

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

    #[test]
    fn a_schema_name_cannot_carry_sql() {
        // The name goes inside a quoted identifier, so what matters is that a
        // quote never survives to close it.
        let name = schema_name(&clean_slug(r#"a"; DROP SCHEMA public CASCADE; --"#).unwrap());
        assert_eq!(name.unwrap(), "site_adropschemapubliccascade__");
    }

    #[test]
    fn a_schema_name_is_prefixed_and_underscored() {
        assert_eq!(schema_name("example-com").unwrap(), "site_example_com");
        // The prefix is also what keeps a site off a name Postgres owns.
        assert_eq!(schema_name("public").unwrap(), "site_public");
    }

    #[test]
    fn a_schema_name_postgres_would_truncate_is_refused() {
        // Postgres silently cuts identifiers at 63 bytes, which would let two
        // long names land on the same schema.
        assert!(schema_name(&"a".repeat(58)).unwrap().len() == 63);
        assert!(schema_name(&"a".repeat(59)).is_err());
    }
}
