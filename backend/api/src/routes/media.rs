use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryOrder};
use uuid::Uuid;

use crate::{
    dto::media::{ImportMediaRequest, MediaResponse},
    entities::media,
    error::{AppError, AppResult},
    fetch::{FetchError, fetch_remote_file},
    plugins::{active_storage, storage_for},
    state::AppState,
};

pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// Identifies the image from its magic bytes, returning `(mime, extension)`.
/// The client-supplied content type is not trusted: these files are later
/// served from the same origin as the app, so a mislabelled upload would be a
/// stored-XSS vector. SVG is deliberately unsupported for the same reason —
/// it is a script-capable document, not an inert raster image.
fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

/// Upload an image. Accepts a `multipart/form-data` body with a single
/// `file` field; only common image types are accepted.
#[utoipa::path(
    post,
    path = "/media",
    tag = "media",
    responses(
        (status = 201, description = "Media uploaded", body = MediaResponse),
        (status = 400, description = "Invalid or unsupported file", body = crate::error::ErrorBody),
    )
)]
pub async fn upload_media(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<MediaResponse>)> {
    let mut file_bytes = None;
    let mut original_filename = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::Validation(format!("invalid upload: {err}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        original_filename = field.file_name().unwrap_or("upload").to_string();
        file_bytes = Some(
            field
                .bytes()
                .await
                .map_err(|err| AppError::Validation(format!("invalid upload: {err}")))?,
        );
    }

    let bytes = file_bytes.ok_or_else(|| AppError::Validation("no file provided".to_string()))?;
    let saved = store_image(&state, &bytes, original_filename, String::new()).await?;
    Ok((StatusCode::CREATED, Json(saved.into())))
}

/// Validates, stores and records an image. Shared by uploading and importing.
async fn store_image(
    state: &AppState,
    bytes: &[u8],
    filename: String,
    alt_text: String,
) -> AppResult<media::Model> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::Validation(
            "file is too large (max 10MB)".to_string(),
        ));
    }
    let (mime_type, extension) = sniff_image(bytes).ok_or_else(|| {
        AppError::Validation("unsupported file type (PNG, JPEG, GIF or WebP only)".to_string())
    })?;

    let now = Utc::now();
    let id = Uuid::new_v4();
    let key = format!("{}/{}/{id}.{extension}", now.format("%Y"), now.format("%m"));

    let storage = active_storage(state).await?;
    let url = storage.put(&key, bytes, mime_type).await?;

    let model = media::ActiveModel {
        id: Set(id),
        filename: Set(filename),
        url_path: Set(url),
        mime_type: Set(mime_type.to_string()),
        size_bytes: Set(bytes.len() as i64),
        alt_text: Set(alt_text),
        uploaded_at: Set(now.fixed_offset()),
        storage_backend: Set(storage.name().to_string()),
        storage_key: Set(key),
    };

    Ok(model.insert(state.db()).await?)
}

/// Import an image from a public URL. Only public hosts are reachable: the
/// address is resolved and refused if it points anywhere private.
#[utoipa::path(
    post,
    path = "/media/import",
    tag = "media",
    request_body = ImportMediaRequest,
    responses(
        (status = 201, description = "Media imported", body = MediaResponse),
        (status = 400, description = "Unreachable, refused or unsupported file", body = crate::error::ErrorBody),
    )
)]
pub async fn import_media(
    State(state): State<AppState>,
    Json(payload): Json<ImportMediaRequest>,
) -> AppResult<(StatusCode, Json<MediaResponse>)> {
    let bytes = fetch_remote_file(&payload.url, MAX_UPLOAD_BYTES)
        .await
        .map_err(|err| match err {
            FetchError::Rejected(message) => AppError::Validation(message),
            other => AppError::Validation(other.to_string()),
        })?;

    let filename = payload.filename.unwrap_or_else(|| {
        payload
            .url
            .rsplit('/')
            .next()
            .and_then(|name| name.split(['?', '#']).next())
            .filter(|name| !name.is_empty())
            .unwrap_or("import")
            .to_string()
    });

    let saved = store_image(
        &state,
        &bytes,
        filename,
        payload.alt_text.unwrap_or_default(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(saved.into())))
}

/// List uploaded media, most recently uploaded first.
#[utoipa::path(
    get,
    path = "/media",
    tag = "media",
    responses((status = 200, description = "List of media", body = Vec<MediaResponse>))
)]
pub async fn list_media(State(state): State<AppState>) -> AppResult<Json<Vec<MediaResponse>>> {
    let items = media::Entity::find()
        .order_by_desc(media::Column::UploadedAt)
        .all(state.db())
        .await?;
    Ok(Json(items.into_iter().map(MediaResponse::from).collect()))
}

/// Delete a media item, removing both the database record and the file.
#[utoipa::path(
    delete,
    path = "/media/{id}",
    tag = "media",
    params(("id" = Uuid, Path, description = "Media id")),
    responses(
        (status = 204, description = "Media deleted"),
        (status = 404, description = "Media not found", body = crate::error::ErrorBody),
    )
)]
pub async fn delete_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let db = state.db();
    let existing = media::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {id}")))?;

    let storage = storage_for(&state, &existing.storage_backend).await?;
    storage.delete(&existing.storage_key).await;

    media::Entity::delete_by_id(id).exec(state.db()).await?;
    Ok(StatusCode::NO_CONTENT)
}
