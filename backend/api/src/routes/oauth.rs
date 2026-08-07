//! The addresses a client walks through to connect itself to a site.
//!
//! The screen a person actually sees is in the panel, not here: this validates
//! the request, sends the browser to that screen, and turns "yes" into a code.
//! Keeping the page in the panel means the consent screen looks like the rest
//! of the site somebody is being asked about, which is most of what makes it
//! possible to know what you are agreeing to.

use axum::{
    Form, Json,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::{
    auth::Administrator,
    error::{AppError, AppResult},
    oauth::{self, AuthorizeRequest},
    tenants::Site,
};

/// Where this site is, as the caller reached it. The same reasoning as
/// `llms.rs`: which site this is depends entirely on the address.
fn origin(headers: &HeaderMap) -> String {
    crate::routes::llms::origin(headers)
}

/// Who guards the MCP endpoint. A client that gets a 401 from it reads this
/// to find out where to send somebody to sign in.
#[utoipa::path(
    get,
    path = "/.well-known/oauth-protected-resource",
    tag = "oauth",
    responses((status = 200, description = "Resource metadata (RFC 9728)")),
    // Answers without a credential.
    security(())
)]
pub async fn protected_resource(headers: HeaderMap) -> Json<Value> {
    Json(oauth::protected_resource(&origin(&headers)))
}

/// How to ask this site for permission.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    get,
    path = "/.well-known/oauth-authorization-server",
    tag = "oauth",
    responses((status = 200, description = "Authorization server metadata (RFC 8414)"))
)]
pub async fn authorization_server(headers: HeaderMap) -> Json<Value> {
    Json(oauth::authorization_server(&origin(&headers)))
}

/// A client introducing itself.
///
/// Open, as the specification has it. Registering buys nothing on its own: it
/// records a name and the addresses a code may be sent to. Whether anything
/// happens is decided by the person who then has to sign in and agree.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    post,
    path = "/oauth/register",
    tag = "oauth",
    request_body = crate::oauth::Registration,
    responses((status = 201, description = "Registered", body = crate::oauth::Registered))
)]
pub async fn register(
    Site(state): Site,
    Json(payload): Json<oauth::Registration>,
) -> AppResult<(StatusCode, Json<oauth::Registered>)> {
    let registered = oauth::register(state.db_or_unavailable()?, payload).await?;
    Ok((StatusCode::CREATED, Json(registered)))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AuthorizeQuery {
    #[serde(default)]
    pub response_type: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub redirect_uri: String,
    #[serde(default)]
    pub code_challenge: String,
    #[serde(default)]
    pub code_challenge_method: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

impl AuthorizeQuery {
    fn asked(&self) -> AuthorizeRequest {
        AuthorizeRequest {
            client_id: self.client_id.clone(),
            redirect_uri: self.redirect_uri.clone(),
            challenge: self.code_challenge.clone(),
            method: self.code_challenge_method.clone(),
        }
    }
}

/// Where a browser arrives when a client wants to connect.
///
/// It goes straight to the panel with the question intact. Nothing is checked
/// here that the panel does not immediately ask about, because a refusal
/// rendered as this application's error body is a page nobody can act on.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    get,
    path = "/oauth/authorize",
    tag = "oauth",
    responses((status = 303, description = "On to the panel, where somebody is asked"))
)]
pub async fn authorize(Query(query): Query<AuthorizeQuery>) -> Response {
    let mut carried = form_urlencoded::Serializer::new(String::new());
    carried
        .append_pair("response_type", &query.response_type)
        .append_pair("client_id", &query.client_id)
        .append_pair("redirect_uri", &query.redirect_uri)
        .append_pair("code_challenge", &query.code_challenge)
        .append_pair("code_challenge_method", &query.code_challenge_method);
    if let Some(state) = &query.state {
        carried.append_pair("state", state);
    }

    Redirect::to(&format!("/admin/oauth?{}", carried.finish())).into_response()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Asking {
    /// What the program calls itself. Shown to somebody who is being asked to
    /// trust it, and self-reported — which the screen says.
    pub client_name: String,
    pub redirect_uri: String,
    pub site_title: String,
    pub username: String,
}

/// What the consent screen needs to show. Refuses unless somebody is signed
/// in, which is what sends them to the sign-in page first.
#[utoipa::path(
    get,
    path = "/oauth/request",
    tag = "oauth",
    responses(
        (status = 200, description = "What is being asked", body = Asking),
        (status = 401, description = "Nobody is signed in", body = crate::error::ErrorBody),
    )
)]
pub async fn request(
    Site(state): Site,
    cookies: Cookies,
    Query(query): Query<AuthorizeQuery>,
) -> AppResult<Json<Asking>> {
    let db = state.db_or_unavailable()?;
    let user = crate::auth::authenticate(&state, &cookies, None).await?;

    if query.response_type != "code" {
        return Err(AppError::Validation(
            "this site only hands out authorization codes".to_string(),
        ));
    }

    let client = oauth::check(db, &query.asked()).await?;
    let site_title = <crate::entities::site_settings::Entity as sea_orm::EntityTrait>::find()
        .one(db)
        .await?
        .map(|settings| settings.site_title)
        .unwrap_or_else(|| "this site".to_string());

    Ok(Json(Asking {
        client_name: client.name,
        redirect_uri: query.redirect_uri.clone(),
        site_title,
        username: user.username,
    }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Granted {
    /// Where the browser should go next, code and all.
    pub redirect: String,
}

/// Somebody said yes.
#[utoipa::path(
    post,
    path = "/oauth/grant",
    tag = "oauth",
    responses((status = 200, description = "Where to send the browser", body = Granted))
)]
pub async fn grant(
    Site(state): Site,
    cookies: Cookies,
    Json(query): Json<AuthorizeQuery>,
) -> AppResult<Json<Granted>> {
    let db = state.db_or_unavailable()?;
    let user = crate::auth::authenticate(&state, &cookies, None).await?;

    let code = oauth::approve(db, &query.asked(), user.id).await?;

    let mut answer = form_urlencoded::Serializer::new(String::new());
    answer.append_pair("code", &code);
    if let Some(state) = &query.state {
        answer.append_pair("state", state);
    }

    Ok(Json(Granted {
        redirect: with_query(&query.redirect_uri, &answer.finish()),
    }))
}

/// Puts a query on an address that may already have one.
///
/// Built separately and joined rather than appended to: a serializer handed a
/// string that already ends in "?" writes its own separator before the first
/// pair, and "?&code=" is the kind of thing that works everywhere until it
/// meets the one client that parses strictly.
fn with_query(uri: &str, query: &str) -> String {
    let separator = if uri.contains('?') { '&' } else { '?' };
    format!("{uri}{separator}{query}")
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TokenRequest {
    #[serde(default)]
    pub grant_type: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub code_verifier: String,
    #[serde(default)]
    pub redirect_uri: String,
    #[serde(default)]
    pub refresh_token: String,
}

/// The code, or a renewal, for a token.
///
/// Form-encoded and answered in OAuth's own error shape rather than this
/// application's: a client reads `error` and acts on it, and would only be
/// able to show a person the message in ours.
#[utoipa::path(
    // Answers without a credential.
    security(()),
    post,
    path = "/oauth/token",
    tag = "oauth",
    responses(
        (status = 200, description = "An access token", body = crate::oauth::Tokens),
        (status = 400, description = "The grant could not be used"),
    )
)]
pub async fn token(Site(state): Site, Form(payload): Form<TokenRequest>) -> Response {
    let db = match state.db_or_unavailable() {
        Ok(db) => db,
        Err(err) => return refusal("temporarily_unavailable", &err.to_string()),
    };

    let issued = match payload.grant_type.as_str() {
        "authorization_code" => {
            oauth::exchange(
                db,
                &payload.client_id,
                &payload.code,
                &payload.code_verifier,
                &payload.redirect_uri,
            )
            .await
        }
        "refresh_token" => oauth::refresh(db, &payload.client_id, &payload.refresh_token).await,
        other => {
            return refusal(
                "unsupported_grant_type",
                &format!("{other} is not a grant this site hands out"),
            );
        }
    };

    match issued {
        Ok(tokens) => (
            // A token is never a thing to keep a copy of.
            [(header::CACHE_CONTROL, "no-store")],
            Json(tokens),
        )
            .into_response(),
        Err(err) => refusal("invalid_grant", &err.to_string()),
    }
}

