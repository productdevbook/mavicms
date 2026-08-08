use std::time::Duration;

use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

/// How many connections one site may hold.
///
/// The pool's own default is ten, which is a sensible number for a server
/// running one site and a ruinous one for a server running hundreds: every
/// Postgres connection is a process on the database server, so the default
/// would let a busy afternoon open more of them than Postgres will accept.
/// Two is enough for a site to read while it writes.
const SITE_CONNECTIONS: u32 = 2;

/// How long an unused connection is kept before it is closed.
///
/// This is what makes a quiet site cost nothing. Sites hold no connections
/// when nobody is reading them, so the number open at any moment reflects the
/// sites being used rather than the sites that exist.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

fn options(database_url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(database_url.to_owned());
    options
        .max_connections(SITE_CONNECTIONS)
        .min_connections(0)
        .idle_timeout(IDLE_TIMEOUT)
        .acquire_timeout(Duration::from_secs(15));
    options
}

/// Opens a database without bringing a site's schema up to date — for
/// something that only reads the control plane and has no business migrating
/// anything.
pub async fn connect_plain(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    Database::connect(options(database_url)).await
}

pub async fn connect(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let db = Database::connect(options(database_url)).await?;
    Migrator::up(&db, None).await?;
    Ok(db)
}

/// Opens a database with every query confined to one Postgres schema, and
/// brings that schema's tables up to date.
///
/// The search path is set on each connection as it is opened, and holds only
/// the site's own schema — not `public` after it. A missing table is then an
/// error rather than a quiet read of the server's own copy, which is the
/// difference between a bug that shows up immediately and one that shows a
/// customer someone else's posts.
pub async fn connect_in_schema(
    database_url: &str,
    schema: &str,
) -> Result<DatabaseConnection, DbErr> {
    let db = open_in_schema(database_url, schema).await?;
    Migrator::up(&db, None).await?;
    // The only moment that happens for every site however it was made. A site
    // opened from the console never runs the setup wizard, and one with no
    // language files its content under a language it does not have.
    crate::languages::ensure_site_languages(&db)
        .await
        .map_err(|err| DbErr::Custom(format!("could not prepare the site's languages: {err}")))?;
    Ok(db)
}

/// The same, without migrating — for something that only writes rows a site
/// already has. Migrating is what a request that opens a site does; a
/// background task doing it as well would run them on a schedule, against
/// sites nobody has asked for.
pub async fn connect_plain_in_schema(
    database_url: &str,
    schema: &str,
) -> Result<DatabaseConnection, DbErr> {
    open_in_schema(database_url, schema).await
}

async fn open_in_schema(database_url: &str, schema: &str) -> Result<DatabaseConnection, DbErr> {
    let mut options = options(database_url);
    options.set_schema_search_path(schema.to_owned());

    Database::connect(options).await
}
