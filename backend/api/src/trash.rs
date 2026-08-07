//! Deleting something puts it here first.
//!
//! An assistant is connected to this CMS and can be asked to tidy up. Tidying
//! up is deleting, and an assistant that has misread which post was meant
//! deletes the wrong one at the speed of a sentence. A person can do the same
//! with a mouse, but rather more slowly and with a dialog in the way.
//!
//! So nothing is destroyed when it is deleted. The row leaves its table whole
//! — with whatever hung off it — and is written here as JSON, where it can be
//! put back or, after long enough that nobody is coming for it, actually
//! thrown away.
//!
//! Files are the exception that shapes the rest. A deleted image's bytes stay
//! in the bucket until the entry is purged, because a restore that hands back
//! a row pointing at nothing is not a restore.

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    entities::trash,
    error::{AppError, AppResult},
};

/// What a deleted thing was.
pub const POST: &str = "post";
pub const PAGE: &str = "page";
pub const MEDIA: &str = "media";
pub const FORM: &str = "form";
pub const FORM_SUBMISSION: &str = "form_submission";

/// How long something stays before it is really gone.
///
/// Long enough to cover somebody being away for a fortnight and coming back to
/// find what an assistant did while they were out.
pub const KEEP_DAYS: i64 = 30;

/// One deleted thing, as the panel shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct Entry {
    pub id: Uuid,
    pub kind: String,
    /// The id it had, which is the id it gets back.
    pub subject_id: Uuid,
    pub title: String,
    pub deleted_at: DateTime<FixedOffset>,
    pub deleted_by: String,
    /// When this stops being recoverable.
    pub purges_at: DateTime<FixedOffset>,
}

impl From<trash::Model> for Entry {
    fn from(model: trash::Model) -> Self {
        Self {
            purges_at: model.deleted_at + chrono::Duration::days(KEEP_DAYS),
            id: model.id,
            kind: model.kind,
            subject_id: model.subject_id,
            title: model.title,
            deleted_at: model.deleted_at,
            deleted_by: model.deleted_by,
        }
    }
}

/// Puts something in the bin.
///
/// `payload` is whatever `restore` will need to write the thing back — the row
/// itself and any rows that only existed because of it.
pub async fn keep(
    db: &impl ConnectionTrait,
    kind: &str,
    subject_id: Uuid,
    title: &str,
    payload: serde_json::Value,
    by: &str,
) -> AppResult<Uuid> {
    let id = Uuid::now_v7();
    trash::ActiveModel {
        id: Set(id),
        kind: Set(kind.to_string()),
        subject_id: Set(subject_id),
        // Long titles are for reading in a list, not for keeping.
        title: Set(title.chars().take(300).collect()),
        payload: Set(payload.to_string()),
        deleted_at: Set(Utc::now().fixed_offset()),
        deleted_by: Set(by.chars().take(120).collect()),
    }
    .insert(db)
    .await?;
    Ok(id)
}

/// Everything in the bin, newest first.
pub async fn list(db: &DatabaseConnection) -> AppResult<Vec<Entry>> {
    Ok(trash::Entity::find()
        .order_by_desc(trash::Column::DeletedAt)
        .all(db)
        .await?
        .into_iter()
        .map(Entry::from)
        .collect())
}

/// One entry, with its payload, for whoever knows how to put it back.
pub async fn take(
    db: &DatabaseConnection,
    id: Uuid,
) -> AppResult<(trash::Model, serde_json::Value)> {
    let entry = trash::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("that deleted thing".to_string()))?;

    let payload: serde_json::Value = serde_json::from_str(&entry.payload)
        .map_err(|err| AppError::Internal(format!("what was kept cannot be read back: {err}")))?;

    Ok((entry, payload))
}

/// Forgets the entry. The caller has already put the thing back, or has
/// decided it is going for good.
pub async fn forget(db: &impl ConnectionTrait, id: Uuid) -> AppResult<()> {
    trash::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}

