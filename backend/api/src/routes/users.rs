//! The people who write a site.
//!
//! Separate from the agency accounts in the console: these live in the site's
//! own schema and are who its posts belong to. An agency reaches them the same
//! way it reaches anything else in a site — by walking into it.

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use axum::{Extension, Json, extract::Path, http::StatusCode};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{ADMINISTRATOR, Administrator},
    entities::{session, user},
    error::{AppError, AppResult},
    tenants::Site,
};

/// The shortest password this will store.
const MINIMUM_PASSWORD: usize = 10;

#[derive(Debug, Serialize, ToSchema)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: String,
    /// False for the account an agency arrives on: it has no password, and the
    /// only way in is a fresh link from the console.
    pub can_sign_in: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    /// A new password for this account. An administrator setting somebody
    /// else's is how a forgotten one is dealt with; there is no email to send.
    pub password: Option<String>,
}

fn hash(password: &str) -> AppResult<String> {
    if password.chars().count() < MINIMUM_PASSWORD {
        return Err(AppError::Validation(format!(
            "the password must be at least {MINIMUM_PASSWORD} characters"
        )));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hashed| hashed.to_string())
        .map_err(|err| AppError::Internal(format!("could not hash the password: {err}")))
}

fn clean_username(username: &str) -> AppResult<String> {
    let username = username.trim().to_string();
    let usable = (3..=32).contains(&username.chars().count())
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    if !usable {
        return Err(AppError::Validation(
            "the username must be 3-32 letters, numbers, _ or -".to_string(),
        ));
    }
    Ok(username)
}

fn clean_email(email: &str) -> AppResult<String> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 3 {
        return Err(AppError::Validation(
            "that does not look like an email address".to_string(),
        ));
    }
    Ok(email)
}

fn row(model: user::Model) -> UserRow {
    UserRow {
        id: model.id.to_string(),
        username: model.username,
        email: model.email,
        role: model.role,
        can_sign_in: !model.password_hash.is_empty(),
        created_at: model.created_at.to_rfc3339(),
    }
}

/// Who can sign in to this site.
#[utoipa::path(
    get,
    path = "/users",
    tag = "users",
    responses((status = 200, description = "The site's people", body = Vec<UserRow>))
)]
pub async fn list_users(Site(state): Site) -> AppResult<Json<Vec<UserRow>>> {
    let people = user::Entity::find().all(state.db_or_unavailable()?).await?;

    Ok(Json(people.into_iter().map(row).collect()))
}

/// Add someone.
#[utoipa::path(
    post,
    path = "/users",
    tag = "users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "Added", body = UserRow),
        (status = 409, description = "That name or address is taken", body = crate::error::ErrorBody),
    )
)]
pub async fn create_user(
    _admin: Administrator,
    Site(state): Site,
    Json(payload): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<UserRow>)> {
    let db = state.db_or_unavailable()?;
    let username = clean_username(&payload.username)?;
    let email = clean_email(&payload.email)?;
    let password_hash = hash(&payload.password)?;

    if taken(db, &username, &email, None).await? {
        return Err(AppError::Conflict(
            "somebody here already has that name or address".to_string(),
        ));
    }

    let created = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username),
        email: Set(email),
        password_hash: Set(password_hash),
        role: Set(ADMINISTRATOR.to_string()),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok((StatusCode::CREATED, Json(row(created))))
}

async fn taken(
    db: &sea_orm::DatabaseConnection,
    username: &str,
    email: &str,
    except: Option<Uuid>,
) -> AppResult<bool> {
    let clash = user::Entity::find()
        .filter(
            sea_orm::Condition::any()
                .add(user::Column::Username.eq(username))
                .add(user::Column::Email.eq(email)),
        )
        .all(db)
        .await?;

    Ok(clash.into_iter().any(|found| Some(found.id) != except))
}

/// Change someone's address, or give them a new password.
#[utoipa::path(
    put,
    path = "/users/{id}",
    tag = "users",
    params(("id" = String, Path, description = "User id")),
    request_body = UpdateUserRequest,
    responses((status = 200, description = "Changed", body = UserRow))
)]
pub async fn update_user(
    _admin: Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUserRequest>,
) -> AppResult<Json<UserRow>> {
    let db = state.db_or_unavailable()?;
    let existing = user::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("user".to_string()))?;

    let mut changed: user::ActiveModel = existing.clone().into();

    if let Some(email) = payload.email {
        let email = clean_email(&email)?;
        if taken(db, &existing.username, &email, Some(id)).await? {
            return Err(AppError::Conflict(
                "somebody here already has that address".to_string(),
            ));
        }
        changed.email = Set(email);
    }

    if let Some(password) = payload.password {
        changed.password_hash = Set(hash(&password)?);
        // Every session this account had is now somebody else's business:
        // a new password that leaves the old windows open is not a new
        // password.
        session::Entity::delete_many()
            .filter(session::Column::UserId.eq(id))
            .exec(db)
            .await?;
    }

    Ok(Json(row(changed.update(db).await?)))
}

/// Remove someone.
#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "users",
    params(("id" = String, Path, description = "User id")),
    responses(
        (status = 204, description = "Removed"),
        (status = 409, description = "The last account cannot go", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_user(
    Administrator(who): Administrator,
    Site(state): Site,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;

    if who.id == id {
        return Err(AppError::Validation(
            "you cannot remove your own account".to_string(),
        ));
    }

    // Counting the ones that can actually sign in: the agency's passwordless
    // account is not somebody who can let themselves back in.
    let people = user::Entity::find().all(db).await?;
    let remaining = people
        .iter()
        .filter(|person| person.id != id && !person.password_hash.is_empty())
        .count();
    if remaining == 0 {
        return Err(AppError::Conflict(
            "this is the last account that can sign in".to_string(),
        ));
    }

    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(id))
        .exec(db)
        .await?;
    user::Entity::delete_by_id(id).exec(db).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Change your own password.
///
/// The current one is asked for even though the session already proves who
/// this is: a session left open on a shared machine should not be enough to
/// take the account away from whoever owns it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[utoipa::path(
    post,
    path = "/me/password",
    tag = "users",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Changed"),
        (status = 401, description = "That is not the current password", body = crate::error::ErrorBody),
    )
)]
pub async fn change_own_password(
    Extension(who): Extension<user::Model>,
    Site(state): Site,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<StatusCode> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};

    let db = state.db_or_unavailable()?;

    if who.password_hash.is_empty() {
        return Err(AppError::Forbidden(
            "this account signs in from the console, not with a password".to_string(),
        ));
    }

    let parsed = PasswordHash::new(&who.password_hash)
        .map_err(|_| AppError::Internal("corrupt password hash".to_string()))?;
    Argon2::default()
        .verify_password(payload.current_password.as_bytes(), &parsed)
        .map_err(|_| AppError::Unauthorized("that is not the current password".to_string()))?;

    let mut changed: user::ActiveModel = who.clone().into();
    changed.password_hash = Set(hash(&payload.new_password)?);
    changed.update(db).await?;

    // Everywhere else stays signed out; this browser signs in again.
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(who.id))
        .exec(db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
