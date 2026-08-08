//! The video library, and the plugin behind it.
//!
//! Everything here is small on purpose. The expensive part of a video — the
//! bytes — never touches this server: the panel asks for a ticket, uploads to
//! the host, and the host says when it is done. What these endpoints move is
//! a row, an address and a signature.
//!
//! # Who may do what, and why it is not the same for all of them
//!
//! Listing, reading and adding are open to anything signed in that may write,
//! which is what `media` already does for images and what makes the assistant
//! key of any use for a course. A build token reaches the two `GET`s and is
//! refused the rest by its own rule, which is that it may not write at all.
//!
//! Two are not like the others, and the asymmetry is deliberate:
//!
//! - **The settings** are administrator-only, like every other plugin's. They
//!   hold another service's credentials.
//! - **Deleting** is administrator-only because it does not go to the bin. A
//!   deleted post waits thirty days; a deleted video is gone from the host as
//!   well, and the only copy left is on somebody's laptop.
//!
//! And one is a placeholder: `playback` mints an address for **whoever asked**,
//! which is right while the only callers are the panel and an assistant
//! previewing. It is not the endpoint a student may ever reach. When the member
//! area arrives it gets its own, which decides enrolment, expiry and drip
//! before it signs anything — see `docs/courses.md`. Wiring a member area to
//! this one would hand every video to anybody with an account.

use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, Order, QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::Administrator,
    entities::video_asset,
    error::{AppError, AppResult},
    state::AppState,
    tenants::Site,
    video::{Facts, Playback, Protection, Status, Ticket, Usage, VIDEO_PLUGIN, VideoConfig},
};

// ---------------------------------------------------------------- settings

/// What the panel shows. No secret comes back out, only whether one is held.
#[derive(Debug, Serialize, ToSchema)]
pub struct VideoSettingsResponse {
    pub enabled: bool,
    pub host: String,
    pub library_id: String,
    pub cdn_hostname: String,
    pub account_id: String,
    pub customer_subdomain: String,
    pub has_api_key: bool,
    pub has_token_key: bool,
    pub has_api_token: bool,
    /// Where the host should post when a video finishes transcoding. Shown so
    /// it can be pasted into the host's own settings, which is the one step
    /// nobody can do for them.
    pub events_url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VideoSettingsRequest {
    pub enabled: bool,
    pub host: String,
    #[serde(default)]
    pub library_id: String,
    #[serde(default)]
    pub cdn_hostname: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub customer_subdomain: String,
    /// Left out to keep what is stored.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub token_key: Option<String>,
    #[serde(default)]
    pub api_token: Option<String>,
}

async fn stored(state: &AppState) -> AppResult<(bool, VideoConfig)> {
    let found =
        crate::plugins::load::<VideoConfig>(state.db(), &state.secrets, VIDEO_PLUGIN).await?;
    Ok(match found {
        Some(stored) => (stored.enabled, stored.config),
        None => (false, VideoConfig::default()),
    })
}

/// The settings a caller may see, and the address to give the host.
fn shown(enabled: bool, config: &VideoConfig, headers: &HeaderMap) -> VideoSettingsResponse {
    VideoSettingsResponse {
        enabled,
        host: config.host.clone(),
        library_id: config.library_id.clone(),
        cdn_hostname: config.cdn_hostname.clone(),
        account_id: config.account_id.clone(),
        customer_subdomain: config.customer_subdomain.clone(),
        has_api_key: !config.api_key.is_empty(),
        has_token_key: !config.token_key.is_empty(),
        has_api_token: !config.api_token.is_empty(),
        events_url: if config.events_token.is_empty() {
            String::new()
        } else {
            format!(
                "{}/api/videos/events/{}",
                crate::routes::llms::origin(headers),
                config.events_token
            )
        },
    }
}

/// Current video settings.
#[utoipa::path(
    get,
    path = "/plugins/video",
    tag = "plugins",
    responses((status = 200, description = "Video settings", body = VideoSettingsResponse))
)]
pub async fn get_settings(
    Site(state): Site,
    _admin: Administrator,
    headers: HeaderMap,
) -> AppResult<Json<VideoSettingsResponse>> {
    let (enabled, config) = stored(&state).await?;
    Ok(Json(shown(enabled, &config, &headers)))
}

