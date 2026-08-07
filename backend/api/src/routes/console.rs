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
    /// From the link the server's operator sent. There is no way in without
    /// one.
    pub invite: String,
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
    /// Whether a token is kept here for all of this agency's sites to build
    /// with. Never the token itself.
    pub has_build_token: bool,
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
    // Answers without a credential.
    security(()),
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
    headers: axum::http::HeaderMap,
    cookies: Cookies,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<AccountResponse>)> {
    let db = control(&hosting, &resolved)?;

    // Counted by address alone: there is no account here yet to count against,
    // and a wrong invitation is a guess like any other.
    let key = format!("invite:from:{}", crate::throttle::caller(&headers));
    if let Some(left) = crate::throttle::pause_left(&key) {
        return Err(AppError::TooManyRequests(left.as_secs().max(1)));
    }

    let (organization, operator) = match console::register(
        db,
        &payload.organization_name,
        &payload.name,
        &payload.email,
        &payload.password,
        &payload.invite,
    )
    .await
    {
        Ok(made) => {
            crate::throttle::right(&key);
            made
        }
        Err(err) => {
            crate::throttle::wrong(&key);
            return Err(err);
        }
    };

    set_cookie(&cookies, console::create_session(db, operator.id).await?);

    Ok((
        StatusCode::CREATED,
        Json(AccountResponse {
            name: operator.name,
            email: operator.email,
            organization_name: organization.name,
            site_limit: organization.site_limit,
            has_build_token: organization.has_build_token,
        }),
    ))
}

/// Sign in to the console.
#[utoipa::path(
    // Answers without a credential.
    security(()),
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
    headers: axum::http::HeaderMap,
    cookies: Cookies,
    Json(payload): Json<SignInRequest>,
) -> AppResult<Json<AccountResponse>> {
    let db = control(&hosting, &resolved)?;

    let keys = [
        format!("console:who:{}", payload.email.trim().to_lowercase()),
        format!("console:from:{}", crate::throttle::caller(&headers)),
    ];
    if let Some(left) = keys.iter().find_map(|key| crate::throttle::pause_left(key)) {
        return Err(AppError::TooManyRequests(left.as_secs().max(1)));
    }

    let operator = match console::authenticate(db, &payload.email, &payload.password).await {
        Ok(operator) => {
            keys.iter().for_each(|key| crate::throttle::right(key));
            operator
        }
        Err(err) => {
            keys.iter().for_each(|key| {
                crate::throttle::wrong(key);
            });
            return Err(err);
        }
    };
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
        has_build_token: organization.has_build_token,
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
        has_build_token: organization.has_build_token,
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
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let tenant = console::add_site(&hosting, &operator, &payload.host).await?;

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

/// A token an agency gives to an assistant, so it can be asked about the
/// sites rather than opened fifty times.
#[utoipa::path(
    get,
    path = "/console/tokens",
    tag = "console",
    responses((status = 200, description = "Tokens", body = Vec<crate::dto::plugins::DevelopmentToken>))
)]
pub async fn list_tokens(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
) -> AppResult<Json<Vec<crate::dto::plugins::DevelopmentToken>>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;

    Ok(Json(
        console::tokens(db, operator.id)
            .await?
            .into_iter()
            .map(
                |(id, created_at, expires_at)| crate::dto::plugins::DevelopmentToken {
                    id,
                    created_at,
                    expires_at,
                },
            )
            .collect(),
    ))
}

/// Make one. It is shown once and kept nowhere it can be read again.
#[utoipa::path(
    post,
    path = "/console/tokens",
    tag = "console",
    responses((status = 201, description = "The token, the once", body = crate::dto::plugins::NewDevelopmentToken))
)]
pub async fn create_token(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
) -> AppResult<(StatusCode, Json<crate::dto::plugins::NewDevelopmentToken>)> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;
    let (token, expires_at) = console::mint_token(db, operator.id).await?;

    Ok((
        StatusCode::CREATED,
        Json(crate::dto::plugins::NewDevelopmentToken {
            token: token.to_string(),
            expires_at,
        }),
    ))
}

