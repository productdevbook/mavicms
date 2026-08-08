//! What the server itself provides to the sites it hosts.
//!
//! A customer who wants their contact form to email them should not have to
//! open an AWS account first. So the server can hold one set of Amazon
//! credentials and let sites send through it — and the whole of the difficulty
//! is that reputation is shared: one site with a careless list gets everybody
//! else's mail put in spam.
//!
//! Amazon's own answer to that is a tenant, which is a container for a site's
//! identities and its own reputation, and can be stopped on its own. Every
//! send made on the server's behalf names one, so a site that starts
//! collecting complaints is the only one that suffers for it.
//!
//! The second half is ours: a site gets a number of messages a day, small to
//! begin with, and whoever runs the server decides when it grows.

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    crypto::SecretBox,
    error::{AppError, AppResult},
};

/// What a site gets on its first day using the server's account.
///
/// Small on purpose. Amazon judges an account by what leaves it, and the way
/// a shared account is ruined is one new site sending its first campaign to a
/// list somebody bought. Two hundred is enough for a contact form and a
/// handful of notifications, and enough to find out whether the site is what
/// it says it is before it is trusted with more.
pub const A_DAY_TO_BEGIN_WITH: i64 = 200;

/// Where the server's own Amazon credentials are kept.
const SES_SETTING: &str = "platform_ses";

/// How a site sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sends {
    /// With its own Amazon account. Nothing here applies.
    Own,
    /// Through the server's, as its own Amazon tenant.
    Shared,
}

impl Sends {
    pub fn as_str(&self) -> &'static str {
        match self {
            Sends::Own => "own",
            Sends::Shared => "shared",
        }
    }

    /// Not `FromStr`: this cannot fail, and anything unrecognised is the
    /// site's own account — the reading that cannot accidentally spend
    /// somebody else's reputation.
    pub fn read(value: &str) -> Self {
        match value.trim() {
            "shared" => Sends::Shared,
            _ => Sends::Own,
        }
    }
}

/// What the server allows one site, and what it has used today.
#[derive(Debug, Clone, Serialize)]
pub struct Allowance {
    pub tenant_id: Uuid,
    pub host: String,
    pub sends: String,
    /// Messages a day. Nothing is sent past it.
    pub a_day: i64,
    /// The Amazon tenant this site's mail is attributed to, once made.
    pub ses_tenant: Option<String>,
    pub created_at: DateTime<FixedOffset>,
}

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
        // One row per name. Hand-written like the rest of the control plane,
        // because none of this belongs to a site.
        "CREATE TABLE IF NOT EXISTS platform_settings (
            name TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS site_email_allowance (
            tenant_id TEXT PRIMARY KEY,
            sends TEXT NOT NULL,
            a_day INTEGER NOT NULL,
            ses_tenant TEXT NULL,
            created_at TEXT NOT NULL
        )",
        // Which site added which sender. On a shared account the Amazon
        // identity list is the whole server's, so without this a site would
        // see every other site's domains and could delete them.
        "CREATE TABLE IF NOT EXISTS site_email_identity (
            tenant_id TEXT NOT NULL,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (tenant_id, name)
        )",
    ] {
        db.execute_raw(Statement::from_string(db.get_database_backend(), statement))
            .await?;
    }
    Ok(())
}

/// The server's own Amazon credentials, in the clear. Never leaves the server:
/// a site is told it may send, not what with.
pub async fn ses(
    db: &DatabaseConnection,
    secrets: &SecretBox,
) -> AppResult<Option<crate::email::EmailConfig>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT value FROM platform_settings WHERE name = {}",
                parameter(backend, 1)
            ),
            [SES_SETTING.into()],
        ))
        .await?;

    let Some(row) = row else { return Ok(None) };
    let stored: String = row.try_get("", "value")?;
    if stored.is_empty() {
        return Ok(None);
    }
    let clear = secrets.decrypt(&stored)?;
    serde_json::from_str(&clear).map(Some).map_err(|err| {
        AppError::Internal(format!("the server's mail settings are unreadable: {err}"))
    })
}

pub async fn set_ses(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    config: &crate::email::EmailConfig,
) -> AppResult<()> {
    let backend = db.get_database_backend();
    let value = secrets.encrypt(&serde_json::to_string(config).map_err(|err| {
        AppError::Internal(format!("could not store the server's mail settings: {err}"))
    })?)?;
    let now = Utc::now().to_rfc3339();

    let existing = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT name FROM platform_settings WHERE name = {}",
                parameter(backend, 1)
            ),
            [SES_SETTING.into()],
        ))
        .await?;

    let statement = if existing.is_some() {
        Statement::from_sql_and_values(
            backend,
            format!(
                "UPDATE platform_settings SET value = {}, updated_at = {} WHERE name = {}",
                parameter(backend, 1),
                parameter(backend, 2),
                parameter(backend, 3)
            ),
            [value.into(), now.into(), SES_SETTING.into()],
        )
    } else {
        Statement::from_sql_and_values(
            backend,
            format!(
                "INSERT INTO platform_settings (name, value, updated_at) VALUES ({})",
                parameters(backend, 3)
            ),
            [SES_SETTING.into(), value.into(), now.into()],
        )
    };

    db.execute_raw(statement).await?;
    Ok(())
}

