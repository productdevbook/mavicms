//! Agencies, and the sites they own.
//!
//! Three kinds of people use this server, and they are deliberately different
//! things rather than one account type with flags:
//!
//! - whoever runs the server, who signs in to its own installation and sees
//!   every site on the machine;
//! - an agency, which signs in here, and can see and open sites of its own —
//!   and nobody else's;
//! - the people who write the site, who sign in to their own site and know
//!   nothing about any of this.
//!
//! An agency's account lives in the control plane rather than in any site,
//! because it is not a person on any one site: it is who a set of sites
//! belongs to. Signing in to one of those sites is a separate act, handled by
//! a short-lived token rather than by making the agency's password work
//! everywhere — a password that opens fifty sites is fifty times the loss.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const CONSOLE_COOKIE: &str = "mavicms_console";
const SESSION_TTL_DAYS: i64 = 30;

/// How long the hand-off from this console into a site's dashboard is good
/// for. It travels in a URL, which is the least private place a secret can be
/// — browser history, proxy logs, whatever the link is pasted into — so it is
/// usable once and stops being valid before any of that matters.
const ENTRY_TTL_SECONDS: i64 = 120;

/// How many sites a new agency may open before someone says otherwise.
const DEFAULT_SITE_LIMIT: i32 = 10;

#[derive(Debug, Clone)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub site_limit: i32,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct Operator {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub email: String,
    pub name: String,
}

/// Creates the control-plane tables. Hand-written for the same reason the
/// tenant table is: the migrator builds a site's schema, and none of this
/// belongs to a site.
pub async fn create_tables(db: &DatabaseConnection) -> AppResult<()> {
    for statement in [
        "CREATE TABLE IF NOT EXISTS organizations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            site_limit INTEGER NOT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS operators (
            id TEXT PRIMARY KEY,
            organization_id TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS operator_sessions (
            id TEXT PRIMARY KEY,
            operator_id TEXT NOT NULL,
            expires_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS site_entries (
            token TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            operator_id TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            used INTEGER NOT NULL
        )",
    ] {
        db.execute_raw(Statement::from_string(db.get_database_backend(), statement))
            .await?;
    }
    Ok(())
}

fn parameter(backend: DatabaseBackend, position: u8) -> String {
    match backend {
        DatabaseBackend::Postgres => format!("${position}"),
        _ => "?".to_string(),
    }
}

/// `$1, $2, …` or `?, ?, …`, whichever this database understands.
fn parameters(backend: DatabaseBackend, count: u8) -> String {
    (1..=count)
        .map(|n| parameter(backend, n))
        .collect::<Vec<_>>()
        .join(", ")
}

fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::Internal(format!("could not hash the password: {err}")))
}

fn parse_time(value: &str) -> AppResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|err| AppError::Internal(format!("unreadable timestamp: {err}")))
}

/// Registers an agency and its first account, and signs that account in.
///
/// The two happen together on purpose: an agency with no way to sign in is a
/// row nobody can reach, and an account with no agency has nothing to own.
pub async fn register(
    db: &DatabaseConnection,
    organization_name: &str,
    name: &str,
    email: &str,
    password: &str,
) -> AppResult<(Organization, Operator)> {
    let backend = db.get_database_backend();
    let organization_name = organization_name.trim();
    let name = name.trim();
    let email = email.trim().to_lowercase();

    if organization_name.is_empty() {
        return Err(AppError::Validation("the agency needs a name".to_string()));
    }
    if !email.contains('@') || email.len() < 3 {
        return Err(AppError::Validation(
            "that does not look like an email address".to_string(),
        ));
    }
    if password.chars().count() < 10 {
        return Err(AppError::Validation(
            "the password must be at least 10 characters".to_string(),
        ));
    }
    if find_operator_by_email(db, &email).await?.is_some() {
        return Err(AppError::Conflict(
            "an account already uses that email address".to_string(),
        ));
    }

    let organization = Organization {
        id: Uuid::new_v4(),
        name: organization_name.to_string(),
        site_limit: DEFAULT_SITE_LIMIT,
        active: true,
    };
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO organizations (id, name, site_limit, active, created_at) VALUES ({})",
            parameters(backend, 5)
        ),
        [
            organization.id.to_string().into(),
            organization.name.clone().into(),
            organization.site_limit.into(),
            1.into(),
            Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;

    let operator = Operator {
        id: Uuid::new_v4(),
        organization_id: organization.id,
        email,
        name: if name.is_empty() {
            organization.name.clone()
        } else {
            name.to_string()
        },
    };
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO operators \
             (id, organization_id, email, name, password_hash, active, created_at) \
             VALUES ({})",
            parameters(backend, 7)
        ),
        [
            operator.id.to_string().into(),
            operator.organization_id.to_string().into(),
            operator.email.clone().into(),
            operator.name.clone().into(),
            hash_password(password)?.into(),
            1.into(),
            Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;

    Ok((organization, operator))
}