/// Take one back.
#[utoipa::path(
    delete,
    path = "/console/tokens/{id}",
    tag = "console",
    params(("id" = String, Path, description = "Token id")),
    responses((status = 204, description = "Revoked"))
)]
pub async fn delete_token(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;

    if console::revoke_token(db, operator.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("token {id}")))
    }
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
    let tenant = owned_site(&hosting, &operator, id).await?;
    let token = console::create_entry(db, tenant.id, operator.id).await?;

    Ok(Json(EntryResponse {
        url: format!("https://{}/admin/enter?token={}", tenant.host, token),
    }))
}

/// One of this agency's sites, or nothing — whether a site id exists at all
/// is not an agency's business either.
async fn owned_site(
    hosting: &Hosting,
    operator: &Operator,
    id: Uuid,
) -> AppResult<crate::tenants::Tenant> {
    hosting
        .registry()?
        .all()
        .await?
        .into_iter()
        .find(|tenant| tenant.id == id && tenant.organization_id == Some(operator.organization_id))
        .ok_or_else(|| AppError::NotFound("site".to_string()))
}

/// How one of this agency's sites is built.
///
/// This lives here rather than in the site's own panel because it belongs to
/// whoever built the site, not to whoever writes it: a repository address and
/// a build command are the agency's work, and the person writing posts should
/// not have to understand them to publish — or be able to break publishing by
/// changing them.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/publish",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Build settings and history", body = crate::routes::publish::PublishStatus))
)]
pub async fn get_site_publish(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::routes::publish::PublishStatus>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;
    let tenant = owned_site(&hosting, &operator, id).await?;

    Ok(Json(crate::routes::publish::status_of(db, &tenant).await?))
}

/// Say where one of this agency's sites is built from.
#[utoipa::path(
    put,
    path = "/console/sites/{id}/publish",
    tag = "console",
    request_body = crate::publish::SaveBuildConfig,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Saved", body = crate::publish::BuildConfig))
)]
pub async fn save_site_publish(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::publish::SaveBuildConfig>,
) -> AppResult<Json<crate::publish::BuildConfig>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;
    let tenant = owned_site(&hosting, &operator, id).await?;

    Ok(Json(
        crate::publish::save_config(db, hosting.secrets(), tenant.id, payload).await?,
    ))
}

