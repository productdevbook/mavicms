//! Mailing lists, and the campaigns sent to them.
//!
//! A site keeps people on lists, writes a campaign, and this sends it through
//! whatever the SES plugin is set to. The sending itself happens in the
//! builder's loop rather than in a request: a campaign is thousands of
//! messages at a rate Amazon caps, which is not something a browser can wait
//! for and not something that should stop when a pod restarts.
//!
//! The record of what went where is `campaign_events`, and a unique index on
//! (campaign, subscriber, "sent") is what makes resuming safe. A worker that
//! comes back after a restart does not have to remember where it was — it
//! asks who has not been written to yet.
//!
//! Written from scratch. No part of it is derived from another project.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    crypto::SecretBox,
    error::{AppError, AppResult},
};

/// Statuses a campaign moves through.
pub const DRAFT: &str = "draft";
pub const SCHEDULED: &str = "scheduled";
pub const RUNNING: &str = "running";
pub const PAUSED: &str = "paused";
pub const FINISHED: &str = "finished";
pub const CANCELLED: &str = "cancelled";

/// Where somebody stands on one list.
pub const UNCONFIRMED: &str = "unconfirmed";
pub const CONFIRMED: &str = "confirmed";
pub const UNSUBSCRIBED: &str = "unsubscribed";

/// Whether a person may be written to at all.
pub const ENABLED: &str = "enabled";
pub const BLOCKED: &str = "blocked";

/// What a campaign event records.
pub const SENT: &str = "sent";
pub const FAILED: &str = "failed";
pub const OPEN: &str = "open";
pub const CLICK: &str = "click";

/// How many to send before writing progress back and looking again.
///
/// Small enough that a cancelled campaign stops promptly and a restarted
/// worker repeats little; large enough that the progress write is not most of
/// the work.
pub const BATCH: u64 = 25;

/// A link somebody can follow without an account, and could not have guessed.
///
/// The site's own key encrypts what the link is for and who it is about, and
/// AES-GCM refuses to decrypt anything that was tampered with. So there is no
/// token column to keep, nothing to leak from the database, and a link that
/// works for one person cannot be edited into one that works for another.
pub fn link_token(
    secrets: &SecretBox,
    purpose: &str,
    subscriber: uuid::Uuid,
    list: Option<uuid::Uuid>,
) -> AppResult<String> {
    let list = list.map(|id| id.to_string()).unwrap_or_default();
    let token = secrets.encrypt(&format!("{purpose}:{subscriber}:{list}"))?;
    // Base64 with + and / in it goes through a mail client and comes back
    // different. URL-safe alphabet, done by hand rather than by re-encoding.
    Ok(token.replace('+', "-").replace('/', "_"))
}

/// The other half. `None` for anything that was not made by `link_token` for
/// this purpose.
pub fn read_token(
    secrets: &SecretBox,
    purpose: &str,
    token: &str,
) -> Option<(uuid::Uuid, Option<uuid::Uuid>)> {
    let restored = token.replace('-', "+").replace('_', "/");
    let plain = secrets.decrypt(&restored).ok()?;

    let mut parts = plain.splitn(3, ':');
    if parts.next()? != purpose {
        return None;
    }
    let subscriber = uuid::Uuid::parse_str(parts.next()?).ok()?;
    let list = parts
        .next()
        .filter(|value| !value.is_empty())
        .and_then(|value| uuid::Uuid::parse_str(value).ok());

    Some((subscriber, list))
}

/// What a message can say about the person reading it.
#[derive(Debug, Clone, Default)]
pub struct Recipient {
    pub name: String,
    pub email: String,
    /// Whatever else the site knows, as a JSON object.
    pub attributes: serde_json::Value,
    pub unsubscribe_url: String,
    pub site_url: String,
}