async fn find_operator_by_email(
    db: &DatabaseConnection,
    email: &str,
) -> AppResult<Option<(Operator, String, bool)>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id, organization_id, email, name, password_hash, active \
                 FROM operators WHERE email = {}",
                parameter(backend, 1)
            ),
            [email.into()],
        ))
        .await?;

    let Some(row) = row else { return Ok(None) };
    Ok(Some((
        Operator {
            id: Uuid::parse_str(&row.try_get::<String>("", "id")?)
                .map_err(|err| AppError::Internal(format!("bad operator id: {err}")))?,
            organization_id: Uuid::parse_str(&row.try_get::<String>("", "organization_id")?)
                .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
            email: row.try_get("", "email")?,
            name: row.try_get("", "name")?,
        },
        row.try_get("", "password_hash")?,
        row.try_get::<i32>("", "active")? != 0,
    )))
}

/// Checks an email and password, taking the same time whether or not the
/// account exists — an error that comes back faster for an unknown address
/// tells whoever is asking which addresses are worth attacking.
pub async fn authenticate(
    db: &DatabaseConnection,
    email: &str,
    password: &str,
) -> AppResult<Operator> {
    let invalid = || AppError::Unauthorized("invalid email or password".to_string());
    let found = find_operator_by_email(db, &email.trim().to_lowercase()).await?;

    let (operator, hash, active) = match found {
        Some(found) => found,
        None => {
            // A hash of nothing, to spend the time an argon2 verify would.
            let _ = hash_password(password);
            return Err(invalid());
        }
    };

    let parsed = PasswordHash::new(&hash)
        .map_err(|_| AppError::Internal("corrupt password hash".to_string()))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| invalid())?;

    if !active {
        return Err(AppError::Forbidden(
            "this account has been switched off".to_string(),
        ));
    }
    Ok(operator)
}

pub async fn create_session(db: &DatabaseConnection, operator_id: Uuid) -> AppResult<Uuid> {
    let backend = db.get_database_backend();
    let id = Uuid::new_v4();
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO operator_sessions (id, operator_id, expires_at) VALUES ({})",
            parameters(backend, 3)
        ),
        [
            id.to_string().into(),
            operator_id.to_string().into(),
            (Utc::now() + Duration::days(SESSION_TTL_DAYS))
                .to_rfc3339()
                .into(),
        ],
    ))
    .await?;
    Ok(id)
}

pub async fn delete_session(db: &DatabaseConnection, session_id: Uuid) {
    let backend = db.get_database_backend();
    let statement = Statement::from_sql_and_values(
        backend,
        format!(
            "DELETE FROM operator_sessions WHERE id = {}",
            parameter(backend, 1)
        ),
        [session_id.to_string().into()],
    );
    let _ = db.execute_raw(statement).await;
}

/// The agency behind a console session, if it is still good.
pub async fn session_operator(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<Operator>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT o.id, o.organization_id, o.email, o.name, s.expires_at, o.active \
                 FROM operator_sessions s JOIN operators o ON o.id = s.operator_id \
                 WHERE s.id = {}",
                parameter(backend, 1)
            ),
            [session_id.to_string().into()],
        ))
        .await?;

    let Some(row) = row else { return Ok(None) };
    if parse_time(&row.try_get::<String>("", "expires_at")?)? < Utc::now() {
        delete_session(db, session_id).await;
        return Ok(None);
    }
    if row.try_get::<i32>("", "active")? == 0 {
        return Ok(None);
    }

    Ok(Some(Operator {
        id: Uuid::parse_str(&row.try_get::<String>("", "id")?)
            .map_err(|err| AppError::Internal(format!("bad operator id: {err}")))?,
        organization_id: Uuid::parse_str(&row.try_get::<String>("", "organization_id")?)
            .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
        email: row.try_get("", "email")?,
        name: row.try_get("", "name")?,
    }))
}