/// Publish one of this agency's sites.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/publish",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 202, description = "A build is queued", body = crate::publish::Build))
)]
pub async fn request_site_publish(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<crate::publish::Build>)> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;
    let tenant = owned_site(&hosting, &operator, id).await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(crate::publish::request(db, tenant.id).await?),
    ))
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
/// One per agency per site, made on first arrival. It carries no usable
/// password: the only way in is through a fresh entry token, so revoking the
/// agency's console account closes every site with it rather than leaving a
/// working login behind on fifty of them.
///
/// Found by a name built from the agency account's id, and by nothing else.
///
/// The id rather than the email, because an agency that changes its email is
/// the same agency — going by email left the old account behind on every site
/// it had ever opened and made a second one beside it.
///
/// And *only* the id: matching on the email as well would mean an agency whose
/// address happens to match somebody already writing on the site takes that
/// person's account over — renames it, and signs in as them. An agency and one
/// of its customers' editors sharing an address is not unusual; the same
/// person often is both.
///
/// The whole id, not a prefix of it: two agencies sharing eight characters
/// would share an account, and there is no reason to leave that reasoning to
/// anybody.
async fn agency_user(db: &sea_orm::DatabaseConnection, operator: &Operator) -> AppResult<Uuid> {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

    use crate::entities::user;

    let username = format!("agency-{}", operator.id);

    let existing = user::Entity::find()
        .filter(user::Column::Username.eq(username.clone()))
        .one(db)
        .await?;

    if let Some(found) = existing {
        // The email follows the console account, so a change there shows here
        // too — but only onto a row this agency already owns, and only when
        // nobody on this site is already using that address. A site's emails
        // are unique, and this is a label on an account whose identity is the
        // name above; failing the sign-in over it would be trading the whole
        // feature for a cosmetic detail.
        let taken = user::Entity::find()
            .filter(user::Column::Email.eq(operator.email.clone()))
            .one(db)
            .await?
            .is_some_and(|other| other.id != found.id);

        if found.email != operator.email && !taken {
            let mut changed: user::ActiveModel = found.clone().into();
            changed.email = Set(operator.email.clone());
            changed.update(db).await?;
        }
        return Ok(found.id);
    }

    // Same reason as above, on the way in: an agency whose address is already
    // somebody's here gets one that cannot be anybody's, rather than a failed
    // sign-in.
    let free = user::Entity::find()
        .filter(user::Column::Email.eq(operator.email.clone()))
        .one(db)
        .await?
        .is_none();

    let id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(id),
        username: Set(username.clone()),
        email: Set(if free {
            operator.email.clone()
        } else {
            format!("{username}@localhost")
        }),
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountRequest {
    /// Asked for even though the session already proves who this is: a session
    /// left open somewhere should not be enough to take the account away.
    pub current_password: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub new_password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BuildTokenRequest {
    /// Empty takes the agency's token away, and its sites fall back to
    /// whatever each of them was given.
    pub token: String,
}

/// Keep one access token for every site this agency builds.
///
/// The agency's own, not a site's and not another agency's: there is no id in
/// the path, so the only organization this can be pointed at is the one whose
/// account made the request.
#[utoipa::path(
    put,
    path = "/console/organization/build-token",
    tag = "console",
    request_body = BuildTokenRequest,
    responses(
        (status = 200, description = "Kept, or taken away", body = AccountResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    )
)]
pub async fn save_organization_build_token(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Json(payload): Json<BuildTokenRequest>,
) -> AppResult<Json<AccountResponse>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;

    console::set_build_token(
        db,
        hosting.secrets(),
        operator.organization_id,
        &payload.token,
    )
    .await?;

    let organization = console::organization(db, operator.organization_id)
        .await?
        .ok_or_else(|| AppError::Internal("the agency behind this account is gone".to_string()))?;

    Ok(Json(AccountResponse {
        name: operator.name,
        email: operator.email,
        organization_name: organization.name,
        site_limit: organization.site_limit,
        has_build_token: organization.has_build_token,
    }))
}