/// Replaces `{{ name }}` and its friends.
///
/// Not a template language. A campaign is written by whoever runs the site,
/// in the panel, and the useful placeholders are a handful — adding an engine
/// with loops and conditionals would be adding a way to run code in an
/// address bar, for no request anybody has made.
pub fn fill(text: &str, to: &Recipient) -> String {
    let mut out = text.to_string();

    let simple = [
        ("name", to.name.as_str()),
        ("email", to.email.as_str()),
        ("unsubscribe_url", to.unsubscribe_url.as_str()),
        ("site_url", to.site_url.as_str()),
    ];
    for (key, value) in simple {
        for form in [format!("{{{{{key}}}}}"), format!("{{{{ {key} }}}}")] {
            out = out.replace(&form, value);
        }
    }

    // Anything else the site collected, by its own name.
    if let Some(fields) = to.attributes.as_object() {
        for (key, value) in fields {
            let shown = match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            for form in [format!("{{{{{key}}}}}"), format!("{{{{ {key} }}}}")] {
                out = out.replace(&form, &shown);
            }
        }
    }

    out
}

/// The campaign's body inside its template.
///
/// A template is the letterhead — a header, a footer, an unsubscribe line —
/// and `{{ content }}` is where the campaign goes. Without one the body is
/// the whole message.
pub fn wrap(template: Option<&str>, body: &str) -> String {
    match template {
        Some(template) if template.contains("{{ content }}") => {
            template.replace("{{ content }}", body)
        }
        Some(template) if template.contains("{{content}}") => template.replace("{{content}}", body),
        // A template that forgot to say where the content goes would
        // otherwise send the letterhead and nothing else.
        Some(template) => format!("{template}\n{body}"),
        None => body.to_string(),
    }
}

/// A readable plain-text part from the HTML one.
///
/// Every message carries both. One with only HTML scores worse with several
/// filters that count parts, and a reader whose client shows text gets a wall
/// of markup instead of a letter.
/// True when `haystack` begins with `needle`, ignoring case.
///
/// Tag names are ASCII, so this compares ASCII-insensitively and never builds
/// a second string. Lowercasing the whole document and indexing into it — the
/// obvious way to write this — is wrong in a way that only shows up on some
/// alphabets: Turkish "İ" lowercases to two characters, every byte index
/// after it shifts, and the next slice lands inside a character. That
/// panicked the worker on the first campaign written in Turkish.
fn starts_ci(haystack: &str, needle: &str) -> bool {
    haystack.len() >= needle.len()
        && haystack.is_char_boundary(needle.len())
        && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

/// Where `needle` next appears in `haystack`, ignoring case.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .char_indices()
        .find(|(at, _)| starts_ci(&haystack[*at..], needle))
        .map(|(at, _)| at)
}

