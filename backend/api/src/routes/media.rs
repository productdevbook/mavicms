use axum::{
    Json,
    extract::{Multipart, Path},
    http::StatusCode,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::{
    dto::media::{ImportMediaRequest, MediaResponse},
    entities::{media, post},
    error::{AppError, AppResult},
    fetch::{FetchError, fetch_remote_file},
    plugins::{active_storage, storage_for},
    state::AppState,
    tenants::Site,
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
    } else if is_isobmff(bytes, b"avif") || is_isobmff(bytes, b"avis") {
        Some(("image/avif", "avif"))
    } else {
        None
    }
}

/// Whether the file is an ISO base media container with the given brand.
///
/// AVIF declares itself in the `ftyp` box rather than at offset zero, either
/// as the major brand or somewhere in the compatible-brand list that follows.
fn is_isobmff(bytes: &[u8], brand: &[u8; 4]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let box_size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let end = box_size.clamp(16, bytes.len());
    bytes[8..end].chunks_exact(4).any(|chunk| chunk == brand)
}

/// Names the file type for an error message, so a rejection says what arrived
/// instead of only listing what was wanted.
fn describe(bytes: &[u8]) -> &'static str {
    let head = &bytes[..bytes.len().min(512)];
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start();

    if trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && text.contains("<svg")) {
        // Rejected on purpose: an SVG can carry script and would run on this
        // site's own origin.
        "an SVG, which cannot be served safely"
    } else if bytes.starts_with(b"%PDF") {
        "a PDF"
    } else if trimmed.starts_with("<!DOCTYPE html") || trimmed.starts_with("<html") {
        // Usually a login wall or an error page returned with status 200.
        "a web page, not an image"
    } else if is_isobmff(bytes, b"heic") || is_isobmff(bytes, b"heix") || is_isobmff(bytes, b"mif1")
    {
        "a HEIC image, which most browsers cannot display"
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        "a TIFF image, which most browsers cannot display"
    } else if bytes.starts_with(b"BM") {
        "a BMP image"
    } else if bytes.starts_with(b"PK\x03\x04") {
        "a zip archive"
    } else {
        "not a recognised image"
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
    Site(state): Site,
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

/// Validates, stores and records an image. Shared by uploading, importing,
/// and an assistant sending one over MCP.
pub(crate) async fn store_image(
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
        AppError::Validation(format!(
            "{} is {} (PNG, JPEG, GIF, WebP and AVIF are supported)",
            if filename.is_empty() {
                "the file"
            } else {
                &filename
            },
            describe(bytes)
        ))
    })?;

    let now = Utc::now();
    let id = Uuid::now_v7();
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
    Site(state): Site,
    Json(payload): Json<ImportMediaRequest>,
) -> AppResult<(StatusCode, Json<MediaResponse>)> {
    let saved = fetch_and_store(&state, &payload.url, payload.filename, payload.alt_text).await?;
    Ok((StatusCode::CREATED, Json(saved.into())))
}

/// Fetches a public address and stores what comes back as an image.
///
/// The name is worked out from the address when nobody gave one, which is
/// almost always: an assistant has a link and no opinion about filenames.
pub(crate) async fn fetch_and_store(
    state: &AppState,
    url: &str,
    filename: Option<String>,
    alt_text: Option<String>,
) -> AppResult<media::Model> {
    let payload = ImportMediaRequest {
        url: url.to_string(),
        filename,
        alt_text,
    };
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

    store_image(
        state,
        &bytes,
        filename,
        payload.alt_text.unwrap_or_default(),
    )
    .await
}

/// Deletes any of `candidates` that no post refers to any more.
///
/// Called after a post is removed: its pictures are usually its own, and
/// leaving them behind means paying to store files nothing will ever show
/// again. A file still used by another post — or as another post's cover — is
/// kept, so a picture shared between two posts survives the first deletion.
///
/// Best effort by design: a failure here must not turn a successful deletion
/// into an error for the caller.
pub async fn drop_unreferenced(state: &AppState, candidates: Vec<String>) {
    if candidates.is_empty() {
        return;
    }
    let db = state.db();

    let items = match media::Entity::find().all(db).await {
        Ok(items) => items,
        Err(err) => {
            tracing::warn!(error = %err, "could not list media to tidy up");
            return;
        }
    };

    // Compare file names exactly. The stored name is a uuid we generated, so
    // it identifies the file on its own whatever host or CDN prefix the
    // content carries. Matching on any looser suffix would let a post
    // containing something as small as `src=".png"` sweep up every unreferenced
    // png in the library when it was deleted.
    let wanted: std::collections::HashSet<&str> =
        candidates.iter().filter_map(|url| file_name(url)).collect();

    let orphans: Vec<_> = items
        .into_iter()
        .filter(|item| file_name(&item.url_path).is_some_and(|name| wanted.contains(name)))
        .collect();

    for item in orphans {
        match still_referenced(db, &item.url_path).await {
            Err(err) => tracing::warn!(error = %err, "could not check whether media is still used"),
            Ok(true) => {}
            Ok(false) => {
                let storage = match storage_for(state, &item.storage_backend).await {
                    Ok(storage) => storage,
                    Err(err) => {
                        tracing::warn!(error = %err, "cannot reach storage to tidy up");
                        continue;
                    }
                };
                storage.delete(&item.storage_key).await;
                if let Err(err) = media::Entity::delete_by_id(item.id).exec(db).await {
                    tracing::warn!(error = %err, "could not remove the media record");
                }
            }
        }
    }
}

