//! Posts whose time has come.
//!
//! "Scheduled" was a label and nothing else: a post given a date sat at that
//! status for ever, and the site it belonged to never heard about it. Somebody
//! writing on Friday for Monday morning came back on Monday afternoon to a
//! post that had not gone out.
//!
//! What makes this awkward is that a server hosts hundreds of sites and keeps
//! at most a few of them open, so the obvious loop — open each site, ask it
//! what is due — would open every site on the machine every minute, run every
//! site's migrations while doing it, and push the sites people are actually
//! reading out of the cache. Sites on the server's own database are therefore
//! reached across the schema boundary on the connection the control plane
//! already holds, which costs one statement each and opens nothing.

use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};
use uuid::Uuid;

use crate::{
    dto::post::PostStatus,
    error::{AppError, AppResult},
    tenants::{Hosting, Tenant},
};

/// How often the clock is read.
///
/// A publish date is picked to the minute, so a minute is the coarsest tick
/// that keeps the promise the panel makes. It costs one statement per site.
const TICK: Duration = Duration::from_secs(60);

/// Publishes what is due, everywhere, for as long as the server runs.
pub fn spawn(hosting: Hosting) {
    tokio::spawn(async move {
        // Sites with a database of their own cannot be reached across a schema
        // boundary, so each keeps a connection here. Made once and held: a
        // background task has no business running a site's migrations, and
        // reopening one a minute would be a connection storm on a timer.
        let mut elsewhere: HashMap<Uuid, DatabaseConnection> = HashMap::new();

        loop {
            tokio::time::sleep(TICK).await;
            run(&hosting, &mut elsewhere).await;
        }
    });
}

async fn run(hosting: &Hosting, elsewhere: &mut HashMap<Uuid, DatabaseConnection>) {
    let now = Utc::now().fixed_offset();

    // The installation the server itself runs. It has posts like any site and
    // nothing to build: publishing belongs to a hosted site.
    if let Some(db) = hosting.default_state.db.as_ref() {
        match publish_due(db, "posts", now).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(posts = n, "published what was due"),
            Err(err) => tracing::warn!(error = %err, "could not publish what was due"),
        }
    }

    let Some(registry) = hosting.registry.as_deref() else {
        return;
    };

    let tenants = match registry.all().await {
        Ok(tenants) => tenants,
        Err(err) => {
            tracing::warn!(error = %err, "could not read the list of sites");
            return;
        }
    };

    for tenant in tenants {
        // A site that has been switched off answers nothing; it should not
        // start publishing behind the switch either.
        if !tenant.active {
            continue;
        }

        let moved = match due_for(registry, elsewhere, &tenant, now).await {
            Ok(moved) => moved,
            Err(err) => {
                tracing::warn!(site = %tenant.slug, error = %err, "could not publish what was due");
                continue;
            }
        };
        if moved == 0 {
            continue;
        }

        tracing::info!(site = %tenant.slug, posts = moved, "published what was due");

        // The post is online in the API the moment it moves; the pages are
        // not, and a post published at nine that appears whenever somebody
        // next presses the button is the same broken promise in a later place.
        match crate::publish::request(registry.control(), tenant.id).await {
            Ok(_) => {}
            // A site nobody has told where its pages come from. Not a fault:
            // plenty of sites are read through the API and build elsewhere.
            Err(AppError::Validation(_)) => {}
            Err(err) => {
                tracing::warn!(site = %tenant.slug, error = %err, "could not ask for a build")
            }
        }
    }
}

async fn due_for(
    registry: &crate::tenants::Registry,
    elsewhere: &mut HashMap<Uuid, DatabaseConnection>,
    tenant: &Tenant,
    now: DateTime<FixedOffset>,
) -> AppResult<u64> {
    if tenant.database_url.trim().is_empty() {
        return publish_due(registry.control(), &qualified(&tenant.schema)?, now).await;
    }

    let db = match elsewhere.entry(tenant.id) {
        std::collections::hash_map::Entry::Occupied(held) => held.into_mut(),
        std::collections::hash_map::Entry::Vacant(empty) => empty.insert(
            crate::db::connect_plain_in_schema(&tenant.database_url, &tenant.schema)
                .await
                .map_err(|err| {
                    AppError::Internal(format!("could not open the site's database: {err}"))
                })?,
        ),
    };

    publish_due(db, "posts", now).await
}

/// A site's posts table, named from outside the site.
///
/// The name is interpolated rather than bound — no database takes a table name
/// as a parameter — so it is checked here rather than trusted. Schema names
/// are made from a slug that cannot hold anything else, which is the reason
/// this has never had anything to reject and no reason at all to stop looking.
fn qualified(schema: &str) -> AppResult<String> {
    if schema.is_empty()
        || !schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(AppError::Internal(format!(
            "{schema} is not a name this can safely reach"
        )));
    }
    Ok(format!(r#""{schema}".posts"#))
}

/// Moves every post whose time has passed to published, and says how many.
///
/// A scheduled post with no date is left where it is. It is not a schedule,
/// and guessing a time for it would put somebody's draft online.
async fn publish_due(
    db: &impl ConnectionTrait,
    table: &str,
    now: DateTime<FixedOffset>,
) -> AppResult<u64> {
    let backend = db.get_database_backend();
    let mut placeholder = (1..).map(|n| match backend {
        sea_orm::DatabaseBackend::Postgres => format!("${n}"),
        _ => "?".to_string(),
    });

    let result = db
        .execute_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "UPDATE {table} SET status = {}, updated_at = {} \
                 WHERE status = {} AND publish_at IS NOT NULL AND publish_at <= {}",
                placeholder.next().expect("the sequence is infinite"),
                placeholder.next().expect("the sequence is infinite"),
                placeholder.next().expect("the sequence is infinite"),
                placeholder.next().expect("the sequence is infinite"),
            ),
            [
                PostStatus::Published.as_str().into(),
                now.into(),
                PostStatus::Scheduled.as_str().into(),
                now.into(),
            ],
        ))
        .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::qualified;

    #[test]
    fn a_schema_is_quoted() {
        assert_eq!(qualified("site_one").unwrap(), r#""site_one".posts"#);
    }

    #[test]
    fn a_name_that_could_end_the_quoting_is_refused() {
        assert!(qualified(r#"site"; DROP TABLE posts; --"#).is_err());
        assert!(qualified("site one").is_err());
        assert!(qualified("").is_err());
    }
}