fn refusal(code: &str, description: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "error": code, "error_description": description })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::with_query;

    #[test]
    fn a_code_is_added_to_whatever_query_was_already_there() {
        assert_eq!(
            with_query("https://example.test/back", "code=abc"),
            "https://example.test/back?code=abc"
        );
        assert_eq!(
            with_query("https://example.test/back?from=somewhere", "code=abc"),
            "https://example.test/back?from=somewhere&code=abc"
        );
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Connection {
    pub id: String,
    pub client_name: String,
    pub username: String,
    pub created_at: String,
    pub renewed_at: String,
}

/// The assistants connected to this site.
#[utoipa::path(
    get,
    path = "/oauth/connections",
    tag = "oauth",
    responses((status = 200, description = "Connections", body = Vec<Connection>))
)]
pub async fn connections(
    Site(state): Site,
    _admin: Administrator,
) -> AppResult<Json<Vec<Connection>>> {
    let described = oauth::connections(state.db_or_unavailable()?).await?;

    Ok(Json(
        described
            .into_iter()
            .map(|(grant, client_name, username)| Connection {
                id: grant.id.to_string(),
                client_name,
                username,
                created_at: grant.created_at.to_rfc3339(),
                renewed_at: grant.used_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// Disconnect one. The way in goes with it, immediately.
#[utoipa::path(
    delete,
    path = "/oauth/connections/{id}",
    tag = "oauth",
    params(("id" = String, Path, description = "Connection id")),
    responses((status = 204, description = "Disconnected"))
)]
pub async fn disconnect(
    Site(state): Site,
    _admin: Administrator,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    if oauth::revoke(state.db_or_unavailable()?, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("connection {id}")))
    }
}
