use axum::{Json, extract::Path, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use tower_cookies::{Cookie, Cookies, cookie::SameSite};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    console::{self, CONSOLE_COOKIE, Operator},
    error::{AppError, AppResult},
    tenants::{Hosting, Resolved, Site},
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// What the agency is called.
    pub organization_name: String,
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SignInRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountResponse {
    pub name: String,
    pub email: String,
    pub organization_name: String,
    /// How many sites this agency may have in total.
    pub site_limit: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsoleSiteResponse {
    pub id: String,
    pub host: String,
    pub slug: String,
    pub active: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateConsoleSiteRequest {
    pub host: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntryResponse {
    /// Where to send the browser to arrive signed in.
    pub url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EntryRequest {
    pub token: String,
}

fn set_cookie(cookies: &Cookies, session_id: Uuid) {
    let mut cookie = Cookie::new(CONSOLE_COOKIE, session_id.to_string());
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookies.add(cookie);
}

/// The console is the server's own front door, not a page on somebody's site.
/// Reaching it through a hosted site's address would put an agency's account
/// behind a hostname that agency does not control.
fn control<'a>(
    hosting: &'a Hosting,
    resolved: &Resolved,
) -> AppResult<&'a sea_orm::DatabaseConnection> {
    if !resolved.is_host() {
        return Err(AppError::Forbidden(
            "agencies sign in on the server's own address".to_string(),
        ));
    }
    Ok(hosting.registry()?.control())
}

async fn signed_in<'a>(
    hosting: &'a Hosting,
    resolved: &Resolved,
    cookies: &Cookies,
) -> AppResult<(Operator, &'a sea_orm::DatabaseConnection)> {
    let db = control(hosting, resolved)?;
    let not_signed_in = || AppError::Unauthorized("not signed in".to_string());

    let id = cookies
        .get(CONSOLE_COOKIE)
        .and_then(|cookie| Uuid::parse_str(cookie.value()).ok())
        .ok_or_else(not_signed_in)?;

    let operator = console::session_operator(db, id)
        .await?
        .ok_or_else(not_signed_in)?;
    Ok((operator, db))
}

/// Open an agency account and sign in with it.
#[utoipa::path(
    post,
    path = "/console/register",
    tag = "console",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Agency created", body = AccountResponse),
        (status = 409, description = "That email is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn register(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<AccountResponse>)> {
    let db = control(&hosting, &resolved)?;

    let (organization, operator) = console::register(
        db,
        &payload.organization_name,
        &payload.name,
        &payload.email,
        &payload.password,
    )
    .await?;

    set_cookie(&cookies, console::create_session(db, operator.id).await?);

    Ok((
        StatusCode::CREATED,
        Json(AccountResponse {
            name: operator.name,
            email: operator.email,
            organization_name: organization.name,
            site_limit: organization.site_limit,
        }),
    ))
}

/// Sign in to the console.
#[utoipa::path(
    post,
    path = "/console/login",
    tag = "console",
    request_body = SignInRequest,
    responses(
        (status = 200, description = "Signed in", body = AccountResponse),
        (status = 401, description = "Wrong email or password", body = crate::error::ErrorBody),
    )
)]
pub async fn login(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Json(payload): Json<SignInRequest>,
) -> AppResult<Json<AccountResponse>> {
    let db = control(&hosting, &resolved)?;
    let operator = console::authenticate(db, &payload.email, &payload.password).await?;
    let organization = console::organization(db, operator.organization_id)
        .await?
        .ok_or_else(|| AppError::Internal("the agency behind this account is gone".to_string()))?;

    if !organization.active {
        return Err(AppError::Forbidden(
            "this agency has been switched off".to_string(),
        ));
    }

    set_cookie(&cookies, console::create_session(db, operator.id).await?);

    Ok(Json(AccountResponse {
        name: operator.name,
        email: operator.email,
        organization_name: organization.name,
        site_limit: organization.site_limit,
    }))
}

/// Sign out of the console. Always succeeds.
#[utoipa::path(
    post,
    path = "/console/logout",
    tag = "console",
    responses((status = 204, description = "Signed out"))
)]
pub async fn logout(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
) -> StatusCode {
    if let Ok(db) = control(&hosting, &resolved)
        && let Some(id) = cookies
            .get(CONSOLE_COOKIE)
            .and_then(|cookie| Uuid::parse_str(cookie.value()).ok())
    {
        console::delete_session(db, id).await;
    }

    let mut removal = Cookie::new(CONSOLE_COOKIE, "");
    removal.set_path("/");
    cookies.remove(removal);
    StatusCode::NO_CONTENT
}

/// Who is signed in to the console.
#[utoipa::path(
    get,
    path = "/console/me",
    tag = "console",
    responses(
        (status = 200, description = "The signed-in agency", body = AccountResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    )
)]
pub async fn me(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
) -> AppResult<Json<AccountResponse>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;
    let organization = console::organization(db, operator.organization_id)
        .await?
        .ok_or_else(|| AppError::Internal("the agency behind this account is gone".to_string()))?;

    Ok(Json(AccountResponse {
        name: operator.name,
        email: operator.email,
        organization_name: organization.name,
        site_limit: organization.site_limit,
    }))
}