/// What one site is allowed. A site nobody has decided about sends with its
/// own account and is allowed nothing here, which is the safe way round.
pub async fn allowance(db: &DatabaseConnection, tenant_id: Uuid) -> AppResult<Option<Allowance>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT tenant_id, sends, a_day, ses_tenant, created_at \
                 FROM site_email_allowance WHERE tenant_id = {}",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;

    let Some(row) = row else { return Ok(None) };
    Ok(Some(Allowance {
        tenant_id,
        host: String::new(),
        sends: row.try_get("", "sends")?,
        a_day: row.try_get("", "a_day")?,
        ses_tenant: row.try_get::<Option<String>>("", "ses_tenant")?,
        created_at: parse_time(&row.try_get::<String>("", "created_at")?)?,
    }))
}

/// Says how a site sends, and how much. Makes the row when there is none.
pub async fn set_allowance(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    sends: Sends,
    a_day: i64,
    ses_tenant: Option<&str>,
) -> AppResult<()> {
    // A number nobody can make sense of is a number that was a mistake: zero
    // is "stopped", and the ceiling is above any real site and below anything
    // that would empty the account's own quota in an afternoon.
    let a_day = a_day.clamp(0, 200_000);
    let backend = db.get_database_backend();
    let existing = allowance(db, tenant_id).await?;

    let statement = if existing.is_some() {
        Statement::from_sql_and_values(
            backend,
            format!(
                "UPDATE site_email_allowance SET sends = {}, a_day = {}, ses_tenant = {} \
                 WHERE tenant_id = {}",
                parameter(backend, 1),
                parameter(backend, 2),
                parameter(backend, 3),
                parameter(backend, 4)
            ),
            [
                sends.as_str().into(),
                a_day.into(),
                ses_tenant.map(str::to_string).into(),
                tenant_id.to_string().into(),
            ],
        )
    } else {
        Statement::from_sql_and_values(
            backend,
            format!(
                "INSERT INTO site_email_allowance (tenant_id, sends, a_day, ses_tenant, created_at) \
                 VALUES ({})",
                parameters(backend, 5)
            ),
            [
                tenant_id.to_string().into(),
                sends.as_str().into(),
                a_day.into(),
                ses_tenant.map(str::to_string).into(),
                Utc::now().to_rfc3339().into(),
            ],
        )
    };

    db.execute_raw(statement).await?;
    Ok(())
}

/// Every site the server sends for.
pub async fn allowances(db: &DatabaseConnection) -> AppResult<Vec<Allowance>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT a.tenant_id, a.sends, a.a_day, a.ses_tenant, a.created_at, \
             COALESCE(t.host, '') AS host \
             FROM site_email_allowance a LEFT JOIN tenants t ON t.id = a.tenant_id \
             ORDER BY host",
        ))
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(Allowance {
                tenant_id: Uuid::parse_str(&row.try_get::<String>("", "tenant_id")?)
                    .map_err(|err| AppError::Internal(format!("bad site id: {err}")))?,
                host: row.try_get("", "host")?,
                sends: row.try_get("", "sends")?,
                a_day: row.try_get("", "a_day")?,
                ses_tenant: row.try_get::<Option<String>>("", "ses_tenant")?,
                created_at: parse_time(&row.try_get::<String>("", "created_at")?)?,
            })
        })
        .collect()
}

/// The senders one site has added to the server's account.
pub async fn identities(db: &DatabaseConnection, tenant_id: Uuid) -> AppResult<Vec<String>> {
    let backend = db.get_database_backend();
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT name FROM site_email_identity WHERE tenant_id = {} ORDER BY name",
                parameter(backend, 1)
            ),
            [tenant_id.to_string().into()],
        ))
        .await?;
    rows.into_iter()
        .map(|row| Ok(row.try_get("", "name")?))
        .collect()
}

/// Whether this site may speak for this sender.
///
/// A domain answers for its own subdomains: a site that verified
/// `example.com` gets `mail.example.com` without asking again, which is what
/// a custom MAIL FROM is.
pub async fn owns_identity(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    name: &str,
) -> AppResult<bool> {
    let name = name.trim().to_ascii_lowercase();
    let mine = identities(db, tenant_id).await?;
    Ok(mine.iter().any(|held| {
        let held = held.trim().to_ascii_lowercase();
        name == held || name.ends_with(&format!(".{held}")) || name.ends_with(&format!("@{held}"))
    }))
}

