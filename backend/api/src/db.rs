use std::time::Duration;

use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

/// How many connections one site may hold.
///
/// The pool's own default is ten, which is a sensible number for a server
/// running one database and a wasteful one for a server running hundreds:
/// every open SQLite connection carries its own page cache, so the default
/// costs about twenty megabytes per site whether or not the site is busy.
/// Four is enough for a site to read while it writes, and the ceiling of what
/// a single SQLite file can use anyway — writes take turns regardless.
const MAX_CONNECTIONS: u32 = 4;

/// How long an unused connection is kept before it is closed.
///
/// This is what makes a quiet site cost nothing. A site that served a request
/// this morning and none since should not still be holding page caches at
/// lunchtime; a minute later, its memory is back.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

fn options(database_url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(database_url.to_owned());
    options
        .max_connections(MAX_CONNECTIONS)
        .min_connections(0)
        .idle_timeout(IDLE_TIMEOUT)
        .acquire_timeout(Duration::from_secs(15));
    options
}

/// Opens a database without running the site schema over it — for the small
/// registry that only records which sites exist.
pub async fn connect_plain(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    Database::connect(options(database_url)).await
}

pub async fn connect(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let db = Database::connect(options(database_url)).await?;
    Migrator::up(&db, None).await?;
    Ok(db)
}