/// Change this agency account's own name, address or password.
#[utoipa::path(
    put,
    path = "/console/me",
    tag = "console",
    request_body = UpdateAccountRequest,
    responses(
        (status = 200, description = "Changed", body = AccountResponse),
        (status = 401, description = "That is not the current password", body = crate::error::ErrorBody),
    )
)]
pub async fn update_account(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Json(payload): Json<UpdateAccountRequest>,
) -> AppResult<Json<AccountResponse>> {
    let (operator, db) = signed_in(&hosting, &resolved, &cookies).await?;

    let updated = console::update_account(
        db,
        operator.id,
        &payload.current_password,
        payload.name.as_deref(),
        payload.email.as_deref(),
        payload.new_password.as_deref(),
    )
    .await?;

    let organization = console::organization(db, updated.organization_id)
        .await?
        .ok_or_else(|| AppError::Internal("the agency behind this account is gone".to_string()))?;

    Ok(Json(AccountResponse {
        name: updated.name,
        email: updated.email,
        organization_name: organization.name,
        site_limit: organization.site_limit,
        has_build_token: organization.has_build_token,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgencyResponse {
    pub id: String,
    pub name: String,
    pub email: String,
    pub site_limit: i32,
    pub active: bool,
    /// How many sites it has now, against the limit above.
    pub sites: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAgencyRequest {
    pub site_limit: Option<i32>,
    /// Switching an agency off leaves its sites serving and stops it signing
    /// in — the customers are not the ones who fell out with anybody.
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInviteRequest {
    /// How many sites the agency may open. The usual number if left out.
    pub site_limit: Option<i32>,
    /// What the operator wrote it for — a name, a company, whatever helps
    /// later. Never shown to whoever follows the link.
    pub note: Option<String>,
    /// How long the link stands, in days.
    pub days: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InviteResponse {
    /// Goes on the end of the console's register address.
    pub token: String,
    pub site_limit: i32,
    pub note: String,
    pub created_at: String,
    pub expires_at: String,
    pub used: bool,
    pub organization: Option<String>,
}

/// Invites an agency onto this server.
#[utoipa::path(
    post,
    path = "/invites",
    tag = "sites",
    request_body = CreateInviteRequest,
    responses((status = 201, description = "The invitation", body = InviteResponse))
)]
pub async fn create_invite(
    _operator: crate::tenants::Operator,
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    Json(payload): Json<CreateInviteRequest>,
) -> AppResult<(StatusCode, Json<InviteResponse>)> {
    let db = control(&hosting, &resolved)?;
    let invite = console::create_invite(
        db,
        payload.site_limit,
        payload.note.as_deref().unwrap_or_default(),
        payload.days,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(shown(invite, &[]))))
}

/// Every invitation this server has given out.
#[utoipa::path(
    get,
    path = "/invites",
    tag = "sites",
    responses((status = 200, description = "The invitations", body = Vec<InviteResponse>))
)]
pub async fn list_invites(
    _operator: crate::tenants::Operator,
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
) -> AppResult<Json<Vec<InviteResponse>>> {
    let db = control(&hosting, &resolved)?;
    let agencies: Vec<console::Organization> = console::organizations(db)
        .await?
        .into_iter()
        .map(|(organization, _)| organization)
        .collect();
    let invites = console::list_invites(db).await?;

    Ok(Json(
        invites
            .into_iter()
            .map(|invite| shown(invite, &agencies))
            .collect(),
    ))
}

/// Takes back an invitation nobody has used yet.
#[utoipa::path(
    delete,
    path = "/invites/{token}",
    tag = "sites",
    params(("token" = String, Path, description = "The invitation")),
    responses((status = 204, description = "Taken back"))
)]
pub async fn revoke_invite(
    _operator: crate::tenants::Operator,
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    Path(token): Path<String>,
) -> AppResult<StatusCode> {
    let db = control(&hosting, &resolved)?;
    console::revoke_invite(db, &token).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// An invitation with the agency that took it named rather than numbered.
fn shown(invite: console::Invite, agencies: &[console::Organization]) -> InviteResponse {
    InviteResponse {
        organization: invite.organization_id.and_then(|id| {
            agencies
                .iter()
                .find(|agency| agency.id == id)
                .map(|agency| agency.name.clone())
        }),
        token: invite.token,
        site_limit: invite.site_limit,
        note: invite.note,
        created_at: invite.created_at.to_rfc3339(),
        expires_at: invite.expires_at.to_rfc3339(),
        used: invite.used,
    }
}

/// Whether this server takes registrations without an invitation.
///
/// Asked before the form is drawn: a page that collects a name, an address and
/// a password and then says "by invitation only" wasted somebody's time.
#[utoipa::path(
    get,
    path = "/console/registration",
    tag = "console",
    responses((status = 200, description = "Whether anybody may sign up", body = RegistrationResponse))
)]
pub async fn registration() -> Json<RegistrationResponse> {
    Json(RegistrationResponse {
        open: console::registration_is_open(),
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegistrationResponse {
    pub open: bool,
}

/// Every agency on this server. The operator's view.
#[utoipa::path(
    get,
    path = "/agencies",
    tag = "sites",
    responses((status = 200, description = "The agencies", body = Vec<AgencyResponse>))
)]
pub async fn list_agencies(
    _operator: crate::tenants::Operator,
    State(hosting): State<Hosting>,
) -> AppResult<Json<Vec<AgencyResponse>>> {
    let registry = hosting.registry()?;
    let sites = registry.all().await?;

    Ok(Json(
        console::organizations(registry.control())
            .await?
            .into_iter()
            .map(|(organization, email)| AgencyResponse {
                sites: sites
                    .iter()
                    .filter(|site| site.organization_id == Some(organization.id))
                    .count() as u32,
                id: organization.id.to_string(),
                name: organization.name,
                email,
                site_limit: organization.site_limit,
                active: organization.active,
            })
            .collect(),
    ))
}

/// How many sites an agency may have, and whether it may sign in.
#[utoipa::path(
    put,
    path = "/agencies/{id}",
    tag = "sites",
    params(("id" = String, Path, description = "Agency id")),
    request_body = UpdateAgencyRequest,
    responses((status = 204, description = "Changed"))
)]
pub async fn update_agency(
    _operator: crate::tenants::Operator,
    State(hosting): State<Hosting>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateAgencyRequest>,
) -> AppResult<StatusCode> {
    console::set_organization(
        hosting.registry()?.control(),
        id,
        payload.site_limit,
        payload.active,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// The state of one of this agency's sites, which is what the plugin
/// functions work on. Opening it is the same thing a request to that site
/// would do — the console is simply reaching it from the other side.
async fn owned_state(
    hosting: &Hosting,
    operator: &Operator,
    id: Uuid,
) -> AppResult<crate::state::AppState> {
    let tenant = owned_site(hosting, operator, id).await?;
    hosting.registry()?.state_for(&tenant).await
}

/// One of this agency's sites' storage settings.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/s3",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "S3 settings", body = crate::dto::plugins::S3SettingsResponse))
)]
pub async fn get_site_s3(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::dto::plugins::S3SettingsResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(crate::routes::plugins::s3_settings_of(&state).await?))
}