/// Merges what was sent over what is held, keeping a secret that was left out.
async fn merged(state: &AppState, payload: &VideoSettingsRequest) -> AppResult<VideoConfig> {
    let (_, mut config) = stored(state).await?;

    let kept = |given: &Option<String>, held: &str| match given.as_deref() {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => held.to_string(),
    };

    config.api_key = kept(&payload.api_key, &config.api_key);
    config.token_key = kept(&payload.token_key, &config.token_key);
    config.api_token = kept(&payload.api_token, &config.api_token);

    config.host = payload.host.trim().to_lowercase();
    config.library_id = payload.library_id.trim().to_string();
    config.cdn_hostname = payload
        .cdn_hostname
        .trim()
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string();
    config.account_id = payload.account_id.trim().to_string();
    config.customer_subdomain = payload
        .customer_subdomain
        .trim()
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_string();

    // Made once and then left alone: it is in an address already pasted into
    // somebody else's dashboard, and changing it would stop the webhook
    // silently — videos would upload, transcode, and never turn ready.
    if config.events_token.is_empty() {
        config.events_token = Uuid::now_v7().simple().to_string();
    }

    Ok(config)
}

/// Save them. Switching it on proves the credentials first.
#[utoipa::path(
    put,
    path = "/plugins/video",
    tag = "plugins",
    request_body = VideoSettingsRequest,
    responses((status = 200, description = "Saved", body = VideoSettingsResponse))
)]
pub async fn update_settings(
    Site(state): Site,
    _admin: Administrator,
    headers: HeaderMap,
    Json(payload): Json<VideoSettingsRequest>,
) -> AppResult<Json<VideoSettingsResponse>> {
    let config = merged(&state, &payload).await?;

    if payload.enabled {
        config.validate()?;
        config.test_connection().await?;
    }

    crate::plugins::save(
        state.db(),
        &state.secrets,
        VIDEO_PLUGIN,
        payload.enabled,
        &config,
    )
    .await?;

    Ok(Json(shown(payload.enabled, &config, &headers)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestResponse {
    pub ok: bool,
    pub message: String,
}

/// Try the credentials without saving them.
#[utoipa::path(
    post,
    path = "/plugins/video/test",
    tag = "plugins",
    request_body = VideoSettingsRequest,
    responses((status = 200, description = "Result", body = TestResponse))
)]
pub async fn test_settings(
    Site(state): Site,
    _admin: Administrator,
    Json(payload): Json<VideoSettingsRequest>,
) -> AppResult<Json<TestResponse>> {
    let config = merged(&state, &payload).await?;

    // A refusal is a valid answer to "does this work?", so it comes back in
    // the body rather than as an HTTP error.
    Ok(Json(match config.test_connection().await {
        Ok(()) => TestResponse {
            ok: true,
            message: "The video host answered.".to_string(),
        },
        Err(err) => TestResponse {
            ok: false,
            message: err.to_string(),
        },
    }))
}

/// Whether the videos are really behind the signature that is put on them.
///
/// Tried against one video that is ready, because there is no way to ask the
/// question in the abstract — and a site with nothing uploaded has nothing
/// exposed either.
#[utoipa::path(
    get,
    path = "/plugins/video/protection",
    tag = "plugins",
    responses((status = 200, description = "What an unsigned address does", body = Protection))
)]
pub async fn get_protection(
    Site(state): Site,
    _admin: Administrator,
) -> AppResult<Json<Protection>> {
    let (enabled, config) = stored(&state).await?;

    let ready = video_asset::Entity::find()
        .filter(video_asset::Column::Status.eq(Status::Ready.name()))
        .order_by(video_asset::Column::CreatedAt, Order::Desc)
        .one(state.db())
        .await?;

    let (Some(video), true) = (ready, enabled) else {
        return Ok(Json(Protection {
            checked: false,
            protected: false,
            message: String::new(),
        }));
    };

    Ok(Json(config.protection(&video.host_id).await?))
}