pub fn as_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut index = 0;

    while index < html.len() {
        let rest = &html[index..];

        // <script> and <style> hold text nobody wants to read.
        let skip = ["script", "style"]
            .into_iter()
            .find(|tag| starts_ci(rest, &format!("<{tag}")));
        if let Some(tag) = skip {
            let close = format!("</{tag}>");
            match find_ci(rest, &close) {
                Some(at) => {
                    index += at + close.len();
                    continue;
                }
                None => break,
            }
        }

        let character = rest.chars().next().unwrap_or(' ');
        match character {
            '<' => {
                in_tag = true;
                // A block that ended is a line that ended.
                if ["</p", "<br", "</div", "</h", "</li"]
                    .into_iter()
                    .any(|tag| starts_ci(rest, tag))
                {
                    out.push('\n');
                }
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
        index += character.len_utf8();
    }

    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    // Collapse the blank lines that stripping tags leaves behind.
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() && lines.last().is_some_and(|last: &&str| last.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// Rewrites the links in a message so a click is counted.
///
/// The destination is carried in a token this site signed, not in a query
/// parameter: a redirect that forwards to whatever it is handed is an open
/// redirect, and an open redirect on a domain people trust is what makes a
/// phishing link work.
pub fn track_clicks(
    html: &str,
    secrets: &SecretBox,
    base: &str,
    campaign: uuid::Uuid,
    subscriber: uuid::Uuid,
) -> AppResult<String> {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(at) = rest.find("href=\"") {
        let (before, after) = rest.split_at(at + 6);
        out.push_str(before);

        let Some(end) = after.find('"') else {
            out.push_str(after);
            return Ok(out);
        };
        let (url, remainder) = after.split_at(end);

        // Only real destinations. An anchor, a mailto: or the unsubscribe
        // link itself is not something to count or to send through a redirect.
        if url.starts_with("http://") || url.starts_with("https://") {
            let signed = secrets.encrypt(&format!("click:{campaign}:{subscriber}:{url}"))?;
            out.push_str(&format!(
                "{base}/mail/click/{}",
                signed.replace('+', "-").replace('/', "_")
            ));
        } else {
            out.push_str(url);
        }
        rest = remainder;
    }

    out.push_str(rest);
    Ok(out)
}

/// What a click token was hiding.
pub fn read_click(secrets: &SecretBox, token: &str) -> Option<(uuid::Uuid, uuid::Uuid, String)> {
    let restored = token.replace('-', "+").replace('_', "/");
    let plain = secrets.decrypt(&restored).ok()?;

    let mut parts = plain.splitn(4, ':');
    if parts.next()? != "click" {
        return None;
    }
    let campaign = uuid::Uuid::parse_str(parts.next()?).ok()?;
    let subscriber = uuid::Uuid::parse_str(parts.next()?).ok()?;
    let url = parts.next()?.to_string();

    (url.starts_with("http://") || url.starts_with("https://"))
        .then_some((campaign, subscriber, url))
}

/// A one-pixel transparent GIF, which is how an open is counted.
pub const PIXEL: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xff, 0xff, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];

/// The message a list sends to ask somebody to confirm.
pub fn confirmation(list_name: &str, url: &str) -> (String, String) {
    (
        format!("Please confirm: {list_name}"),
        format!(
            "<p>Somebody — we hope you — asked to join <strong>{list_name}</strong>.</p>\
             <p><a href=\"{url}\">Yes, add me</a></p>\
             <p>If it was not you, do nothing at all. Nothing will be sent.</p>"
        ),
    )
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SubscribeRequest {
    pub email: String,
    #[serde(default)]
    pub name: String,
    /// Which lists to join, by their address.
    pub lists: Vec<String>,
    /// Anything else the form collected.
    #[serde(default)]
    pub attributes: serde_json::Value,
}

/// Checks an address the same way the mail plugin does, so the two cannot
/// disagree about what is storable.
pub fn clean_email(value: &str) -> AppResult<String> {
    let value = value.trim().to_lowercase();
    if !crate::email::looks_like_an_address(&value) {
        return Err(AppError::Validation(
            "that is not an email address".to_string(),
        ));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// The control-plane side: which sites have a campaign that wants sending.
//
// Campaigns live in a site's own schema, so nothing can ask "which campaign
// anywhere is running". This table is that question, in the same shape as the
// build queue beside it — a row a worker claims with a conditional update, so
// running two workers is a matter of raising a replica count.
// ---------------------------------------------------------------------------

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};

pub const RUN_QUEUED: &str = "queued";
pub const RUN_RUNNING: &str = "running";
pub const RUN_DONE: &str = "done";

fn parameter(backend: DatabaseBackend, position: u8) -> String {
    match backend {
        DatabaseBackend::Postgres => format!("${position}"),
        _ => "?".to_string(),
    }
}

pub async fn create_tables(db: &DatabaseConnection) -> AppResult<()> {
    for statement in [
        "CREATE TABLE IF NOT EXISTS mail_runs (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            campaign_id TEXT NOT NULL,
            status TEXT NOT NULL,
            requested_at TEXT NOT NULL,
            started_at TEXT NULL,
            finished_at TEXT NULL
        )",
        "CREATE INDEX IF NOT EXISTS idx_mail_runs_status ON mail_runs (status, requested_at)",
    ] {
        db.execute_raw(Statement::from_string(db.get_database_backend(), statement))
            .await?;
    }
    Ok(())
}

/// Says a campaign wants sending. Idempotent: a campaign already waiting or
/// already going is not queued twice, so pressing send twice sends once.
pub async fn enqueue(
    db: &DatabaseConnection,
    tenant_id: uuid::Uuid,
    campaign_id: uuid::Uuid,
) -> AppResult<()> {
    let backend = db.get_database_backend();

    let existing = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id FROM mail_runs WHERE campaign_id = {} \
                 AND status IN ('{RUN_QUEUED}', '{RUN_RUNNING}')",
                parameter(backend, 1)
            ),
            [campaign_id.to_string().into()],
        ))
        .await?;
    if existing.is_some() {
        return Ok(());
    }

    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "INSERT INTO mail_runs (id, tenant_id, campaign_id, status, requested_at) \
             VALUES ({}, {}, {}, '{RUN_QUEUED}', {})",
            parameter(backend, 1),
            parameter(backend, 2),
            parameter(backend, 3),
            parameter(backend, 4)
        ),
        [
            uuid::Uuid::now_v7().to_string().into(),
            tenant_id.to_string().into(),
            campaign_id.to_string().into(),
            chrono::Utc::now().to_rfc3339().into(),
        ],
    ))
    .await?;

    Ok(())
}