/// Set them.
#[utoipa::path(
    put,
    path = "/console/sites/{id}/plugins/s3",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    request_body = crate::dto::plugins::S3SettingsRequest,
    responses((status = 200, description = "Saved", body = crate::dto::plugins::S3SettingsResponse))
)]
pub async fn save_site_s3(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::dto::plugins::S3SettingsRequest>,
) -> AppResult<Json<crate::dto::plugins::S3SettingsResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::save_s3_of(&state, payload).await?,
    ))
}

/// One of this agency's sites' backup settings and archives.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/backup",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Backup settings", body = crate::dto::plugins::BackupSettingsResponse))
)]
pub async fn get_site_backup(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::dto::plugins::BackupSettingsResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::backup_settings_of(&state).await?,
    ))
}

/// Set them.
#[utoipa::path(
    put,
    path = "/console/sites/{id}/plugins/backup",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    request_body = crate::dto::plugins::UpdateBackupRequest,
    responses((status = 200, description = "Saved", body = crate::dto::plugins::BackupSettingsResponse))
)]
pub async fn save_site_backup(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::dto::plugins::UpdateBackupRequest>,
) -> AppResult<Json<crate::dto::plugins::BackupSettingsResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::save_backup_of(&state, payload).await?,
    ))
}

/// Take a backup of one of this agency's sites now.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/backup/run",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Backup written", body = crate::backup::BackupFile))
)]
pub async fn run_site_backup(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::backup::BackupFile>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(crate::backup::run(&state).await?))
}

/// Put one of this agency's sites back from one of its archives.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/backup/{name}/restore",
    tag = "console",
    params(
        ("id" = String, Path, description = "Site id"),
        ("name" = String, Path, description = "Archive name"),
    ),
    responses((status = 200, description = "What was put back", body = crate::backup::RestoreReport))
)]
pub async fn restore_site_backup(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<crate::backup::RestoreReport>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    let (_, config) = crate::backup::config(&state).await?;
    let bytes = crate::backup::read(&state, &config, &name).await?;

    tracing::warn!(by = %operator.email, site = %id, archive = %name, "restoring a backup");

    Ok(Json(crate::backup::restore(&state, &bytes).await?))
}

