//! A key an assistant can actually work with, good for a day.
//!
//! What the panel handed an assistant before this was `llms.txt`: an accurate
//! and quite long description of an API it had no way of reaching. The only
//! credential a site could make was the build token, which reads and nothing
//! else, so the assistant read the document, wrote the code, and could not run
//! any of it — or wrote a post and was told 403 by a site it had just been
//! given the keys to on paper.
//!
//! So the document is handed over with a key in it. It writes, it lasts a day,
//! it is revocable, and the handful of things it may not do — accounts, other
//! keys, the settings that hold other services' credentials, and the two
//! actions there is no way back from — are refused at the door in
//! [`crate::auth`] rather than trusted to the assistant's discretion.

use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, StatusCode},
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::{
    auth::{ASSISTANT, Administrator},
    dto::assistant::{AssistantKey, Handover},
    entities::{session, user},
    error::{AppError, AppResult},
    tenants::Site,
};

/// How long a key lasts.
///
/// A day, because it is pasted into somebody else's program and read by a
/// model: it will end up in a transcript, in a log, and possibly in a
/// screenshot, and none of those should still be a way in next week. Long
/// enough that an afternoon's work does not stop halfway.
pub const KEY_HOURS: i64 = 24;

/// The account keys are made against.
///
/// Its own, not the person's: what an assistant did should read as an
/// assistant in the bin and in a post's author, a key should be revocable
/// without ending somebody's own session, and the limits in
/// [`crate::auth::off_limits`] hang off the role rather than off the request.
pub const ASSISTANT_ACCOUNT: &str = "assistant";

pub(crate) async fn account(db: &DatabaseConnection) -> AppResult<Uuid> {
    if let Some(found) = user::Entity::find()
        .filter(user::Column::Username.eq(ASSISTANT_ACCOUNT))
        .one(db)
        .await?
    {
        return Ok(found.id);
    }

    let created = user::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(ASSISTANT_ACCOUNT.to_string()),
        email: Set(format!("{ASSISTANT_ACCOUNT}@localhost")),
        // Empty, and never set: there is no password to sign in with, so the
        // only way to be this account is to hold a key somebody made.
        password_hash: Set(String::new()),
        role: Set(ASSISTANT.to_string()),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    Ok(created.id)
}

async fn keys(db: &DatabaseConnection) -> AppResult<Vec<AssistantKey>> {
    let holder = account(db).await?;
    Ok(session::Entity::find()
        .filter(session::Column::UserId.eq(holder))
        .order_by(session::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?
        .into_iter()
        .map(|row| AssistantKey {
            id: row.id.to_string(),
            created_at: row.created_at.to_rfc3339(),
            expires_at: row.expires_at.to_rfc3339(),
        })
        .collect())
}

/// The keys this site has out.
#[utoipa::path(
    get,
    path = "/assistant/keys",
    tag = "development",
    responses((status = 200, description = "Keys", body = Vec<AssistantKey>))
)]
pub async fn list_keys(
    Site(state): Site,
    _admin: Administrator,
) -> AppResult<Json<Vec<AssistantKey>>> {
    Ok(Json(keys(state.db_or_unavailable()?).await?))
}

/// Make a key and hand over the document with it in.
///
/// One call rather than two, because the two are one act: a key with no
/// document is a string somebody has to explain, and a document with no key is
/// what this site used to hand out.
#[utoipa::path(
    post,
    path = "/assistant/handover",
    tag = "development",
    responses((status = 201, description = "The key, the once, and the document", body = Handover))
)]
pub async fn handover(
    Site(state): Site,
    _admin: Administrator,
    headers: HeaderMap,
) -> AppResult<(StatusCode, Json<Handover>)> {
    let db = state.db_or_unavailable()?;

    let holder = account(db).await?;
    let key = Uuid::now_v7();
    let expires_at = Utc::now() + Duration::hours(KEY_HOURS);

    session::ActiveModel {
        id: Set(key),
        user_id: Set(holder),
        expires_at: Set(expires_at.fixed_offset()),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;

    let text =
        super::llms::handover_document(&state, &headers, &key.to_string(), expires_at).await?;

    Ok((
        StatusCode::CREATED,
        Json(Handover {
            id: key.to_string(),
            token: key.to_string(),
            expires_at: expires_at.to_rfc3339(),
            text,
        }),
    ))
}

/// Take one back before it runs out.
#[utoipa::path(
    delete,
    path = "/assistant/keys/{id}",
    tag = "development",
    params(("id" = String, Path, description = "Key id")),
    responses((status = 204, description = "Revoked"))
)]
pub async fn delete_key(
    Site(state): Site,
    _admin: Administrator,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db_or_unavailable()?;
    let holder = account(db).await?;

    let result = session::Entity::delete_many()
        .filter(session::Column::Id.eq(id))
        // Only this account's. Every session id lives in this table, so
        // without this an administrator could end a colleague's login by
        // pasting its id here.
        .filter(session::Column::UserId.eq(holder))
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("key {id}")));
    }

    Ok(StatusCode::NO_CONTENT)
}