/// Entries old enough to be past saving, oldest first.
///
/// Returned rather than deleted, because some of them own bytes in a bucket
/// that have to go at the same time and only the caller can reach those.
pub async fn expired(db: &DatabaseConnection) -> AppResult<Vec<trash::Model>> {
    let cutoff = (Utc::now() - chrono::Duration::days(KEEP_DAYS)).fixed_offset();
    Ok(trash::Entity::find()
        .filter(trash::Column::DeletedAt.lt(cutoff))
        .order_by_asc(trash::Column::DeletedAt)
        .all(db)
        .await?)
}

/// Puts a deleted thing back where it came from.
///
/// The address it had may have been taken while it was gone — somebody wrote a
/// new post at the same slug. Rather than refuse, the slug gets a suffix and
/// the caller is told: getting the words back matters more than getting the
/// address back, and the address is one field to fix.
pub async fn restore(db: &DatabaseConnection, id: Uuid) -> AppResult<String> {
    let (entry, payload) = take(db, id).await?;

    let what = match entry.kind.as_str() {
        MEDIA => restore_media(db, &payload).await?,
        FORM => restore_form(db, &payload).await?,
        FORM_SUBMISSION => restore_submission(db, &payload).await?,
        // A post, a page, or a kind this site made up. They are one table and
        // one way back, so matching the two names this used to know would have
        // meant a course going into the bin and never coming out of it.
        _ => restore_post(db, &payload).await?,
    };

    forget(db, entry.id).await?;
    Ok(what)
}

async fn restore_post(db: &DatabaseConnection, payload: &serde_json::Value) -> AppResult<String> {
    use crate::entities::{post, post_category, post_tag};

    let mut model: post::Model = field(payload, "post")
        .ok_or_else(|| AppError::Internal("what was kept is not a post".to_string()))?;

    // An address that somebody else has taken in the meantime.
    let taken = post::Entity::find()
        .filter(post::Column::Kind.eq(model.kind.clone()))
        .filter(post::Column::Locale.eq(model.locale.clone()))
        .filter(post::Column::Slug.eq(model.slug.clone()))
        .one(db)
        .await?
        .is_some();
    let mut note = String::new();
    if taken {
        let freed = format!("{}-{}", model.slug, &model.id.simple().to_string()[..6]);
        note = format!(" (its address was taken, so it came back at {freed})");
        model.slug = freed;
    }

    let title = model.title.clone();
    let id = model.id;
    let active: post::ActiveModel = model.into();
    active.insert(db).await?;

    for category_id in field::<Vec<Uuid>>(payload, "categories").unwrap_or_default() {
        // A category deleted while the post was in the bin is not a reason to
        // refuse the post.
        let _ = post_category::ActiveModel {
            post_id: Set(id),
            category_id: Set(category_id),
        }
        .insert(db)
        .await;
    }
    for tag_id in field::<Vec<Uuid>>(payload, "tags").unwrap_or_default() {
        let _ = post_tag::ActiveModel {
            post_id: Set(id),
            tag_id: Set(tag_id),
        }
        .insert(db)
        .await;
    }

    Ok(format!("{title}{note}"))
}

async fn restore_media(db: &DatabaseConnection, payload: &serde_json::Value) -> AppResult<String> {
    use crate::entities::media;

    let model: media::Model = field(payload, "media")
        .ok_or_else(|| AppError::Internal("what was kept is not an image".to_string()))?;
    let filename = model.filename.clone();
    let active: media::ActiveModel = model.into();
    active.insert(db).await?;
    Ok(filename)
}