/// The last path segment of an address, without any query or fragment.
fn file_name(url: &str) -> Option<&str> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or(path);
    (!name.is_empty()).then_some(name)
}

/// Whether any post still points at this file.
async fn still_referenced(db: &impl ConnectionTrait, url_path: &str) -> AppResult<bool> {
    // The file name is what both the cover and the content have in common,
    // whatever host or CDN prefix sits in front of it.
    let Some(needle) = file_name(url_path) else {
        // Nothing identifiable: keep the file rather than guess.
        return Ok(true);
    };
    // `%` and `_` are wildcards in LIKE; a name carrying one would otherwise
    // match more than itself and hide a real reference.
    let escaped = needle
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");

    Ok(post::Entity::find()
        .filter(
            Condition::any()
                .add(post::Column::ContentHtml.like(&pattern))
                .add(post::Column::CoverUrl.like(&pattern)),
        )
        .one(db)
        .await?
        .is_some())
}

/// List uploaded media, most recently uploaded first.
#[utoipa::path(
    get,
    path = "/media",
    tag = "media",
    responses((status = 200, description = "List of media", body = Vec<MediaResponse>))
)]
pub async fn list_media(Site(state): Site) -> AppResult<Json<Vec<MediaResponse>>> {
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
pub async fn delete_media(Site(state): Site, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ISO base media header: box size, `ftyp`, major brand, minor
    /// version, then the compatible brands.
    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + compatible.len() * 4;
        let mut bytes = (size as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(major);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        for brand in compatible {
            bytes.extend_from_slice(*brand);
        }
        bytes
    }

    #[test]
    fn recognises_raster_formats() {
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\n").unwrap().1, "png");
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap().1, "jpg");
        assert_eq!(sniff_image(b"GIF89a....").unwrap().1, "gif");
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 ").unwrap().1, "webp");
    }

    #[test]
    fn recognises_avif_as_major_and_as_compatible_brand() {
        assert_eq!(
            sniff_image(&ftyp(b"avif", &[b"mif1"])).unwrap().0,
            "image/avif"
        );
        // Encoders often declare mif1 as the major brand and avif only in the
        // compatible list.
        assert_eq!(
            sniff_image(&ftyp(b"mif1", &[b"avif"])).unwrap().0,
            "image/avif"
        );
    }

    #[test]
    fn heic_is_not_accepted_as_avif() {
        assert!(sniff_image(&ftyp(b"heic", &[b"mif1"])).is_none());
        assert_eq!(
            describe(&ftyp(b"heic", &[b"mif1"])),
            "a HEIC image, which most browsers cannot display"
        );
    }

    #[test]
    fn svg_is_named_rather_than_accepted() {
        assert!(sniff_image(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").is_none());
        assert_eq!(
            describe(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"),
            "an SVG, which cannot be served safely"
        );
        assert_eq!(
            describe(b"<?xml version=\"1.0\"?>\n<svg width=\"1\"></svg>"),
            "an SVG, which cannot be served safely"
        );
    }

    #[test]
    fn names_other_things_that_arrive_instead_of_images() {
        assert_eq!(describe(b"%PDF-1.7"), "a PDF");
        assert_eq!(
            describe(b"<!DOCTYPE html><html>"),
            "a web page, not an image"
        );
        assert_eq!(describe(b"BM\0\0"), "a BMP image");
        assert_eq!(describe(b"\x00\x01\x02\x03"), "not a recognised image");
    }

    #[test]
    fn a_truncated_header_is_not_mistaken_for_a_container() {
        assert!(!is_isobmff(b"\0\0\0\x18ftypav", b"avif"));
        assert!(sniff_image(b"").is_none());
        assert_eq!(describe(b""), "not a recognised image");
    }
}

#[cfg(test)]
mod orphan_tests {
    use super::file_name;

    #[test]
    fn takes_the_file_name_only() {
        assert_eq!(
            file_name("https://cdn.example.com/prod/2026/08/abc.png"),
            Some("abc.png")
        );
        assert_eq!(file_name("/uploads/2026/08/abc.png?v=2"), Some("abc.png"));
        assert_eq!(file_name("abc.png#frag"), Some("abc.png"));
    }

    #[test]
    fn a_bare_extension_matches_nothing_by_itself() {
        // The whole point: ".png" must not stand in for every png there is.
        assert_ne!(file_name(".png"), file_name("/uploads/2026/08/abc.png"));
    }

    #[test]
    fn has_no_name_when_there_is_nothing_after_the_slash() {
        assert_eq!(file_name("https://example.com/"), None);
        assert_eq!(file_name(""), None);
    }
}