/// How long a run may sit claimed before another worker may take it.
///
/// A worker that dies mid-campaign leaves its row saying "running" and
/// nothing else will touch it, so the campaign stops for ever with no error
/// anywhere. Retrying is safe because sending is: the unique index on
/// (campaign, subscriber, sent) is what stops anybody being written to twice.
/// Long enough that a slow campaign is not stolen from a worker still on it.
const STALE_RUN_MINUTES: i64 = 30;

/// Takes one run if there is one. `(run id, tenant, campaign)`.
pub async fn claim_run(
    db: &DatabaseConnection,
) -> AppResult<Option<(String, uuid::Uuid, uuid::Uuid)>> {
    let backend = db.get_database_backend();
    let stale = (chrono::Utc::now() - chrono::Duration::minutes(STALE_RUN_MINUTES)).to_rfc3339();

    let Some(row) = db
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "SELECT id, tenant_id, campaign_id FROM mail_runs \
                 WHERE status = '{RUN_QUEUED}' \
                 OR (status = '{RUN_RUNNING}' AND started_at < {}) \
                 ORDER BY requested_at LIMIT 1",
                parameter(backend, 1)
            ),
            [stale.clone().into()],
        ))
        .await?
    else {
        return Ok(None);
    };

    let id: String = row.try_get("", "id")?;
    let parse = |value: String| {
        uuid::Uuid::parse_str(&value)
            .map_err(|err| AppError::Internal(format!("bad id in the mail queue: {err}")))
    };
    let tenant_id = parse(row.try_get::<String>("", "tenant_id")?)?;
    let campaign_id = parse(row.try_get::<String>("", "campaign_id")?)?;

    // Conditional, so two workers reaching for the same row leaves one of
    // them with nothing rather than both sending it. The second clause is
    // what lets an abandoned run be taken over, and only an abandoned one.
    let taken = db
        .execute_raw(Statement::from_sql_and_values(
            backend,
            format!(
                "UPDATE mail_runs SET status = '{RUN_RUNNING}', started_at = {} \
                 WHERE id = {} AND (status = '{RUN_QUEUED}' \
                 OR (status = '{RUN_RUNNING}' AND started_at < {}))",
                parameter(backend, 1),
                parameter(backend, 2),
                parameter(backend, 3)
            ),
            [
                chrono::Utc::now().to_rfc3339().into(),
                id.clone().into(),
                stale.into(),
            ],
        ))
        .await?;

    if taken.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some((id, tenant_id, campaign_id)))
}