/// What this site is storing and serving.
#[utoipa::path(
    get,
    path = "/plugins/video/usage",
    tag = "plugins",
    responses((status = 200, description = "Usage", body = Usage))
)]
pub async fn get_usage(Site(state): Site, _admin: Administrator) -> AppResult<Json<Usage>> {
    let rows = video_asset::Entity::find().all(state.db()).await?;

    Ok(Json(Usage {
        stored_seconds: rows.iter().map(|row| i64::from(row.duration_seconds)).sum(),
        stored_bytes: rows.iter().map(|row| row.size_bytes).sum(),
        // Delivery is only knowable where it happened, and reading it costs a
        // call to the host. Left to the reader that bills, not to this page.
        delivered_bytes: 0,
        delivered_minutes: 0,
        since: String::new(),
    }))
}

// ----------------------------------------------------------------- library

#[derive(Debug, Serialize, ToSchema)]
pub struct VideoRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub duration_seconds: i32,
    pub thumbnail_url: String,
    pub size_bytes: i64,
    pub error: String,
    pub host: String,
    pub created_at: String,
}

fn row_of(model: video_asset::Model) -> VideoRow {
    VideoRow {
        id: model.id.to_string(),
        title: model.title,
        status: model.status,
        duration_seconds: model.duration_seconds,
        thumbnail_url: model.thumbnail_url,
        size_bytes: model.size_bytes,
        error: model.error,
        host: model.host,
        created_at: model.created_at.to_rfc3339(),
    }
}

/// The videos this site has.
#[utoipa::path(
    get,
    path = "/videos",
    tag = "videos",
    responses((status = 200, description = "Videos", body = Vec<VideoRow>))
)]
pub async fn list_videos(Site(state): Site) -> AppResult<Json<Vec<VideoRow>>> {
    Ok(Json(
        video_asset::Entity::find()
            .order_by(video_asset::Column::CreatedAt, Order::Desc)
            .all(state.db())
            .await?
            .into_iter()
            .map(row_of)
            .collect(),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NewVideoRequest {
    pub title: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NewVideoResponse {
    pub video: VideoRow,
    pub ticket: Ticket,
}

/// Make a place for a video and say where to send the file.
///
/// The answer is a URL on the host, not on this server. Uploading through here
/// would mean holding a gigabyte in memory to write it out again, on a process
/// that is also answering everybody else's requests.
#[utoipa::path(
    post,
    path = "/videos",
    tag = "videos",
    request_body = NewVideoRequest,
    responses((status = 201, description = "Where to upload", body = NewVideoResponse))
)]
pub async fn create_video(
    Site(state): Site,
    Json(payload): Json<NewVideoRequest>,
) -> AppResult<(StatusCode, Json<NewVideoResponse>)> {
    let (enabled, config) = stored(&state).await?;
    if !enabled {
        return Err(AppError::Validation(
            "no video host is set up for this site yet. Plugins → Video.".to_string(),
        ));
    }

    let title = payload.title.trim();
    let title = if title.is_empty() { "Untitled" } else { title };

    let ticket = config.upload_ticket(title).await?;
    let now = Utc::now().fixed_offset();

    let model = video_asset::ActiveModel {
        id: Set(Uuid::now_v7()),
        host: Set(config.host.clone()),
        host_id: Set(ticket.provider_id.clone()),
        title: Set(title.to_string()),
        status: Set(Status::Uploading.name().to_string()),
        duration_seconds: Set(0),
        thumbnail_url: Set(String::new()),
        size_bytes: Set(0),
        error: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(state.db())
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(NewVideoResponse {
            video: row_of(model),
            ticket,
        }),
    ))
}

async fn find(state: &AppState, id: Uuid) -> AppResult<video_asset::Model> {
    video_asset::Entity::find_by_id(id)
        .one(state.db())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("video {id}")))
}

/// Writes what the host said onto the row.
async fn record(state: &AppState, model: video_asset::Model, facts: Facts) -> AppResult<VideoRow> {
    let mut change: video_asset::ActiveModel = model.into();
    change.status = Set(facts.status.name().to_string());
    change.duration_seconds = Set(facts.duration_seconds);
    if !facts.thumbnail_url.is_empty() {
        change.thumbnail_url = Set(facts.thumbnail_url);
    }
    if facts.bytes > 0 {
        change.size_bytes = Set(facts.bytes);
    }
    change.error = Set(facts.error);
    change.updated_at = Set(Utc::now().fixed_offset());

    Ok(row_of(change.update(state.db()).await?))
}