/// Whether anybody else has this sender already. The check that stops one
/// site claiming another's domain on a shared account.
pub async fn identity_taken(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    name: &str,
) -> AppResult<bool> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT tenant_id FROM site_email_identity WHERE name = {} AND tenant_id <> {}",
                parameter(backend, 1),
                parameter(backend, 2)
            ),
            [
                name.trim().to_ascii_lowercase().into(),
                tenant_id.to_string().into(),
            ],
        ))
        .await?;
    Ok(row.is_some())
}

pub async fn remember_identity(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    name: &str,
) -> AppResult<()> {
    let backend = db.get_database_backend();
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO site_email_identity (tenant_id, name, created_at) VALUES ({})",
            parameters(backend, 3)
        ),
        [
            tenant_id.to_string().into(),
            name.trim().to_ascii_lowercase().into(),
            Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;
    Ok(())
}

pub async fn forget_identity(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    name: &str,
) -> AppResult<()> {
    let backend = db.get_database_backend();
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "DELETE FROM site_email_identity WHERE tenant_id = {} AND name = {}",
            parameter(backend, 1),
            parameter(backend, 2)
        ),
        [
            tenant_id.to_string().into(),
            name.trim().to_ascii_lowercase().into(),
        ],
    ))
    .await?;
    Ok(())
}

/// A name Amazon will accept for this site's tenant.
///
/// Letters, numbers, hyphen and underscore, up to sixty-four — Amazon's rule,
/// not ours. A site slug is already close, so this mostly guards against the
/// day slugs learn to hold something else.
pub fn tenant_name(slug: &str) -> String {
    let cleaned: String = slug
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    cleaned.trim_matches('-').chars().take(64).collect()
}

fn parse_time(value: &str) -> AppResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|err| AppError::Internal(format!("unreadable time {value}: {err}")))
}

#[cfg(test)]
mod tests {
    use super::{A_DAY_TO_BEGIN_WITH, Sends, tenant_name};

    #[test]
    fn a_tenant_name_is_something_amazon_will_take() {
        assert_eq!(tenant_name("ornek-test-com"), "ornek-test-com");
        // A slug cannot hold these today. The rule is Amazon's and outlives
        // what our slugs happen to allow.
        assert_eq!(tenant_name("bir.site.com"), "bir-site-com");
        assert_eq!(tenant_name("--kenarlar--"), "kenarlar");
        assert_eq!(tenant_name(&"a".repeat(200)).len(), 64);
    }

    #[test]
    fn how_a_site_sends_survives_a_round_trip() {
        for one in [Sends::Own, Sends::Shared] {
            assert_eq!(Sends::read(one.as_str()), one);
        }
        // Anything unrecognised is its own account, which is the reading that
        // cannot accidentally spend somebody else's reputation.
        assert_eq!(Sends::read(""), Sends::Own);
        assert_eq!(Sends::read("paylasimli"), Sends::Own);
    }

    #[tokio::test]
    async fn a_site_answers_for_its_own_domain_and_nobody_elses() {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        super::create_tables(&db).await.unwrap();

        let one = uuid::Uuid::from_u128(1);
        let other = uuid::Uuid::from_u128(2);
        super::remember_identity(&db, one, "Ornek.Test")
            .await
            .unwrap();
        super::remember_identity(&db, other, "baska.test")
            .await
            .unwrap();

        assert!(super::owns_identity(&db, one, "ornek.test").await.unwrap());
        // A custom MAIL FROM is a subdomain of a domain already verified, so
        // refusing it here would refuse the records the panel itself hands out.
        assert!(
            super::owns_identity(&db, one, "mail.ornek.test")
                .await
                .unwrap()
        );
        assert!(
            super::owns_identity(&db, one, "bilgi@ornek.test")
                .await
                .unwrap()
        );

        // The whole point: the shared account's list is everybody's.
        assert!(!super::owns_identity(&db, one, "baska.test").await.unwrap());
        // A name that merely ends the same way is a different domain.
        assert!(
            !super::owns_identity(&db, one, "notornek.test")
                .await
                .unwrap()
        );

        assert!(super::identity_taken(&db, one, "baska.test").await.unwrap());
        assert!(!super::identity_taken(&db, one, "ornek.test").await.unwrap());

        super::forget_identity(&db, one, "ornek.test")
            .await
            .unwrap();
        assert!(!super::owns_identity(&db, one, "ornek.test").await.unwrap());
    }

    #[test]
    fn the_first_days_allowance_is_small() {
        // Whatever it is set to, it is a day's worth of a contact form and
        // not a day's worth of a mailing list.
        assert_eq!(A_DAY_TO_BEGIN_WITH, 200);
    }
}