pub async fn finish_run(db: &DatabaseConnection, id: &str) -> AppResult<()> {
    let backend = db.get_database_backend();
    db.execute_raw(Statement::from_sql_and_values(
        backend,
        format!(
            "UPDATE mail_runs SET status = '{RUN_DONE}', finished_at = {} WHERE id = {}",
            parameter(backend, 1),
            parameter(backend, 2)
        ),
        [chrono::Utc::now().to_rfc3339().into(), id.into()],
    ))
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sending. Runs in the worker, against one site's schema at a time.
// ---------------------------------------------------------------------------

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use crate::entities::{
    campaign, campaign_event, campaign_list, mail_template, subscriber, subscriber_list,
};

/// Everybody a campaign is meant to reach who has not been reached yet.
///
/// Confirmed members of its lists, minus anybody blocked, minus anybody
/// already written to. Asking rather than remembering is what makes a worker
/// that died mid-campaign safe to restart.
async fn remaining(
    db: &DatabaseConnection,
    campaign_id: uuid::Uuid,
    limit: u64,
) -> AppResult<Vec<subscriber::Model>> {
    use sea_orm::QuerySelect;

    let lists: Vec<uuid::Uuid> = campaign_list::Entity::find()
        .filter(campaign_list::Column::CampaignId.eq(campaign_id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.list_id)
        .collect();

    if lists.is_empty() {
        return Ok(Vec::new());
    }

    let members: Vec<uuid::Uuid> = subscriber_list::Entity::find()
        .filter(subscriber_list::Column::ListId.is_in(lists))
        .filter(subscriber_list::Column::Status.eq(CONFIRMED))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.subscriber_id)
        .collect();

    if members.is_empty() {
        return Ok(Vec::new());
    }

    let done: std::collections::HashSet<uuid::Uuid> = campaign_event::Entity::find()
        .filter(campaign_event::Column::CampaignId.eq(campaign_id))
        .filter(campaign_event::Column::Kind.is_in([SENT, FAILED]))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.subscriber_id)
        .collect();

    let waiting: Vec<uuid::Uuid> = members
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|id| !done.contains(id))
        .collect();

    if waiting.is_empty() {
        return Ok(Vec::new());
    }

    Ok(subscriber::Entity::find()
        .filter(subscriber::Column::Id.is_in(waiting))
        .filter(subscriber::Column::Status.eq(ENABLED))
        .limit(limit)
        .all(db)
        .await?)
}

/// How many a campaign is meant to reach in total.
pub async fn audience(db: &DatabaseConnection, campaign_id: uuid::Uuid) -> AppResult<u64> {
    let lists: Vec<uuid::Uuid> = campaign_list::Entity::find()
        .filter(campaign_list::Column::CampaignId.eq(campaign_id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.list_id)
        .collect();

    if lists.is_empty() {
        return Ok(0);
    }

    let members: std::collections::BTreeSet<uuid::Uuid> = subscriber_list::Entity::find()
        .filter(subscriber_list::Column::ListId.is_in(lists))
        .filter(subscriber_list::Column::Status.eq(CONFIRMED))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.subscriber_id)
        .collect();

    Ok(members.len() as u64)
}