/// The account local development reads as.
///
/// Its own, not the one a build uses: revoking what a designer's laptop holds
/// should not stop the site publishing, and stopping a build should not lock
/// somebody out of their editor. Passwordless, like the others here — the only
/// way to be it is a token this handed out.
pub(crate) async fn local_reader(db: &sea_orm::DatabaseConnection) -> AppResult<Uuid> {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

    use crate::entities::user;

    if let Some(found) = user::Entity::find()
        .filter(user::Column::Username.eq(LOCAL_READER))
        .one(db)
        .await?
    {
        return Ok(found.id);
    }

    let created = user::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(LOCAL_READER.to_string()),
        email: Set(format!("{LOCAL_READER}@localhost")),
        password_hash: Set(String::new()),
        role: Set(crate::auth::BUILDER.to_string()),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(created.id)
}

/// The username of that account. Also listed in `routes::users` as one a site
/// may not remove.
pub const LOCAL_READER: &str = "local";

/// How long a development token lasts.
///
/// Long enough to not be a weekly errand, short enough that a laptop that
/// leaves the agency stops working by itself.
pub(crate) const DEVELOPMENT_TOKEN_DAYS: i64 = 30;

pub(crate) async fn development_tokens(
    db: &sea_orm::DatabaseConnection,
) -> AppResult<Vec<crate::dto::plugins::DevelopmentToken>> {
    use sea_orm::{ColumnTrait, EntityTrait, Order, QueryFilter, QueryOrder};

    use crate::entities::session;

    let reader = local_reader(db).await?;
    Ok(session::Entity::find()
        .filter(session::Column::UserId.eq(reader))
        .order_by(session::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?
        .into_iter()
        .map(|row| crate::dto::plugins::DevelopmentToken {
            id: row.id.to_string(),
            created_at: row.created_at.to_rfc3339(),
            expires_at: row.expires_at.to_rfc3339(),
        })
        .collect())
}

/// What to put in a `.env` to work on one of this agency's sites locally.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/development",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Local settings", body = crate::dto::plugins::DevelopmentResponse))
)]
pub async fn get_site_development(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::dto::plugins::DevelopmentResponse>> {
    let (operator, control) = signed_in(&hosting, &resolved, &cookies).await?;
    let tenant = owned_site(&hosting, &operator, id).await?;
    let state = hosting.registry()?.state_for(&tenant).await?;
    let site_db = state.db_or_unavailable()?;

    let variables = crate::publish::config(control, tenant.id)
        .await?
        .map(|config| config.environment_keys)
        .unwrap_or_default();

    Ok(Json(crate::dto::plugins::DevelopmentResponse {
        api_url: format!("https://{}/api", tenant.host),
        site_url: format!("https://{}", tenant.host),
        variables,
        tokens: development_tokens(site_db).await?,
    }))
}

/// Hand out a token for somebody's own machine.
///
/// Read-only, and it is shown once. Nothing else here needs to leave the
/// server: an address is not a secret, and the alternative — an agency
/// sending a password over chat because a designer could not fetch any posts
/// — is how one password ends up on four laptops and in a group thread.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/development/tokens",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 201, description = "The token, once", body = crate::dto::plugins::NewDevelopmentToken))
)]
pub async fn create_site_development_token(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<(StatusCode, Json<crate::dto::plugins::NewDevelopmentToken>)> {
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};

    use crate::entities::session;

    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;
    let db = state.db_or_unavailable()?;

    let reader = local_reader(db).await?;
    let token = Uuid::now_v7();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(DEVELOPMENT_TOKEN_DAYS);

    session::ActiveModel {
        id: Set(token),
        user_id: Set(reader),
        expires_at: Set(expires_at.fixed_offset()),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(crate::dto::plugins::NewDevelopmentToken {
            token: token.to_string(),
            expires_at: expires_at.to_rfc3339(),
        }),
    ))
}

