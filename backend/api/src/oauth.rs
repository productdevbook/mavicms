//! Letting somebody connect an assistant without handing it a password.
//!
//! Claude and ChatGPT will both connect to a remote MCP server, and neither of
//! them has anywhere to put a token: their dialogs ask for an address and then
//! send you through the site's own sign-in. That flow is OAuth, and without it
//! the whole feature is reachable only by somebody willing to run a command in
//! a terminal — which is not who writes a site.
//!
//! What is implemented is the small part of OAuth that this needs: an
//! authorization code, PKCE, and refresh. Public clients only — an assistant
//! on somebody's laptop cannot keep a secret, so none is issued and the code
//! is bound to whoever asked for it instead.
//!
//! The access token is a row in `sessions`, the same as a browser's cookie and
//! a build's token. That is deliberate: one place decides who a caller is, and
//! `oauth_grants` records which connection a session came from so the panel
//! can name them and take one back.

use base64::Engine as _;
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    entities::{oauth, session, user},
    error::{AppError, AppResult},
};

/// How long the code is good for. Long enough to be exchanged, short enough
/// that one left in a log or a browser history is worth nothing.
const CODE_SECONDS: i64 = 60;

/// How long a connection reads for before it renews itself.
const ACCESS_DAYS: i64 = 30;

/// What a connection may do. There is one, because a connection is either the
/// person's own reach into their site or it is nothing; the tools an assistant
/// is offered are already decided by the account it connects as.
pub const SCOPE: &str = "mcp";

/// The metadata a client reads to find out how to ask.
pub fn authorization_server(origin: &str) -> serde_json::Value {
    serde_json::json!({
        "issuer": origin,
        "authorization_endpoint": format!("{origin}/api/oauth/authorize"),
        "token_endpoint": format!("{origin}/api/oauth/token"),
        "registration_endpoint": format!("{origin}/api/oauth/register"),
        "scopes_supported": [SCOPE],
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        // Public clients, so no secret is presented at the token endpoint.
        "token_endpoint_auth_methods_supported": ["none"],
        // S256 and not "plain": plain is a challenge that proves nothing.
        "code_challenge_methods_supported": ["S256"],
    })
}

/// What the protected resource says about who guards it.
pub fn protected_resource(origin: &str) -> serde_json::Value {
    serde_json::json!({
        "resource": format!("{origin}/api/mcp"),
        "authorization_servers": [origin],
        "scopes_supported": [SCOPE],
        "bearer_methods_supported": ["header"],
    })
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct Registration {
    #[serde(default)]
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Registered {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: &'static str,
    pub grant_types: Vec<&'static str>,
    pub response_types: Vec<&'static str>,
}

/// A client saying who it is and where it should be sent back to.
///
/// Anybody may register. That sounds worse than it is: registering creates a
/// name and a list of addresses and nothing else — no access to anything. What
/// decides whether a connection happens is the person who has to sign in and
/// agree to it on the next screen.
pub async fn register(db: &DatabaseConnection, request: Registration) -> AppResult<Registered> {
    let redirect_uris: Vec<String> = request
        .redirect_uris
        .iter()
        .map(|uri| uri.trim().to_string())
        .filter(|uri| !uri.is_empty())
        .collect();

    if redirect_uris.is_empty() {
        return Err(AppError::Validation(
            "a client has to say where it should be sent back to".to_string(),
        ));
    }
    for uri in &redirect_uris {
        check_redirect(uri)?;
    }

    let name = match request.client_name.trim() {
        "" => "An assistant".to_string(),
        given => given.chars().take(200).collect(),
    };

    let id = opaque();
    oauth::client::ActiveModel {
        id: Set(id.clone()),
        name: Set(name.clone()),
        redirect_uris: Set(redirect_uris.join("\n")),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(Registered {
        client_id: id,
        client_name: name,
        redirect_uris,
        token_endpoint_auth_method: "none",
        grant_types: vec!["authorization_code", "refresh_token"],
        response_types: vec!["code"],
    })
}

/// Where a client may be sent back to.
///
/// `https` anywhere, and plaintext only to the machine the browser is on —
/// which is what a desktop client listening on a loopback port needs, and is
/// not a way to send somebody's code across a network in the clear.
fn check_redirect(uri: &str) -> AppResult<()> {
    let refuse = || {
        AppError::Validation(format!(
            "{uri} is not somewhere this will send an authorization code"
        ))
    };

    if let Some(rest) = uri.strip_prefix("https://") {
        return (!rest.is_empty()).then_some(()).ok_or_else(refuse);
    }

    if let Some(rest) = uri.strip_prefix("http://") {
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .split(':')
            .next()
            .unwrap_or_default();
        if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
            return Ok(());
        }
        return Err(refuse());
    }

    // A private-use scheme, which is how a native application is called back.
    // It must be a scheme and not a bare word, and must not be one a browser
    // would follow to somewhere.
    if let Some((scheme, rest)) = uri.split_once(':')
        && !scheme.is_empty()
        && !rest.is_empty()
        && scheme.contains('.')
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Ok(());
    }

    Err(refuse())
}

pub struct AuthorizeRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub challenge: String,
    pub method: String,
}