/// The agency's own sites, and nobody else's.
#[utoipa::path(
    get,
    path = "/console/sites",
    tag = "console",
    responses((status = 200, description = "The agency's sites", body = Vec<ConsoleSiteResponse>))
)]
pub async fn list_sites(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
) -> AppResult<Json<Vec<ConsoleSiteResponse>>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;

    Ok(Json(
        hosting
            .registry()?
            .all()
            .await?
            .into_iter()
            .filter(|tenant| tenant.organization_id == Some(operator.organization_id))
            .map(|tenant| ConsoleSiteResponse {
                id: tenant.id.to_string(),
                host: tenant.host,
                slug: tenant.slug,
                active: tenant.active,
            })
            .collect(),
    ))
}

/// Open a site under this agency.
#[utoipa::path(
    post,
    path = "/console/sites",
    tag = "console",
    request_body = CreateConsoleSiteRequest,
    responses(
        (status = 201, description = "Site created", body = ConsoleSiteResponse),
        (status = 403, description = "The agency is at its limit", body = crate::error::ErrorBody),
        (status = 409, description = "That address is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn create_site(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Json(payload): Json<CreateConsoleSiteRequest>,
) -> AppResult<(StatusCode, Json<ConsoleSiteResponse>)> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;
    let registry = hosting.registry()?;

    let organization = console::organization(db, operator.organization_id)
        .await?
        .ok_or_else(|| AppError::Internal("the agency behind this account is gone".to_string()))?;

    let owned = registry
        .all()
        .await?
        .into_iter()
        .filter(|tenant| tenant.organization_id == Some(operator.organization_id))
        .count();
    if owned >= organization.site_limit.max(0) as usize {
        return Err(AppError::Forbidden(format!(
            "{} already has its {} sites",
            organization.name, organization.site_limit
        )));
    }

    let slug = payload.host.replace('.', "-");
    let tenant = registry
        .create(&payload.host, &slug, "", Some(operator.organization_id))
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ConsoleSiteResponse {
            id: tenant.id.to_string(),
            host: tenant.host,
            slug: tenant.slug,
            active: tenant.active,
        }),
    ))
}

/// A link that opens one of the agency's sites already signed in.
///
/// The agency's own password is not what gets it into the site — this mints a
/// token good for one use and two minutes, which the site trades for a session
/// of its own. A link that leaked would already be spent, and it opens exactly
/// the one site it was made for.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/entry",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses(
        (status = 200, description = "A one-time sign-in link", body = EntryResponse),
        (status = 404, description = "Not one of this agency's sites", body = crate::error::ErrorBody),
    )
)]
pub async fn create_entry(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<EntryResponse>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;

    // Not found rather than forbidden: whether a site id exists is not this
    // agency's business either.
    let tenant = hosting
        .registry()?
        .all()
        .await?
        .into_iter()
        .find(|tenant| {
            tenant.id == id && tenant.organization_id == Some(operator.organization_id)
        })
        .ok_or_else(|| AppError::NotFound("site".to_string()))?;

    let token = console::create_entry(db, tenant.id, operator.id).await?;

    Ok(Json(EntryResponse {
        url: format!("https://{}/enter?token={}", tenant.host, token),
    }))
}

/// Trades an entry token for a session on this site.
///
/// This runs on the site's own address, which is what makes it work at all: a
/// cookie can only be set for the host the browser is talking to, so the
/// hand-off has to finish here rather than in the console.
#[utoipa::path(
    post,
    path = "/enter",
    tag = "console",
    request_body = EntryRequest,
    responses(
        (status = 204, description = "Signed in to this site"),
        (status = 401, description = "The link is spent or expired", body = crate::error::ErrorBody),
    )
)]
pub async fn enter(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    Site(state): Site,
    cookies: Cookies,
    Json(payload): Json<EntryRequest>,
) -> AppResult<StatusCode> {
    let Resolved::Tenant(tenant) = &resolved else {
        return Err(AppError::NotFound("site".to_string()));
    };
    let db = hosting.registry()?.control();

    let operator = console::claim_entry(db, &payload.token, tenant.id).await?;
    let site_db = state.db_or_unavailable()?;
    let user = agency_user(site_db, &operator).await?;
    crate::auth::create_session(site_db, &cookies, user).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// The account an agency writes as on one of its sites.
///
/// One per agency per site, found by email and made on first arrival. It
/// carries no usable password: the only way in is through a fresh entry
/// token, so revoking the agency's console account closes every site with it
/// rather than leaving a working login behind on fifty of them.
async fn agency_user(
    db: &sea_orm::DatabaseConnection,
    operator: &Operator,
) -> AppResult<Uuid> {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

    use crate::entities::user;

    if let Some(existing) = user::Entity::find()
        .filter(user::Column::Email.eq(operator.email.clone()))
        .one(db)
        .await?
    {
        return Ok(existing.id);
    }

    let id = Uuid::new_v4();
    user::ActiveModel {
        id: Set(id),
        username: Set(unique_username(db, &operator.email).await?),
        email: Set(operator.email.clone()),
        // Not a hash of anything: argon2 will not verify it, so no password
        // reaches this account.
        password_hash: Set(String::new()),
        role: Set("administrator".to_string()),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(id)
}

async fn unique_username(db: &sea_orm::DatabaseConnection, email: &str) -> AppResult<String> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    use crate::entities::user;

    let base = crate::slug::slugify(email.split('@').next().unwrap_or("agency"));
    let base = if base.is_empty() { "agency".to_string() } else { base };

    for attempt in 0..50 {
        let candidate = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        if user::Entity::find()
            .filter(user::Column::Username.eq(candidate.clone()))
            .one(db)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(AppError::Conflict(
        "could not find a free username for the agency".to_string(),
    ))
}