async fn restore_form(db: &DatabaseConnection, payload: &serde_json::Value) -> AppResult<String> {
    use crate::entities::{form, form_submission};

    let mut model: form::Model = field(payload, "form")
        .ok_or_else(|| AppError::Internal("what was kept is not a form".to_string()))?;

    let taken = form::Entity::find()
        .filter(form::Column::Slug.eq(model.slug.clone()))
        .one(db)
        .await?
        .is_some();
    let mut note = String::new();
    if taken {
        let freed = format!("{}-{}", model.slug, &model.id.simple().to_string()[..6]);
        note = format!(" (its address was taken, so it came back at {freed})");
        model.slug = freed;
    }

    let name = model.name.clone();
    let active: form::ActiveModel = model.into();
    active.insert(db).await?;

    let submissions: Vec<form_submission::Model> =
        field(payload, "submissions").unwrap_or_default();
    let how_many = submissions.len();
    for submission in submissions {
        let active: form_submission::ActiveModel = submission.into();
        active.insert(db).await?;
    }

    Ok(match how_many {
        0 => format!("{name}{note}"),
        n => format!("{name}, with {n} of what people sent{note}"),
    })
}

async fn restore_submission(
    db: &DatabaseConnection,
    payload: &serde_json::Value,
) -> AppResult<String> {
    use crate::entities::{form, form_submission};

    let model: form_submission::Model = field(payload, "submission")
        .ok_or_else(|| AppError::Internal("what was kept is not a submission".to_string()))?;

    // The form it belonged to may have gone in the meantime, and a submission
    // pointing at nothing is a foreign key error rather than a restore.
    if form::Entity::find_by_id(model.form_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(AppError::Validation(
            "the form this was sent to has been deleted — put the form back first".to_string(),
        ));
    }

    let described = crate::routes::forms::describe(&model);
    let active: form_submission::ActiveModel = model.into();
    active.insert(db).await?;
    Ok(described)
}

/// Tidies up after a post that is now gone for good.
///
/// Deleting a post used to take its images with it there and then, which is
/// why a restore had to be told not to. Now it happens here instead: the post
/// is past recovering, so an image nothing else points at is past use.
pub async fn drop_what_a_purged_post_used(
    state: &crate::state::AppState,
    payload: &serde_json::Value,
) {
    let Some(model) = field::<crate::entities::post::Model>(payload, "post") else {
        return;
    };
    let used = crate::routes::posts::referenced_media(&model);
    crate::routes::media::drop_unreferenced(state, used).await;
}

/// Removes the bytes a deleted image was pointing at.
///
/// Only ever called when an entry is being purged. Best effort: a file that
/// has already gone, or a bucket that has been reconfigured since, is not a
/// reason to keep an entry nobody can restore.
pub async fn drop_the_file(state: &crate::state::AppState, payload: &serde_json::Value) {
    let Some(model) = field::<crate::entities::media::Model>(payload, "media") else {
        // Said out loud rather than skipped quietly. If what was kept can no
        // longer be read — a column changed shape since — the bytes would
        // otherwise stay in the bucket for ever with nothing pointing at them
        // and nobody the wiser.
        tracing::warn!("a purged image could not be read back, so its file was left behind");
        return;
    };
    match crate::plugins::storage_for(state, &model.storage_backend).await {
        Ok(storage) => storage.delete(&model.storage_key).await,
        Err(err) => {
            tracing::warn!(error = %err, key = %model.storage_key, "could not reach storage to drop a purged file")
        }
    }
}

/// What a payload said, for a caller that only wants one field of it.
pub fn field<T: for<'a> Deserialize<'a>>(payload: &serde_json::Value, name: &str) -> Option<T> {
    payload
        .get(name)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_too_long_to_read_is_not_kept_whole() {
        let long: String = "a".repeat(1000);
        assert_eq!(long.chars().take(300).count(), 300);
    }

    #[test]
    fn what_was_kept_can_be_read_back_out() {
        let payload = serde_json::json!({ "slug": "merhaba", "tags": ["bir", "iki"] });
        assert_eq!(
            field::<String>(&payload, "slug").as_deref(),
            Some("merhaba")
        );
        assert_eq!(
            field::<Vec<String>>(&payload, "tags"),
            Some(vec!["bir".to_string(), "iki".to_string()])
        );
        assert_eq!(field::<String>(&payload, "yok"), None);
    }
}