/// Everything about a request to connect that can be checked before a person
/// is asked to agree to it.
pub async fn check(
    db: &DatabaseConnection,
    request: &AuthorizeRequest,
) -> AppResult<oauth::client::Model> {
    let client = oauth::client::Entity::find_by_id(&request.client_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Validation("that program is not registered here".to_string()))?;

    // Checked before anything is shown, and never redirected to on failure: an
    // address the client did not register is where an attacker would like the
    // code sent, so the answer belongs on this screen rather than in a
    // redirect.
    if !client.may_redirect_to(&request.redirect_uri) {
        return Err(AppError::Validation(
            "that program did not register the address it is asking to be sent back to".to_string(),
        ));
    }

    if !request.method.eq_ignore_ascii_case("S256") {
        return Err(AppError::Validation(
            "the code challenge has to be S256".to_string(),
        ));
    }
    if request.challenge.len() < 43 {
        return Err(AppError::Validation(
            "that code challenge is too short to be an S256 digest".to_string(),
        ));
    }

    Ok(client)
}

/// The person said yes. Hands out a code for the client to come back with.
pub async fn approve(
    db: &DatabaseConnection,
    request: &AuthorizeRequest,
    user_id: Uuid,
) -> AppResult<String> {
    check(db, request).await?;

    let code = opaque();
    oauth::code::ActiveModel {
        code: Set(code.clone()),
        client_id: Set(request.client_id.clone()),
        user_id: Set(user_id),
        redirect_uri: Set(request.redirect_uri.clone()),
        challenge: Set(request.challenge.clone()),
        expires_at: Set((Utc::now() + Duration::seconds(CODE_SECONDS)).fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(code)
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Tokens {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub refresh_token: String,
    pub scope: &'static str,
}

/// The client comes back with the code and the verifier behind its challenge.
pub async fn exchange(
    db: &DatabaseConnection,
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> AppResult<Tokens> {
    let invalid = || AppError::Validation("that code cannot be exchanged".to_string());

    let held = oauth::code::Entity::find_by_id(code)
        .one(db)
        .await?
        .ok_or_else(invalid)?;

    // Used or not, it is gone: a code is good once, and deleting it before
    // anything else is checked is what makes a second attempt with a guessed
    // verifier pointless.
    oauth::code::Entity::delete_by_id(code).exec(db).await?;

    if held.expires_at < Utc::now() {
        return Err(invalid());
    }
    if held.client_id != client_id || held.redirect_uri != redirect_uri {
        return Err(invalid());
    }
    if !verified(&held.challenge, verifier) {
        return Err(invalid());
    }

    issue(db, &held.client_id, held.user_id).await
}

/// The connection renews itself. The refresh token is replaced as it is used,
/// so a copy that was taken stops working the moment the real client renews —
/// and the copy being used first is a refusal the real client will notice.
pub async fn refresh(db: &DatabaseConnection, client_id: &str, token: &str) -> AppResult<Tokens> {
    let invalid = || AppError::Validation("that refresh token cannot be used".to_string());

    let grant = oauth::grant::Entity::find()
        .filter(oauth::grant::Column::Refresh.eq(token))
        .one(db)
        .await?
        .ok_or_else(invalid)?;

    if grant.client_id != client_id {
        return Err(invalid());
    }

    // The session behind the old one goes with it, so a renewal leaves one
    // way in rather than a growing pile of them.
    session::Entity::delete_by_id(grant.session_id)
        .exec(db)
        .await?;
    oauth::grant::Entity::delete_by_id(grant.id)
        .exec(db)
        .await?;

    issue(db, &grant.client_id, grant.user_id).await
}

async fn issue(db: &DatabaseConnection, client_id: &str, user_id: Uuid) -> AppResult<Tokens> {
    let expires_at = Utc::now() + Duration::days(ACCESS_DAYS);
    let access = Uuid::now_v7();

    session::ActiveModel {
        id: Set(access),
        user_id: Set(user_id),
        expires_at: Set(expires_at.fixed_offset()),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    let refresh = opaque();
    oauth::grant::ActiveModel {
        id: Set(Uuid::now_v7()),
        client_id: Set(client_id.to_string()),
        user_id: Set(user_id),
        session_id: Set(access),
        refresh: Set(refresh.clone()),
        created_at: Set(Utc::now().fixed_offset()),
        used_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(Tokens {
        access_token: access.to_string(),
        token_type: "Bearer",
        expires_in: Duration::days(ACCESS_DAYS).num_seconds(),
        refresh_token: refresh,
        scope: SCOPE,
    })
}

/// Whether the verifier is the one the challenge was made from.
fn verified(challenge: &str, verifier: &str) -> bool {
    // Not the empty string dressed up: a verifier is at least 43 characters by
    // the specification, and checking it here means a client that sends
    // nothing is refused rather than compared.
    if verifier.len() < 43 {
        return false;
    }

    let digest = Sha256::digest(verifier.as_bytes());
    let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    // Not constant-time, and it does not need to be: both sides are already
    // known to whoever sent the request, and the secret is the verifier, which
    // is not being compared to anything a caller can vary.
    expected == challenge
}

/// The connections this site has out, newest first.
pub async fn connections(
    db: &DatabaseConnection,
) -> AppResult<Vec<(oauth::grant::Model, String, String)>> {
    let grants = oauth::grant::Entity::find()
        .order_by(oauth::grant::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?;

    let mut described = Vec::with_capacity(grants.len());
    for grant in grants {
        let client = oauth::client::Entity::find_by_id(&grant.client_id)
            .one(db)
            .await?
            .map(|client| client.name)
            .unwrap_or_else(|| "A program that is no longer registered".to_string());
        let who = user::Entity::find_by_id(grant.user_id)
            .one(db)
            .await?
            .map(|user| user.username)
            .unwrap_or_else(|| "an account that has gone".to_string());

        described.push((grant, client, who));
    }
    Ok(described)
}

/// Takes a connection back, and the way in that came with it.
pub async fn revoke(db: &DatabaseConnection, id: Uuid) -> AppResult<bool> {
    let Some(grant) = oauth::grant::Entity::find_by_id(id).one(db).await? else {
        return Ok(false);
    };

    session::Entity::delete_by_id(grant.session_id)
        .exec(db)
        .await?;
    oauth::grant::Entity::delete_by_id(id).exec(db).await?;
    Ok(true)
}

/// A value nobody can guess and nothing can be read out of.
///
/// From the operating system, not from a uuid. This codebase mints sessions as
/// v7 uuids, which carry the millisecond they were made — fine for a session
/// id, and worth avoiding for a value that travels through a browser's history
/// and sits in a client's settings file.
fn opaque() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system has randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::{check_redirect, opaque, verified};

    #[test]
    fn a_verifier_has_to_be_the_one_the_challenge_was_made_from() {
        // The example from RFC 7636.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

        assert!(verified(challenge, verifier));
        assert!(!verified(
            challenge,
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXj"
        ));
    }

    #[test]
    fn nothing_short_counts_as_a_verifier() {
        assert!(!verified("", ""));
        assert!(!verified(&super::opaque(), "short"));
    }

    #[test]
    fn a_code_is_only_sent_somewhere_it_could_have_come_from() {
        assert!(check_redirect("https://claude.ai/api/mcp/auth_callback").is_ok());
        assert!(check_redirect("http://localhost:8976/callback").is_ok());
        assert!(check_redirect("http://127.0.0.1:1455/oauth/callback").is_ok());
        assert!(check_redirect("com.example.app:/oauth").is_ok());

        // Plaintext to anywhere but this machine is a code crossing a network
        // in the clear.
        assert!(check_redirect("http://example.test/callback").is_err());
        assert!(check_redirect("javascript:alert(1)").is_err());
        assert!(check_redirect("not a uri").is_err());
        assert!(check_redirect("").is_err());
    }

    #[test]
    fn an_opaque_value_says_nothing_and_repeats_nothing() {
        let first = opaque();
        assert_ne!(first, opaque());
        assert!(first.len() >= 40);
        assert!(!first.contains('='));
    }
}