/// What one message looks like for one person.
pub struct Rendered {
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[allow(clippy::too_many_arguments)]
pub fn render_for(
    secrets: &SecretBox,
    api_base: &str,
    site_url: &str,
    campaign: &campaign::Model,
    template: Option<&mail_template::Model>,
    person: &subscriber::Model,
) -> AppResult<Rendered> {
    let unsubscribe = format!(
        "{api_base}/mail/unsubscribe/{}",
        link_token(secrets, "unsubscribe", person.id, None)?
    );

    let to = Recipient {
        name: person.name.clone(),
        email: person.email.clone(),
        attributes: serde_json::from_str(&person.attributes).unwrap_or(serde_json::Value::Null),
        unsubscribe_url: unsubscribe,
        site_url: site_url.to_string(),
    };

    let body = wrap(template.map(|t| t.body.as_str()), &campaign.body);
    let html = fill(&body, &to);
    // Clicks are rewritten after the placeholders, so a link that came from a
    // subscriber's own field is counted like any other.
    let html = track_clicks(&html, secrets, api_base, campaign.id, person.id)?;

    let pixel = format!(
        "<img src=\"{api_base}/mail/open/{}\" width=\"1\" height=\"1\" alt=\"\" style=\"display:none\">",
        link_token(secrets, &format!("open:{}", campaign.id), person.id, None)?
    );

    Ok(Rendered {
        subject: fill(&campaign.subject, &to),
        text: as_text(&html),
        html: format!("{html}{pixel}"),
    })
}

/// Writes down that a message went, or did not.
///
/// The insert is what stops a resumed campaign writing to somebody twice, so
/// a duplicate is not an error here — it means another worker got there
/// first, and the right response is to leave it be.
async fn record(
    db: &DatabaseConnection,
    campaign_id: uuid::Uuid,
    subscriber_id: uuid::Uuid,
    kind: &str,
    detail: &str,
) -> AppResult<bool> {
    let written = campaign_event::ActiveModel {
        id: Set(uuid::Uuid::now_v7()),
        campaign_id: Set(campaign_id),
        subscriber_id: Set(subscriber_id),
        kind: Set(kind.to_string()),
        detail: Set(detail.chars().take(2000).collect()),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    }
    .insert(db)
    .await;

    match written {
        Ok(_) => Ok(true),
        Err(err) if is_duplicate(&err) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

fn is_duplicate(err: &sea_orm::DbErr) -> bool {
    use sea_orm::{DbErr, RuntimeErr};

    matches!(
        err,
        DbErr::Exec(RuntimeErr::SqlxError(sqlx)) | DbErr::Query(RuntimeErr::SqlxError(sqlx))
            if sqlx.as_database_error().is_some_and(|db| db.is_unique_violation())
    )
}

/// Sends one batch and says how many are left to do.
///
/// One batch at a time so a cancelled campaign stops promptly and a worker
/// that dies repeats at most this many.
pub async fn send_batch(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    settings: &crate::email::EmailConfig,
    api_base: &str,
    site_url: &str,
    campaign_id: uuid::Uuid,
) -> AppResult<u64> {
    let Some(found) = campaign::Entity::find_by_id(campaign_id).one(db).await? else {
        return Ok(0);
    };
    if found.status != RUNNING {
        return Ok(0);
    }

    let template = match found.template_id {
        Some(id) => mail_template::Entity::find_by_id(id).one(db).await?,
        None => None,
    };

    let people = remaining(db, campaign_id, BATCH).await?;
    if people.is_empty() {
        let mut done: campaign::ActiveModel = found.into();
        done.status = Set(FINISHED.to_string());
        done.finished_at = Set(Some(chrono::Utc::now().fixed_offset()));
        done.update(db).await?;
        return Ok(0);
    }

    let mut sent = 0;
    let mut failed = 0;

    for person in &people {
        let message = render_for(
            secrets,
            api_base,
            site_url,
            &found,
            template.as_ref(),
            person,
        )?;

        let outcome = crate::email::send(
            settings,
            crate::email::Message {
                to: &person.email,
                subject: &message.subject,
                text: &message.text,
                html: Some(&message.html),
            },
        )
        .await;

        match &outcome {
            Ok(()) => {
                if record(db, campaign_id, person.id, SENT, "").await? {
                    sent += 1;
                }
            }
            Err(err) => {
                if record(db, campaign_id, person.id, FAILED, &err.to_string()).await? {
                    failed += 1;
                }
            }
        }

        log_message(db, &person.email, &message.subject, &outcome).await;
    }

    // Counted from the events rather than added up here, so a worker that
    // died halfway through a batch does not leave the number wrong for ever.
    let counted = campaign_event::Entity::find()
        .filter(campaign_event::Column::CampaignId.eq(campaign_id))
        .all(db)
        .await?;
    let mut progress: campaign::ActiveModel = campaign::Entity::find_by_id(campaign_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("campaign".to_string()))?
        .into();
    progress.sent = Set(counted.iter().filter(|e| e.kind == SENT).count() as i32);
    progress.failed = Set(counted.iter().filter(|e| e.kind == FAILED).count() as i32);
    progress.update(db).await?;

    tracing::info!(campaign = %campaign_id, sent, failed, "sent a batch");
    Ok(people.len() as u64)
}

/// Every message this site tried to send, whatever asked for it.
pub async fn log_message(
    db: &DatabaseConnection,
    to: &str,
    subject: &str,
    outcome: &AppResult<()>,
) {
    let written = crate::entities::email_log::ActiveModel {
        id: Set(uuid::Uuid::now_v7()),
        to_address: Set(to.chars().take(320).collect()),
        subject: Set(subject.chars().take(500).collect()),
        status: Set(match outcome {
            Ok(()) => SENT.to_string(),
            Err(_) => FAILED.to_string(),
        }),
        detail: Set(match outcome {
            Ok(()) => String::new(),
            Err(err) => err.to_string().chars().take(2000).collect(),
        }),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    }
    .insert(db)
    .await;

    // The log is a convenience, not the message. Failing to write it must not
    // turn a message that went out into one that looks like it did not.
    if let Err(err) = written {
        tracing::warn!(error = %err, "could not write the mail log");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> SecretBox {
        let dir = std::env::temp_dir().join(format!("mavicms-mailing-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        SecretBox::load_or_create(&dir).unwrap()
    }

    #[test]
    fn a_link_token_survives_a_round_trip() {
        let secrets = secrets();
        let who = uuid::Uuid::now_v7();
        let list = uuid::Uuid::now_v7();

        let token = link_token(&secrets, "unsubscribe", who, Some(list)).unwrap();
        assert_eq!(
            read_token(&secrets, "unsubscribe", &token),
            Some((who, Some(list)))
        );
    }

    #[test]
    fn a_token_for_one_purpose_does_not_work_for_another() {
        let secrets = secrets();
        let token = link_token(&secrets, "confirm", uuid::Uuid::now_v7(), None).unwrap();

        assert!(read_token(&secrets, "unsubscribe", &token).is_none());
    }

    #[test]
    fn an_edited_token_is_refused() {
        let secrets = secrets();
        let who = uuid::Uuid::now_v7();
        let token = link_token(&secrets, "unsubscribe", who, None).unwrap();

        let mangled = format!("{}A", &token[..token.len() - 1]);
        assert!(read_token(&secrets, "unsubscribe", &mangled).is_none());
    }

    #[test]
    fn a_token_carries_nothing_readable() {
        let secrets = secrets();
        let who = uuid::Uuid::now_v7();
        let token = link_token(&secrets, "unsubscribe", who, None).unwrap();

        assert!(!token.contains(&who.to_string()));
        assert!(!token.contains("unsubscribe"));
    }

    #[test]
    fn a_url_safe_token_has_nothing_a_mail_client_will_rewrite() {
        let secrets = secrets();
        for _ in 0..40 {
            let token = link_token(&secrets, "unsubscribe", uuid::Uuid::now_v7(), None).unwrap();
            assert!(!token.contains('+') && !token.contains('/'), "{token}");
        }
    }

    #[test]
    fn placeholders_are_filled_in_both_spellings() {
        let to = Recipient {
            name: "Ada".to_string(),
            email: "ada@example.com".to_string(),
            unsubscribe_url: "https://example.com/u/1".to_string(),
            ..Default::default()
        };

        assert_eq!(fill("Hello {{ name }}", &to), "Hello Ada");
        assert_eq!(fill("Hello {{name}}", &to), "Hello Ada");
        assert_eq!(fill("<{{ email }}>", &to), "<ada@example.com>");
    }

    #[test]
    fn a_sites_own_fields_are_placeholders_too() {
        let to = Recipient {
            attributes: serde_json::json!({ "town": "İzmir", "orders": 3 }),
            ..Default::default()
        };

        assert_eq!(fill("{{ town }} / {{ orders }}", &to), "İzmir / 3");
    }

    #[test]
    fn an_unknown_placeholder_is_left_alone_rather_than_emptied() {
        // Better a visible {{ oops }} in a test send than a sentence with a
        // hole in it going to four thousand people.
        let to = Recipient::default();
        assert_eq!(fill("a {{ oops }} b", &to), "a {{ oops }} b");
    }

    #[test]
    fn a_template_wraps_the_body() {
        assert_eq!(
            wrap(Some("<h1>Hi</h1>{{ content }}<i>bye</i>"), "BODY"),
            "<h1>Hi</h1>BODY<i>bye</i>"
        );
        assert_eq!(wrap(None, "BODY"), "BODY");
    }

    #[test]
    fn a_template_that_forgot_the_content_still_sends_it() {
        assert_eq!(wrap(Some("<h1>Hi</h1>"), "BODY"), "<h1>Hi</h1>\nBODY");
    }

    #[test]
    fn text_is_readable_and_carries_no_markup() {
        let html = "<h1>Merhaba</h1><p>Bir <b>iki</b> üç</p><script>alert(1)</script><p>Son</p>";
        let text = as_text(html);

        assert!(!text.contains('<'));
        assert!(!text.contains("alert"));
        assert!(text.contains("Merhaba"));
        assert!(text.contains("Bir iki üç"));
        assert!(text.contains("Son"));
    }

    #[test]
    fn a_letter_written_in_turkish_does_not_crash_the_worker() {
        // "İ" lowercases to two characters. Indexing a lowercased copy with
        // offsets from the original landed inside one of them, and the panic
        // took the worker down mid-campaign.
        let html = "<p>İzmir'den İyi Günler</p><p>ŞŞŞ İİİ ĞĞĞ</p><div>Son</div>";
        let text = as_text(html);

        assert!(text.contains("İzmir'den İyi Günler"));
        assert!(text.contains("Son"));
    }

    #[test]
    fn a_script_is_skipped_whatever_its_case_and_wherever_it_sits() {
        let html = "<p>İyi</p><SCRIPT>alert(1)</SCRIPT><p>Günler</p>";
        let text = as_text(html);

        assert!(!text.contains("alert"));
        assert!(text.contains("İyi"));
        assert!(text.contains("Günler"));
    }

    #[test]
    fn links_are_rewritten_and_the_destination_is_not_in_the_url() {
        let secrets = secrets();
        let campaign = uuid::Uuid::now_v7();
        let who = uuid::Uuid::now_v7();

        let html = r##"<a href="https://example.com/post">read</a> <a href="#top">top</a>"##;
        let out = track_clicks(html, &secrets, "https://site.test/api", campaign, who).unwrap();

        assert!(out.contains("https://site.test/api/mail/click/"));
        // An open redirect is what this exists to avoid.
        assert!(!out.contains("href=\"https://example.com/post\""));
        assert!(!out.contains("=https://example.com"));
        // Anchors are left as they were.
        assert!(out.contains("href=\"#top\""));
    }

    #[test]
    fn a_click_token_gives_back_exactly_what_was_signed() {
        let secrets = secrets();
        let campaign = uuid::Uuid::now_v7();
        let who = uuid::Uuid::now_v7();

        let html = r#"<a href="https://example.com/a?b=c&d=e">x</a>"#;
        let out = track_clicks(html, &secrets, "https://s/api", campaign, who).unwrap();
        let token = out
            .split("/mail/click/")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();

        assert_eq!(
            read_click(&secrets, token),
            Some((campaign, who, "https://example.com/a?b=c&d=e".to_string()))
        );
    }

    #[test]
    fn a_click_token_cannot_be_made_to_point_somewhere_else() {
        let secrets = secrets();
        // Anything not signed by this site is refused, so a redirect cannot be
        // handed a destination by whoever follows the link.
        assert!(read_click(&secrets, "aGVsbG8gdGhlcmU").is_none());

        let elsewhere = secrets.encrypt("click:x:y:https://evil.test").unwrap();
        assert!(read_click(&secrets, &elsewhere).is_none());
    }

    #[test]
    fn the_pixel_is_a_gif() {
        assert_eq!(&PIXEL[..6], b"GIF89a");
    }
}