/// Take one back.
#[utoipa::path(
    delete,
    path = "/console/sites/{id}/development/tokens/{token_id}",
    tag = "console",
    params(
        ("id" = String, Path, description = "Site id"),
        ("token_id" = String, Path, description = "Token id"),
    ),
    responses((status = 204, description = "Revoked"))
)]
pub async fn delete_site_development_token(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path((id, token_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    use crate::entities::session;

    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;
    let db = state.db_or_unavailable()?;

    let reader = local_reader(db).await?;
    let result = session::Entity::delete_many()
        .filter(session::Column::Id.eq(token_id))
        // Only this account's. A session id from the panel is somebody's
        // login, and revoking one from here would be signing them out.
        .filter(session::Column::UserId.eq(reader))
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("token {token_id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// One of this agency's sites' mail settings.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Mail settings", body = crate::email::EmailSettingsResponse))
)]
pub async fn get_site_email(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::email::EmailSettingsResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::email_settings_of(&state).await?,
    ))
}

/// Set them.
#[utoipa::path(
    put,
    path = "/console/sites/{id}/plugins/email",
    tag = "console",
    request_body = crate::email::EmailSettingsRequest,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Saved", body = crate::email::EmailSettingsResponse))
)]
pub async fn save_site_email(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::email::EmailSettingsRequest>,
) -> AppResult<Json<crate::email::EmailSettingsResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::save_email_of(&state, payload).await?,
    ))
}

/// Send a test message from one of this agency's sites.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/test",
    tag = "console",
    request_body = crate::email::TestEmailRequest,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Test result", body = crate::dto::plugins::ConnectionTestResponse))
)]
pub async fn test_site_email(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::email::TestEmailRequest>,
) -> AppResult<Json<crate::dto::plugins::ConnectionTestResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::test_email_of(&state, payload).await?,
    ))
}

/// What Amazon says about one of this agency's sites' SES account.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/account",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Quota, sandbox and enforcement", body = crate::email::AccountStatus))
)]
pub async fn get_site_email_account(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::email::AccountStatus>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::email_account_of(&state).await?,
    ))
}

/// Ask Amazon to take it out of the sandbox.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/production-access",
    tag = "console",
    request_body = crate::email::ProductionAccessRequest,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 204, description = "Asked"))
)]
pub async fn request_site_production_access(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::email::ProductionAccessRequest>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::request_production_access_of(&state, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The senders SES will send as.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/identities",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Verified senders", body = Vec<crate::email::Identity>))
)]
pub async fn list_site_email_identities(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<crate::email::Identity>>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::email_identities_of(&state).await?,
    ))
}

/// Ask SES to trust one.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/identities",
    tag = "console",
    request_body = crate::routes::plugins::IdentityRequest,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 204, description = "Asked"))
)]
pub async fn add_site_email_identity(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::routes::plugins::IdentityRequest>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::add_email_identity_of(&state, &payload.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stop trusting one.
#[utoipa::path(
    delete,
    path = "/console/sites/{id}/plugins/email/identities/{name}",
    tag = "console",
    params(
        ("id" = String, Path, description = "Site id"),
        ("name" = String, Path, description = "The address or domain"),
    ),
    responses((status = 204, description = "Removed"))
)]
pub async fn delete_site_email_identity(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::remove_email_identity_of(&state, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The addresses SES has stopped writing to.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/suppressed",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Blocked addresses", body = Vec<crate::email::Suppressed>))
)]
pub async fn list_site_email_suppressed(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<crate::email::Suppressed>>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::email_suppressed_of(&state).await?,
    ))
}

/// Take one off that list.
#[utoipa::path(
    delete,
    path = "/console/sites/{id}/plugins/email/suppressed/{address}",
    tag = "console",
    params(
        ("id" = String, Path, description = "Site id"),
        ("address" = String, Path, description = "The address"),
    ),
    responses((status = 204, description = "Unblocked"))
)]
pub async fn delete_site_email_suppressed(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path((id, address)): Path<(Uuid, String)>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::unsuppress_email_of(&state, &address).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/health",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "For that site", body = crate::email::SendingHealth))
)]
pub async fn get_site_email_health(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::email::SendingHealth>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(crate::routes::plugins::email_health_of(&state).await?))
}

