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
    let mut options = options(database_url);
    options.set_schema_search_path(schema.to_owned());

    let db = Database::connect(options).await?;
    Migrator::up(&db, None).await?;
    Ok(db)
}
