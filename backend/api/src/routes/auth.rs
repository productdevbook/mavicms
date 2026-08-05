use argon2::{Argon2, password_hash::PasswordVerifier};
use axum::{Extension, Json, extract::State, http::StatusCode};
use password_hash::PasswordHash;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tower_cookies::Cookies;

use crate::{
    auth::{clear_session, create_session},
    dto::auth::{LoginRequest, UserResponse},
    entities::user,
    error::{AppError, AppResult},
    state::AppState,
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
    State(state): State<AppState>,
    cookies: Cookies,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Json<UserResponse>> {
    let db = state.db_or_unavailable()?;
    let invalid = || AppError::Unauthorized("invalid username or password".to_string());

    let user = user::Entity::find()
        .filter(user::Column::Username.eq(payload.username))
        .one(db)
        .await?
        .ok_or_else(invalid)?;

    let parsed_hash =
        PasswordHash::new(&user.password_hash).map_err(|_| AppError::Internal("corrupt password hash".to_string()))?;
    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| invalid())?;

    create_session(db, &cookies, user.id).await?;

    Ok(Json(UserResponse {
        username: user.username,
        email: user.email,
        role: user.role,
    }))
}

/// Sign out, clearing the session cookie. Always succeeds.
#[utoipa::path(
    post,
    path = "/logout",
    tag = "auth",
    responses((status = 204, description = "Signed out"))
)]
pub async fn logout(State(state): State<AppState>, cookies: Cookies) -> StatusCode {
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
pub async fn me(Extension(user): Extension<user::Model>) -> Json<UserResponse> {
    Json(UserResponse {
        username: user.username,
        email: user.email,
        role: user.role,
    })
}
