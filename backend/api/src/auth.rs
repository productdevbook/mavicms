use axum::{
    extract::Request,
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use tower_cookies::{Cookie, Cookies, cookie::SameSite};
use uuid::Uuid;

use crate::{
    entities::{session, user},
    error::AppError,
    state::AppState,
    tenants::Site,
};

pub const SESSION_COOKIE: &str = "mavicms_session";
const SESSION_TTL_DAYS: i64 = 30;

/// Creates a session row for `user_id` and sets the session cookie. Used by
/// both `/login` and the setup wizard's final step (which signs the newly
/// created administrator in immediately).
pub async fn create_session(
    db: &DatabaseConnection,
    cookies: &Cookies,
    user_id: Uuid,
) -> Result<(), AppError> {
    let now = Utc::now().fixed_offset();
    let id = Uuid::now_v7();

    let record = session::ActiveModel {
        id: Set(id),
        user_id: Set(user_id),
        expires_at: Set((Utc::now() + Duration::days(SESSION_TTL_DAYS)).fixed_offset()),
        created_at: Set(now),
    };
    record.insert(db).await?;

    let mut cookie = Cookie::new(SESSION_COOKIE, id.to_string());
    cookie.set_http_only(true);
    cookie.set_secure(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookies.add(cookie);
    Ok(())
}

/// Clears the session cookie and, best-effort, its backing row. Never fails
/// — logging out with no active session is a no-op, not an error.
pub async fn clear_session(state: &AppState, cookies: &Cookies) {
    if let Some(cookie) = cookies.get(SESSION_COOKIE)
        && let (Some(db), Ok(id)) = (state.db.as_ref(), Uuid::parse_str(cookie.value()))
    {
        let _ = session::Entity::delete_by_id(id).exec(db).await;
    }

    let mut removal = Cookie::new(SESSION_COOKIE, "");
    removal.set_path("/");
    cookies.remove(removal);
}

/// Rejects requests with 401 unless a valid, unexpired session cookie is
/// present, and makes the signed-in user available to handlers via
/// `Extension<user::Model>`.
pub async fn require_auth(
    Site(state): Site,
    cookies: Cookies,
    mut request: Request,
    next: Next,
) -> Response {
    let bearer = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);

    match authenticate(&state, &cookies, bearer.as_deref()).await {
        Ok(user) => {
            // A build reads; it does not write. The token exists so that a
            // build does not need somebody's password, and a read-only token
            // that can still delete posts would not have been worth having.
            if user.role == BUILDER && !matches!(*request.method(), Method::GET | Method::HEAD) {
                return AppError::Forbidden(
                    "a build token can read this site and nothing else".to_string(),
                )
                .into_response();
            }

            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Err(err) => err.into_response(),
    }
}

/// The role a build runs as. It reads the site to build its pages and has no
/// reason to change anything.
pub const BUILDER: &str = "builder";

async fn authenticate(
    state: &AppState,
    cookies: &Cookies,
    bearer: Option<&str>,
) -> Result<user::Model, AppError> {
    let db = state.db_or_unavailable()?;

    // A bearer token is the same session id by another route, for something
    // holding a token rather than a cookie jar — a build, chiefly.
    if let Some(token) = bearer
        && let Ok(session_id) = Uuid::parse_str(token)
    {
        return by_session(db, session_id).await;
    }

    let token = cookies
        .get(SESSION_COOKIE)
        .ok_or_else(|| AppError::Unauthorized("not signed in".to_string()))?;
    let session_id = Uuid::parse_str(token.value())
        .map_err(|_| AppError::Unauthorized("not signed in".to_string()))?;

    by_session(db, session_id).await
}

async fn by_session(db: &DatabaseConnection, session_id: Uuid) -> Result<user::Model, AppError> {
    let session_row = session::Entity::find_by_id(session_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("session expired".to_string()))?;

    if session_row.expires_at < Utc::now() {
        return Err(AppError::Unauthorized("session expired".to_string()));
    }

    user::Entity::find_by_id(session_row.user_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("session expired".to_string()))
}

/// The role every account has today, and the one the dangerous things ask for.
pub const ADMINISTRATOR: &str = "administrator";

/// Proof that the signed-in account is an administrator of this site.
///
/// Everyone is one today, so this changes nothing yet. It is here because the
/// things it guards — replacing every row a site has — are the things that
/// must not quietly become available to an editor the day editors exist.
pub struct Administrator(pub user::Model);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Administrator {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<user::Model>()
            .cloned()
            .ok_or_else(|| AppError::Unauthorized("not signed in".to_string()))?;

        if user.role != ADMINISTRATOR {
            return Err(AppError::Forbidden(
                "only an administrator of this site can do that".to_string(),
            ));
        }
        Ok(Administrator(user))
    }
}