pub async fn organization(db: &DatabaseConnection, id: Uuid) -> AppResult<Option<Organization>> {
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id, name, site_limit, active FROM organizations WHERE id = {}",
                parameter(backend, 1)
            ),
            [id.to_string().into()],
        ))
        .await?;

    row.map(|row| {
        Ok(Organization {
            id: Uuid::parse_str(&row.try_get::<String>("", "id")?)
                .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
            name: row.try_get("", "name")?,
            site_limit: row.try_get("", "site_limit")?,
            active: row.try_get::<i32>("", "active")? != 0,
        })
    })
    .transpose()
}

/// Mints the one-time token that lets an agency walk into one of its sites.
pub async fn create_entry(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    operator_id: Uuid,
) -> AppResult<String> {
    let backend = db.get_database_backend();
    let token = Uuid::new_v4().to_string();
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO site_entries (token, tenant_id, operator_id, expires_at, used) \
             VALUES ({})",
            parameters(backend, 5)
        ),
        [
            token.clone().into(),
            tenant_id.to_string().into(),
            operator_id.to_string().into(),
            (Utc::now() + Duration::seconds(ENTRY_TTL_SECONDS))
                .to_rfc3339()
                .into(),
            0.into(),
        ],
    ))
    .await?;
    Ok(token)
}

/// Spends an entry token, returning who it was for.
///
/// Marking it used is part of the same question as reading it: two requests
/// arriving together with the same token must not both be told yes, so the
/// update carries the `used = 0` condition and whichever one changes a row is
/// the one that gets in.
pub async fn claim_entry(
    db: &DatabaseConnection,
    token: &str,
    tenant_id: Uuid,
) -> AppResult<Operator> {
    let backend = db.get_database_backend();
    let refused = || AppError::Unauthorized("that sign-in link is no longer valid".to_string());

    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT tenant_id, operator_id, expires_at, used FROM site_entries \
                 WHERE token = {}",
                parameter(backend, 1)
            ),
            [token.into()],
        ))
        .await?
        .ok_or_else(refused)?;

    let for_tenant = Uuid::parse_str(&row.try_get::<String>("", "tenant_id")?)
        .map_err(|err| AppError::Internal(format!("bad tenant id: {err}")))?;
    if for_tenant != tenant_id
        || row.try_get::<i32>("", "used")? != 0
        || parse_time(&row.try_get::<String>("", "expires_at")?)? < Utc::now()
    {
        return Err(refused());
    }

    let spent = db
        .execute_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "UPDATE site_entries SET used = 1 WHERE token = {} AND used = 0",
                parameter(backend, 1)
            ),
            [token.into()],
        ))
        .await?;
    if spent.rows_affected() == 0 {
        return Err(refused());
    }

    let operator_id = Uuid::parse_str(&row.try_get::<String>("", "operator_id")?)
        .map_err(|err| AppError::Internal(format!("bad operator id: {err}")))?;

    let found = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id, organization_id, email, name FROM operators \
                 WHERE id = {} AND active = 1",
                parameter(backend, 1)
            ),
            [operator_id.to_string().into()],
        ))
        .await?
        .ok_or_else(refused)?;

    Ok(Operator {
        id: operator_id,
        organization_id: Uuid::parse_str(&found.try_get::<String>("", "organization_id")?)
            .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
        email: found.try_get("", "email")?,
        name: found.try_get("", "name")?,
    })
}