/// One video, asked of the host if it has not finished yet.
///
/// The webhook is what normally moves a row to ready. This is the fallback for
/// a site whose host has not been given the address, or whose webhook was
/// lost — a panel that never updates is worse than one that polls.
#[utoipa::path(
    get,
    path = "/videos/{id}",
    tag = "videos",
    params(("id" = String, Path, description = "Video id")),
    responses((status = 200, description = "The video", body = VideoRow))
)]
pub async fn get_video(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<Json<VideoRow>> {
    let model = find(&state, id).await?;
    if model.status == Status::Ready.name() || model.status == Status::Failed.name() {
        return Ok(Json(row_of(model)));
    }

    let (_, config) = stored(&state).await?;
    match config.facts(&model.host_id).await {
        Ok(facts) => Ok(Json(record(&state, model, facts).await?)),
        // The host being unreachable is not a reason to fail the page that
        // lists what somebody uploaded.
        Err(_) => Ok(Json(row_of(model))),
    }
}

/// An address that plays, good for a few hours.
///
/// For the panel and for an assistant previewing what it uploaded. **Not** for
/// a student: it signs for whoever asked, having checked only that they are
/// signed in. The member area's own endpoint decides enrolment first.
#[utoipa::path(
    post,
    path = "/videos/{id}/playback",
    tag = "videos",
    params(("id" = String, Path, description = "Video id")),
    responses((status = 200, description = "Where to play it", body = Playback))
)]
pub async fn playback(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<Json<Playback>> {
    let model = find(&state, id).await?;
    if model.status != Status::Ready.name() {
        return Err(AppError::Validation(format!(
            "that video is {}, not ready",
            model.status
        )));
    }

    let (_, config) = stored(&state).await?;
    Ok(Json(
        config
            .playback(&model.host_id, &model.thumbnail_url)
            .await?,
    ))
}

/// Remove it, here and on the host.
///
/// Administrator, unlike adding one. This is the endpoint with no way back:
/// the video goes from the host too, and there is no bin to take it out of.
#[utoipa::path(
    delete,
    path = "/videos/{id}",
    tag = "videos",
    params(("id" = String, Path, description = "Video id")),
    responses((status = 204, description = "Gone"))
)]
pub async fn delete_video(
    Site(state): Site,
    _admin: Administrator,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let model = find(&state, id).await?;
    let (_, config) = stored(&state).await?;

    // The host first: a row deleted before the video would leave something
    // paid for every month that nothing on this site can name.
    config.remove(&model.host_id).await;
    video_asset::Entity::delete_by_id(model.id)
        .exec(state.db())
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ----------------------------------------------------------------- webhook

/// The host, saying a video finished.
///
/// Answers to anybody who has the address, which is why the address holds an
/// unguessable token — the same shape the mail plugin's event address has, and
/// for the same reason: the sender cannot hold a credential of ours.
///
/// What it can do with it is bounded on purpose. It names a video by the id
/// the host gave it, and the only thing that happens is that this server asks
/// the host what that video's state is. Nothing in the body is believed.
#[utoipa::path(
    post,
    path = "/videos/events/{token}",
    tag = "videos",
    params(("token" = String, Path, description = "The address's own token")),
    responses((status = 204, description = "Noted")),
    security(())
)]
pub async fn receive_event(
    Site(state): Site,
    Path(token): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<StatusCode> {
    let (_, config) = stored(&state).await?;
    // Whether the plugin is switched on is not asked. The token is the whole
    // credential, and a site that turns video off while three lessons are
    // still transcoding should not silently lose the notice that they
    // finished — they are uploaded already, and being paid for already.
    if config.events_token.is_empty() || config.events_token != token {
        // Not "wrong token": whoever is guessing learns nothing from a 404.
        return Err(AppError::NotFound("that address".to_string()));
    }

    // Bunny sends VideoGuid, Cloudflare sends uid. Neither is trusted for
    // anything but naming a row.
    let host_id = ["VideoGuid", "videoGuid", "uid", "id"]
        .iter()
        .find_map(|key| body.get(*key).and_then(serde_json::Value::as_str))
        .unwrap_or_default()
        .to_string();

    if host_id.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }

    let Some(model) = video_asset::Entity::find()
        .filter(video_asset::Column::HostId.eq(&host_id))
        .one(state.db())
        .await?
    else {
        return Ok(StatusCode::NO_CONTENT);
    };

    if let Ok(facts) = config.facts(&host_id).await {
        record(&state, model, facts).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}
