use argon2::{Argon2, password_hash::PasswordVerifier};
use axum::{Extension, Json, http::StatusCode};
use password_hash::PasswordHash;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tower_cookies::Cookies;

use crate::{
    auth::{clear_session, create_session},
    dto::auth::{LoginRequest, UserResponse},
    entities::user,
    error::{AppError, AppResult},
    tenants::{Resolved, Site},
};

/// Sign in with a username and password, setting a session cookie.
#[utoipa::path(
    post,
    path = "/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in", body = UserResponse),
        (status = 401, description = "Invalid username or password", body = crate::error::ErrorBody),
    )
)]
pub async fn login(
    Site(state): Site,
    Extension(resolved): Extension<Resolved>,
    headers: axum::http::HeaderMap,
    cookies: Cookies,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<UserResponse>> {
    let db = state.db_or_unavailable()?;

    // Both keys are checked before the password is read, and both are counted
    // after: guessing one account from many addresses and many accounts from
    // one address are the same attack wearing different hats.
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let keys = [
        format!("login:who:{}:{}", host, payload.username),
        format!("login:from:{}", crate::throttle::caller(&headers)),
    ];
    if let Some(left) = keys.iter().find_map(|key| crate::throttle::pause_left(key)) {
        return Err(crate::error::AppError::TooManyRequests(
            left.as_secs().max(1),
        ));
    }

    let counted = |result: AppResult<Json<UserResponse>>| match result {
        Ok(value) => {
            keys.iter().for_each(|key| crate::throttle::right(key));
            Ok(value)
        }
        Err(err) => {
            keys.iter().for_each(|key| {
                crate::throttle::wrong(key);
            });
            Err(err)
        }
    };

    counted(sign_in(db, &cookies, payload, resolved).await)
}

async fn sign_in(
    db: &sea_orm::DatabaseConnection,
    cookies: &Cookies,
    payload: LoginRequest,
    resolved: Resolved,
) -> AppResult<Json<UserResponse>> {
    let invalid = || AppError::Unauthorized("invalid username or password".to_string());

    let user = user::Entity::find()
        .filter(user::Column::Username.eq(payload.username))
        .one(db)
        .await?
        .ok_or_else(invalid)?;

    // An account an agency arrives on through a one-time link has no password
    // at all. That is a wrong password like any other, not a broken row — and
    // saying so differently would tell whoever is guessing which accounts
    // cannot be guessed at.
    if user.password_hash.is_empty() {
        return Err(invalid());
    }

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| AppError::Internal("corrupt password hash".to_string()))?;
    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| invalid())?;

    create_session(db, cookies, user.id).await?;

    Ok(Json(UserResponse {
        username: user.username,
        email: user.email,
        role: user.role,
        operator: resolved.is_host(),
    }))
}

/// Sign out, clearing the session cookie. Always succeeds.
#[utoipa::path(
    post,
    path = "/logout",
    tag = "auth",
    responses((status = 204, description = "Signed out"))
)]
pub async fn logout(Site(state): Site, cookies: Cookies) -> StatusCode {
    clear_session(&state, &cookies).await;
    StatusCode::NO_CONTENT
}

/// The currently signed-in user.
#[utoipa::path(
    get,
    path = "/me",
    tag = "auth",
    responses(
        (status = 200, description = "Signed-in user", body = UserResponse),
        (status = 401, description = "Not signed in", body = crate::error::ErrorBody),
    )
)]
pub async fn me(
    Extension(resolved): Extension<Resolved>,
    Extension(user): Extension<user::Model>,
) -> Json<UserResponse> {
    Json(UserResponse {
        username: user.username,
        email: user.email,
        role: user.role,
        operator: resolved.is_host(),
    })
}