/// Changes an agency account's own details.
///
/// The current password is asked for even though the session already proves
/// who this is: a session left open on a shared machine should not be enough
/// to take the account away from whoever owns it.
pub async fn update_account(
    db: &DatabaseConnection,
    operator_id: Uuid,
    current_password: &str,
    name: Option<&str>,
    email: Option<&str>,
    new_password: Option<&str>,
) -> AppResult<Operator> {
    let backend = db.get_database_backend();

    let (operator, hash, _) = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id, organization_id, email, name, password_hash, active \
                 FROM operators WHERE id = {}",
                parameter(backend, 1)
            ),
            [operator_id.to_string().into()],
        ))
        .await?
        .map(|row| {
            Ok::<_, AppError>((
                Operator {
                    id: operator_id,
                    organization_id: Uuid::parse_str(
                        &row.try_get::<String>("", "organization_id")?,
                    )
                    .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
                    email: row.try_get("", "email")?,
                    name: row.try_get("", "name")?,
                },
                row.try_get::<String>("", "password_hash")?,
                row.try_get::<i32>("", "active")? != 0,
            ))
        })
        .transpose()?
        .ok_or_else(|| AppError::NotFound("account".to_string()))?;

    let parsed = PasswordHash::new(&hash)
        .map_err(|_| AppError::Internal("corrupt password hash".to_string()))?;
    Argon2::default()
        .verify_password(current_password.as_bytes(), &parsed)
        .map_err(|_| AppError::Unauthorized("that is not the current password".to_string()))?;

    let name = name.map(str::trim).filter(|value| !value.is_empty());
    let email = email.map(|value| value.trim().to_lowercase());

    if let Some(email) = &email {
        if !email.contains('@') || email.len() < 3 {
            return Err(AppError::Validation(
                "that does not look like an email address".to_string(),
            ));
        }
        if let Some((other, _, _)) = find_operator_by_email(db, email).await?
            && other.id != operator_id
        {
            return Err(AppError::Conflict(
                "an account already uses that email address".to_string(),
            ));
        }
    }

    if let Some(password) = new_password
        && password.chars().count() < 10
    {
        return Err(AppError::Validation(
            "the password must be at least 10 characters".to_string(),
        ));
    }

    let updated = Operator {
        name: name.map(str::to_string).unwrap_or(operator.name),
        email: email.unwrap_or(operator.email),
        ..operator
    };

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "UPDATE operators SET name = {}, email = {}, password_hash = {} WHERE id = {}",
            parameter(backend, 1),
            parameter(backend, 2),
            parameter(backend, 3),
            parameter(backend, 4)
        ),
        [
            updated.name.clone().into(),
            updated.email.clone().into(),
            match new_password {
                Some(password) => hash_password(password)?,
                None => hash,
            }
            .into(),
            operator_id.to_string().into(),
        ],
    ))
    .await?;

    Ok(updated)
}

/// Every agency on the server, for whoever runs it.
pub async fn organizations(db: &DatabaseConnection) -> AppResult<Vec<(Organization, String)>> {
    let rows = db
        .query_all_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT o.id, o.name, o.site_limit, o.active, \
             COALESCE(MIN(op.email), '') AS email \
             FROM organizations o LEFT JOIN operators op ON op.organization_id = o.id \
             GROUP BY o.id, o.name, o.site_limit, o.active ORDER BY o.name",
        ))
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok((
                Organization {
                    id: Uuid::parse_str(&row.try_get::<String>("", "id")?)
                        .map_err(|err| AppError::Internal(format!("bad organization id: {err}")))?,
                    name: row.try_get("", "name")?,
                    site_limit: row.try_get("", "site_limit")?,
                    active: row.try_get::<i32>("", "active")? != 0,
                },
                row.try_get("", "email")?,
            ))
        })
        .collect()
}

/// How many sites an agency may have, and whether it may sign in at all.
pub async fn set_organization(
    db: &DatabaseConnection,
    id: Uuid,
    site_limit: Option<i32>,
    active: Option<bool>,
) -> AppResult<Organization> {
    let backend = db.get_database_backend();
    let current = organization(db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("agency".to_string()))?;

    let updated = Organization {
        site_limit: site_limit.unwrap_or(current.site_limit).max(0),
        active: active.unwrap_or(current.active),
        ..current
    };

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "UPDATE organizations SET site_limit = {}, active = {} WHERE id = {}",
            parameter(backend, 1),
            parameter(backend, 2),
            parameter(backend, 3)
        ),
        [
            updated.site_limit.into(),
            i32::from(updated.active).into(),
            id.to_string().into(),
        ],
    ))
    .await?;

    Ok(updated)
}
