//! Build tokens, from a site's own panel.
//!
//! The console hands these out for the sites an agency looks after. A site
//! that has no agency — one person, one server, their own blog — had no way to
//! make one at all, and the API page told them to send `CMS_TOKEN`. So the
//! answer was to sign the build in as themselves and put their own password in
//! a CI variable, which is the thing this whole mechanism exists to avoid.

use axum::{Json, extract::Path, http::StatusCode};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::{
    auth::Administrator,
    dto::plugins::{DevelopmentToken, NewDevelopmentToken},
    entities::session,
    error::{AppError, AppResult},
    routes::console::{DEVELOPMENT_TOKEN_DAYS, development_tokens, local_reader},
    tenants::Site,
};

/// The build tokens this site has out.
#[utoipa::path(
    get,
    path = "/development/tokens",
    tag = "development",
    responses((status = 200, description = "Tokens", body = Vec<DevelopmentToken>))
)]
pub async fn list_tokens(
    Site(state): Site,
    _admin: Administrator,
) -> AppResult<Json<Vec<DevelopmentToken>>> {
    Ok(Json(development_tokens(state.db_or_unavailable()?).await?))
}

/// Make one. It is shown once and not kept anywhere it can be read again.
#[utoipa::path(
    post,
    path = "/development/tokens",
    tag = "development",
    responses((status = 201, description = "The token, the once", body = NewDevelopmentToken))
)]
pub async fn create_token(
    Site(state): Site,
    _admin: Administrator,
) -> AppResult<(StatusCode, Json<NewDevelopmentToken>)> {
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
        Json(NewDevelopmentToken {
            token: token.to_string(),
            expires_at: expires_at.to_rfc3339(),
        }),
    ))
}

/// Take one back.
#[utoipa::path(
    delete,
    path = "/development/tokens/{id}",
    tag = "development",
    params(("id" = String, Path, description = "Token id")),
    responses((status = 204, description = "Revoked"))
)]
pub async fn delete_token(
    Site(state): Site,
    _admin: Administrator,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;
    let reader = local_reader(db).await?;

    let result = session::Entity::delete_many()
        .filter(session::Column::Id.eq(id))
        // Only the build account's. Every session id lives in this table, so
        // without this an administrator could revoke a colleague's login by
        // pasting its id here.
        .filter(session::Column::UserId.eq(reader))
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("token {id}")));
    }

    Ok(StatusCode::NO_CONTENT)
}