#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/configuration-sets",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "For that site", body = Vec<String>))
)]
pub async fn list_site_email_configuration_sets(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<String>>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::email_configuration_sets_of(&state).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/requests",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "For that site", body = Vec<crate::email::SupportCase>))
)]
pub async fn list_site_email_requests(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<crate::email::SupportCase>>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::email_requests_of(&state).await?,
    ))
}

/// Stop or resume everything one of this agency's sites sends.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/sending",
    tag = "console",
    request_body = crate::routes::plugins::SendingSwitch,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 204, description = "Changed"))
)]
pub async fn set_site_email_sending(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::routes::plugins::SendingSwitch>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::set_email_sending_of(&state, payload.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ask Amazon for a bigger quota for one of this agency's sites.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/quota-increase",
    tag = "console",
    request_body = crate::email::QuotaIncreaseRequest,
    params(("id" = String, Path, description = "Site id")),
    responses((status = 201, description = "Asked", body = crate::email::SupportCase))
)]
pub async fn request_site_quota_increase(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::email::QuotaIncreaseRequest>,
) -> AppResult<(StatusCode, Json<crate::email::SupportCase>)> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok((
        StatusCode::CREATED,
        Json(crate::routes::plugins::request_quota_increase_of(&state, payload).await?),
    ))
}

/// Set the bounce subdomain for one of this agency's sites.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/identities/{name}/mail-from",
    tag = "console",
    params(
        ("id" = String, Path, description = "Site id"),
        ("name" = String, Path, description = "The domain"),
    ),
    request_body = crate::routes::plugins::MailFromRequest,
    responses((status = 204, description = "Set"))
)]
pub async fn set_site_email_mail_from(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path((id, name)): Path<(Uuid, String)>,
    Json(payload): Json<crate::routes::plugins::MailFromRequest>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::set_mail_from_of(&state, &name, &payload.subdomain).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Make a configuration set for one of this agency's sites.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/configuration-sets",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    request_body = crate::routes::plugins::IdentityRequest,
    responses((status = 204, description = "Made"))
)]
pub async fn create_site_email_configuration_set(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
    Json(payload): Json<crate::routes::plugins::IdentityRequest>,
) -> AppResult<StatusCode> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    crate::routes::plugins::create_configuration_set_of(&state, &payload.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Build one of this agency's sites' event pipeline.
#[utoipa::path(
    post,
    path = "/console/sites/{id}/plugins/email/events/setup",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "What was built", body = crate::routes::plugins::PipelineResponse))
)]
pub async fn setup_site_email_events(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::routes::plugins::PipelineResponse>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let tenant = owned_site(&hosting, &operator, id).await?;
    let state = hosting.registry()?.state_for(&tenant).await?;

    Ok(Json(
        crate::routes::plugins::setup_events_of(&state, &tenant.host).await?,
    ))
}

/// Amazon's own figures for one of this agency's sites.
#[utoipa::path(
    get,
    path = "/console/sites/{id}/plugins/email/deliverability",
    tag = "console",
    params(("id" = String, Path, description = "Site id")),
    responses((status = 200, description = "Deliverability", body = crate::email::Deliverability))
)]
pub async fn get_site_email_deliverability(
    State(hosting): State<Hosting>,
    axum::Extension(resolved): axum::Extension<Resolved>,
    cookies: Cookies,
    Path(id): Path<Uuid>,
) -> AppResult<Json<crate::email::Deliverability>> {
    let (operator, _) = signed_in(&hosting, &resolved, &cookies).await?;
    let state = owned_state(&hosting, &operator, id).await?;

    Ok(Json(
        crate::routes::plugins::deliverability_of(&state, 30).await?,
    ))
}
