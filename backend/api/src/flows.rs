//! Flows: something happened, so do these things.
//!
//! A contact form that emails whoever answers it, a post going out that pings
//! a channel. The shape is deliberately small — a trigger, a list of steps,
//! and a record of every run — because the thing that makes automation useful
//! is not how many kinds of step there are but being able to answer "did it
//! work", and that is the record.
//!
//! Nothing here runs a customer's code. Every step is one of ours, doing one
//! thing with settings it was given. That is why the runner lives in this
//! process rather than in an isolated one like the builder — and it is the
//! line to watch: the day a step evaluates something somebody typed, the
//! runner has to move.

use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    crypto::SecretBox,
    entities::{flow, flow_credential, flow_run, flow_run_step, flow_step},
    error::{AppError, AppResult},
};

/// What sets a flow off.
pub mod trigger {
    pub const FORM_SUBMITTED: &str = "form.submitted";
    pub const POST_PUBLISHED: &str = "post.published";
    pub const SCHEDULE: &str = "schedule";
    pub const WEBHOOK: &str = "webhook";

    pub const ALL: [&str; 4] = [FORM_SUBMITTED, POST_PUBLISHED, SCHEDULE, WEBHOOK];
}

/// What a step does.
pub mod action {
    pub const MAIL_SEND: &str = "mail.send";
    pub const HTTP_REQUEST: &str = "http.request";
    pub const BRANCH: &str = "branch";
    // The three below are the same request with the shape filled in. A site
    // owner should not have to know what JSON Slack wants; that is exactly the
    // difference between a step and a raw call.
    pub const SLACK: &str = "slack.message";
    pub const DISCORD: &str = "discord.message";
    pub const TELEGRAM: &str = "telegram.message";

    pub const ALL: [&str; 6] = [MAIL_SEND, HTTP_REQUEST, BRANCH, SLACK, DISCORD, TELEGRAM];
}

/// What a stored credential is for.
pub mod credential {
    pub const SMTP: &str = "smtp";
    pub const TELEGRAM: &str = "telegram";

    pub const ALL: [&str; 2] = [SMTP, TELEGRAM];
}

pub const QUEUED: &str = "queued";
pub const RUNNING: &str = "running";
pub const SUCCEEDED: &str = "succeeded";
pub const FAILED: &str = "failed";
pub const SKIPPED: &str = "skipped";

/// How many flows deep a chain may go before it is treated as a loop. A flow
/// that publishes a post and a flow that runs when a post is published will
/// otherwise run each other until something else breaks.
const DEEPEST: i32 = 5;

/// How many runs one flow may start in an hour. The public sets two of the
/// triggers off — a form and a webhook — so this is the difference between an
/// automation and a way to make somebody's server send mail all afternoon.
const MOST_RUNS_AN_HOUR: u64 = 200;

/// How many queued runs one tick takes. Bounded because this shares a process
/// with the API: a hundred slow requests must not become a hundred slow
/// requests happening instead of serving anybody.
const RUNS_A_TICK: u64 = 20;

/// How long one step gets before it is abandoned.
const STEP_PATIENCE: std::time::Duration = std::time::Duration::from_secs(20);

pub fn known_trigger(kind: &str) -> AppResult<&'static str> {
    trigger::ALL
        .iter()
        .find(|known| **known == kind)
        .copied()
        .ok_or_else(|| AppError::Validation(format!("{kind} is not something a flow can wait for")))
}

pub fn known_action(kind: &str) -> AppResult<&'static str> {
    action::ALL
        .iter()
        .find(|known| **known == kind)
        .copied()
        .ok_or_else(|| AppError::Validation(format!("{kind} is not something a flow can do")))
}

/// Puts a run on the queue for every enabled flow waiting on this trigger.
///
/// Never runs anything: a visitor filling in a contact form is answered before
/// anybody's mail server is involved, because the alternative is a form that
/// takes as long as the slowest thing it sets off.
pub async fn fire(db: &DatabaseConnection, trigger_kind: &str, payload: Value) -> AppResult<usize> {
    fire_at_depth(db, trigger_kind, payload, 0).await
}

pub async fn fire_at_depth(
    db: &DatabaseConnection,
    trigger_kind: &str,
    payload: Value,
    depth: i32,
) -> AppResult<usize> {
    if depth > DEEPEST {
        tracing::warn!(trigger_kind, depth, "a flow set off a flow too many times");
        return Ok(0);
    }

    let waiting = flow::Entity::find()
        .filter(flow::Column::TriggerKind.eq(trigger_kind))
        .filter(flow::Column::Enabled.eq(true))
        .all(db)
        .await?;

    let mut started = 0;
    for found in waiting {
        if !trigger_matches(&found, &payload) {
            continue;
        }
        // The clock ticks far more often than any flow wants to run, so a
        // scheduled one asks how long it has been rather than going every
        // time the tick comes round.
        if found.trigger_kind == trigger::SCHEDULE && !due_by_the_clock(db, &found).await? {
            continue;
        }
        if too_busy(db, found.id).await? {
            tracing::warn!(flow = %found.name, "flow has run too often this hour; skipping");
            continue;
        }
        flow_run::ActiveModel {
            id: Set(Uuid::now_v7()),
            flow_id: Set(found.id),
            status: Set(QUEUED.to_string()),
            trigger: Set(payload.to_string()),
            error: Set(None),
            depth: Set(depth),
            created_at: Set(Utc::now().fixed_offset()),
            started_at: Set(None),
            finished_at: Set(None),
        }
        .insert(db)
        .await?;
        started += 1;
    }

    Ok(started)
}

/// Whether this flow's trigger settings narrow it further than its kind does.
/// A flow waiting on one form should not run for every form.
fn trigger_matches(found: &flow::Model, payload: &Value) -> bool {
    let config: Value = serde_json::from_str(&found.trigger_config).unwrap_or(Value::Null);
    match found.trigger_kind.as_str() {
        trigger::FORM_SUBMITTED => match config.get("form_id").and_then(Value::as_str) {
            // No form named means every form, which is what somebody who left
            // the box empty meant.
            None | Some("") => true,
            Some(wanted) => payload.get("form_id").and_then(Value::as_str) == Some(wanted),
        },
        trigger::POST_PUBLISHED => match config.get("kind").and_then(Value::as_str) {
            None | Some("") => true,
            Some(wanted) => payload.get("kind").and_then(Value::as_str) == Some(wanted),
        },
        _ => true,
    }
}

/// Whether enough time has passed since a scheduled flow last ran.
///
/// Minutes rather than a cron expression: "every 60 minutes" is what somebody
/// asking for an hourly digest means, and a cron parser is a dependency and a
/// class of confusion ("why did it run at 00:00 on Sunday") for a feature
/// nobody has asked for yet.
async fn due_by_the_clock(db: &DatabaseConnection, found: &flow::Model) -> AppResult<bool> {
    let config: Value = serde_json::from_str(&found.trigger_config).unwrap_or(Value::Null);
    let every = config
        .get("every_minutes")
        .and_then(Value::as_i64)
        .unwrap_or(60)
        .clamp(1, 60 * 24 * 31);

    let last = flow_run::Entity::find()
        .filter(flow_run::Column::FlowId.eq(found.id))
        .order_by_desc(flow_run::Column::CreatedAt)
        .one(db)
        .await?;

    Ok(match last {
        None => true,
        Some(previous) => {
            Utc::now().fixed_offset() - previous.created_at >= chrono::Duration::minutes(every)
        }
    })
}

async fn too_busy(db: &DatabaseConnection, flow_id: Uuid) -> AppResult<bool> {
    let since = Utc::now().fixed_offset() - chrono::Duration::hours(1);
    let recent = flow_run::Entity::find()
        .filter(flow_run::Column::FlowId.eq(flow_id))
        .filter(flow_run::Column::CreatedAt.gt(since))
        .count(db)
        .await?;
    Ok(recent >= MOST_RUNS_AN_HOUR)
}

/// Runs what is queued for one site. Called on the same tick that publishes
/// what is due and empties the bin.
pub async fn run_queued(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    site_url: &str,
) -> AppResult<usize> {
    let queued = flow_run::Entity::find()
        .filter(flow_run::Column::Status.eq(QUEUED))
        .order_by_asc(flow_run::Column::CreatedAt)
        .limit(RUNS_A_TICK)
        .all(db)
        .await?;

    let mut done = 0;
    for run in queued {
        // Marked before the work rather than after: two ticks overlapping is
        // a message sent twice, and somebody's customer reading it twice.
        let mut taken: flow_run::ActiveModel = run.clone().into();
        taken.status = Set(RUNNING.to_string());
        taken.started_at = Set(Some(Utc::now().fixed_offset()));
        taken.update(db).await?;

        let outcome = run_one(db, secrets, site_url, &run).await;

        let mut finished: flow_run::ActiveModel = flow_run::Entity::find_by_id(run.id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::Internal("the run went away mid-run".to_string()))?
            .into();
        match outcome {
            Ok(()) => {
                finished.status = Set(SUCCEEDED.to_string());
                finished.error = Set(None);
            }
            Err(err) => {
                finished.status = Set(FAILED.to_string());
                finished.error = Set(Some(err.to_string()));
            }
        }
        finished.finished_at = Set(Some(Utc::now().fixed_offset()));
        finished.update(db).await?;
        done += 1;
    }

    Ok(done)
}

async fn run_one(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    site_url: &str,
    run: &flow_run::Model,
) -> AppResult<()> {
    let steps = flow_step::Entity::find()
        .filter(flow_step::Column::FlowId.eq(run.flow_id))
        .order_by_asc(flow_step::Column::Position)
        .all(db)
        .await?;

    let trigger: Value = serde_json::from_str(&run.trigger).unwrap_or(Value::Null);
    // What the steps can read from. The trigger under "trigger", and whatever
    // each step produced under its position, so a later step can use an
    // earlier one's answer.
    let mut seen = json!({ "trigger": trigger, "site": { "url": site_url } });
    let mut skipping = false;

    for step in steps {
        if skipping {
            record(db, run.id, &step, SKIPPED, json!({}), None).await?;
            continue;
        }

        let outcome = perform(db, secrets, &step, &seen).await;
        match outcome {
            Ok(output) => {
                // A branch that says no does not fail; it ends the flow.
                if step.action == action::BRANCH
                    && output.get("passed").and_then(Value::as_bool) == Some(false)
                {
                    skipping = true;
                }
                seen[format!("step{}", step.position)] = output.clone();
                record(db, run.id, &step, SUCCEEDED, output, None).await?;
            }
            Err(err) => {
                let message = err.to_string();
                record(db, run.id, &step, FAILED, json!({}), Some(&message)).await?;
                if step.on_error != "continue" {
                    return Err(AppError::Validation(format!(
                        "step {} ({}) failed: {message}",
                        step.position + 1,
                        step.action
                    )));
                }
            }
        }
    }

    Ok(())
}

async fn record(
    db: &DatabaseConnection,
    run_id: Uuid,
    step: &flow_step::Model,
    status: &str,
    output: Value,
    error: Option<&str>,
) -> AppResult<()> {
    flow_run_step::ActiveModel {
        id: Set(Uuid::now_v7()),
        run_id: Set(run_id),
        position: Set(step.position),
        action: Set(step.action.clone()),
        status: Set(status.to_string()),
        output: Set(output.to_string()),
        error: Set(error.map(str::to_string)),
        finished_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;
    Ok(())
}

async fn perform(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    step: &flow_step::Model,
    seen: &Value,
) -> AppResult<Value> {
    let config: Value = serde_json::from_str(&step.config)
        .map_err(|err| AppError::Validation(format!("this step's settings are not JSON: {err}")))?;

    match step.action.as_str() {
        action::MAIL_SEND => send_mail(db, secrets, &config, seen).await,
        action::HTTP_REQUEST => make_request(&config, seen, None).await,
        action::BRANCH => Ok(json!({ "passed": branch_passes(&config, seen) })),
        action::SLACK => {
            post_json(&config, seen, "webhook_url", |text| json!({ "text": text })).await
        }
        action::DISCORD => {
            post_json(
                &config,
                seen,
                "webhook_url",
                |text| json!({ "content": text }),
            )
            .await
        }
        action::TELEGRAM => send_telegram(db, secrets, &config, seen).await,
        other => Err(AppError::Validation(format!(
            "{other} is not something a flow can do"
        ))),
    }
}

async fn send_mail(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    config: &Value,
    seen: &Value,
) -> AppResult<Value> {
    let to = fill(config.get("to").and_then(Value::as_str).unwrap_or(""), seen);
    let subject = fill(
        config.get("subject").and_then(Value::as_str).unwrap_or(""),
        seen,
    );
    let body = fill(
        config.get("body").and_then(Value::as_str).unwrap_or(""),
        seen,
    );

    if to.trim().is_empty() {
        return Err(AppError::Validation(
            "this step has nobody to send to".to_string(),
        ));
    }

    // An account of the site's own comes first. Without one the site's own
    // mail settings are used, which is what a site that has SES set up wants
    // and the only thing a site that has nothing else can do.
    if let Some(id) = config.get("credential_id").and_then(Value::as_str)
        && !id.trim().is_empty()
    {
        let account: crate::smtp::SmtpAccount =
            open_credential(db, secrets, id, credential::SMTP).await?;
        let said = crate::smtp::send(
            &account,
            &to,
            &subject,
            &body,
            config.get("html").and_then(Value::as_bool).unwrap_or(false),
        )
        .await?;
        return Ok(json!({ "to": to, "subject": subject, "server_said": said }));
    }

    let settings = crate::plugins::load::<crate::email::EmailConfig>(
        db,
        secrets,
        crate::plugins::EMAIL_PLUGIN,
    )
    .await?
    .map(|stored| stored.config)
    .ok_or_else(|| {
        AppError::Validation(
            "this site has no mail settings and this step names no account".to_string(),
        )
    })?;

    crate::email::send(
        &settings,
        crate::email::Message {
            to: &to,
            subject: &subject,
            text: &body,
            html: None,
            from: None,
            unsubscribe_url: None,
            tags: &[],
        },
    )
    .await?;

    Ok(json!({ "to": to, "subject": subject }))
}

/// Reads one of the site's stored credentials, checking it is the kind the
/// step expects — a Telegram token where an SMTP account belongs would
/// otherwise be a confusing failure much further along.
async fn open_credential<T: serde::de::DeserializeOwned>(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    id: &str,
    wanted: &str,
) -> AppResult<T> {
    let id = Uuid::parse_str(id.trim())
        .map_err(|_| AppError::Validation("that is not a credential".to_string()))?;
    let found = flow_credential::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation("this step names a credential that is gone".to_string())
        })?;

    if found.kind != wanted {
        return Err(AppError::Validation(format!(
            "\"{}\" is a {} account, and this step needs a {wanted} one",
            found.name, found.kind
        )));
    }

    let clear = secrets.decrypt(&found.secret)?;
    serde_json::from_str(&clear)
        .map_err(|err| AppError::Internal(format!("that credential is unreadable: {err}")))
}

/// The shape shared by every "post a message to a chat" integration: an
/// address the site was given, and a body somebody else's product decided on.
async fn post_json(
    config: &Value,
    seen: &Value,
    address_key: &str,
    shape: impl Fn(&str) -> Value,
) -> AppResult<Value> {
    let url = fill(
        config
            .get(address_key)
            .and_then(Value::as_str)
            .unwrap_or(""),
        seen,
    );
    if url.trim().is_empty() {
        return Err(AppError::Validation(
            "this step has no address to post to".to_string(),
        ));
    }
    let text = fill(
        config.get("text").and_then(Value::as_str).unwrap_or(""),
        seen,
    );

    let sent = json!({
        "url": url,
        "method": "POST",
        "headers": { "content-type": "application/json" },
        "body": shape(&text).to_string(),
    });
    // Through the same door as any other request, so the address check is not
    // something each integration remembers separately. The address is a
    // credential, so it is not what a failure gets to write down.
    make_request(&sent, &Value::Null, Some("the chat's address")).await
}

async fn send_telegram(
    db: &DatabaseConnection,
    secrets: &SecretBox,
    config: &Value,
    seen: &Value,
) -> AppResult<Value> {
    #[derive(serde::Deserialize)]
    struct Bot {
        token: String,
    }

    let id = config
        .get("credential_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let bot: Bot = open_credential(db, secrets, id, credential::TELEGRAM).await?;
    let chat = fill(
        config.get("chat_id").and_then(Value::as_str).unwrap_or(""),
        seen,
    );
    let text = fill(
        config.get("text").and_then(Value::as_str).unwrap_or(""),
        seen,
    );

    let sent = json!({
        "url": format!("https://api.telegram.org/bot{}/sendMessage", bot.token.trim()),
        "method": "POST",
        "headers": { "content-type": "application/json" },
        "body": json!({ "chat_id": chat, "text": text }).to_string(),
    });
    make_request(&sent, &Value::Null, Some("Telegram")).await
}

/// `shown` is what appears in an error instead of the address.
///
/// A Telegram bot's token is in its address and a Slack webhook address is
/// itself the credential, so an error that names the address writes a secret
/// into the run record — which the panel shows and anybody with the panel can
/// read. The address is still checked and still used; it is only what gets
/// written down that is blunted.
async fn make_request(config: &Value, seen: &Value, shown: Option<&str>) -> AppResult<Value> {
    let url = fill(
        config.get("url").and_then(Value::as_str).unwrap_or(""),
        seen,
    );
    let named = shown.unwrap_or(&url).to_string();
    let method = config
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("POST")
        .to_uppercase();

    // Every address a site can be made to fetch goes through this. Without it
    // a flow is a way to reach the cluster's own services and the cloud's
    // metadata address from outside.
    crate::fetch::ensure_public_host(&url)
        .await
        .map_err(|err| AppError::Validation(hide(&err.to_string(), &url, &named)))?;

    let body = fill(
        config.get("body").and_then(Value::as_str).unwrap_or(""),
        seen,
    );
    let mut request = reqwest::Client::builder()
        .timeout(STEP_PATIENCE)
        // Redirects are followed by default and a redirect is a second
        // address, which the check above never saw.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| AppError::Internal(err.to_string()))?
        .request(
            reqwest::Method::from_bytes(method.as_bytes())
                .map_err(|_| AppError::Validation(format!("{method} is not a method")))?,
            &url,
        );

    if let Some(headers) = config.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            if let Some(value) = value.as_str() {
                request = request.header(name, fill(value, seen));
            }
        }
    }
    if !body.is_empty() {
        request = request.body(body);
    }

    let response = request.send().await.map_err(|err| {
        AppError::Validation(hide(
            &format!("could not reach {named}: {err}"),
            &url,
            &named,
        ))
    })?;
    let status = response.status().as_u16();
    // Bounded: a step that fetches a hundred megabytes into a run record is a
    // step that fills somebody's database.
    let text: String = response
        .text()
        .await
        .unwrap_or_default()
        .chars()
        .take(4000)
        .collect();

    if !(200..300).contains(&status) {
        return Err(AppError::Validation(hide(
            &format!("{named} answered {status}: {text}"),
            &url,
            &named,
        )));
    }
    Ok(json!({ "status": status, "body": text }))
}

/// Takes the address back out of whatever a library decided to say.
///
/// The message is built from the safe name, but the error inside it came from
/// reqwest and carries the URL it was given.
fn hide(message: &str, url: &str, named: &str) -> String {
    if url.trim().is_empty() || url == named {
        return message.to_string();
    }
    message.replace(url, named)
}

/// Whether the flow carries on. Deliberately three comparisons and no
/// expression language: an expression language is somewhere a site's own text
/// gets evaluated, which is a different feature with a different review.
fn branch_passes(config: &Value, seen: &Value) -> bool {
    let left = fill(
        config.get("left").and_then(Value::as_str).unwrap_or(""),
        seen,
    );
    let right = fill(
        config.get("right").and_then(Value::as_str).unwrap_or(""),
        seen,
    );
    match config
        .get("test")
        .and_then(Value::as_str)
        .unwrap_or("equals")
    {
        "contains" => left.to_lowercase().contains(&right.to_lowercase()),
        "not_empty" => !left.trim().is_empty(),
        _ => left == right,
    }
}

/// Replaces `{{ trigger.form.email }}` with what is at that path.
///
/// Substitution and nothing else. A missing path becomes an empty string
/// rather than an error, because a flow that stops because somebody left the
/// telephone box blank is worse than one that sends a message without it.
pub fn fill(template: &str, seen: &Value) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str("{{");
            rest = after;
            continue;
        };
        let path = after[..end].trim();
        out.push_str(&at_path(seen, path));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn at_path(seen: &Value, path: &str) -> String {
    let mut here = seen;
    for part in path.split('.') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        here = match here.get(part) {
            Some(next) => next,
            None => return String::new(),
        };
    }
    match here {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The credentials a site holds, without their contents.
pub async fn credentials(db: &impl ConnectionTrait) -> AppResult<Vec<flow_credential::Model>> {
    Ok(flow_credential::Entity::find()
        .order_by_asc(flow_credential::Column::Name)
        .all(db)
        .await?)
}

/// What a run looked like, step by step.
pub async fn steps_of_run(
    db: &impl ConnectionTrait,
    run_id: Uuid,
) -> AppResult<Vec<flow_run_step::Model>> {
    Ok(flow_run_step::Entity::find()
        .filter(flow_run_step::Column::RunId.eq(run_id))
        .order_by_asc(flow_run_step::Column::Position)
        .all(db)
        .await?)
}

/// The values a trigger offers, so the panel can show what may be written into
/// a step rather than leaving somebody to guess the spelling.
pub fn offered_by(trigger_kind: &str) -> BTreeMap<&'static str, &'static str> {
    let mut out = BTreeMap::new();
    match trigger_kind {
        trigger::FORM_SUBMITTED => {
            out.insert("trigger.form", "Which form was filled in");
            out.insert("trigger.form_id", "Its id");
            out.insert("trigger.fields.<name>", "What was typed into each field");
        }
        trigger::POST_PUBLISHED => {
            out.insert("trigger.title", "The title");
            out.insert("trigger.slug", "Its address");
            out.insert("trigger.kind", "post, page, or the site's own kind");
            out.insert("trigger.locale", "Which language");
        }
        trigger::WEBHOOK => {
            out.insert("trigger.body", "Whatever was sent, as text");
            out.insert("trigger.json.<name>", "The same, when it was JSON");
        }
        trigger::SCHEDULE => {
            out.insert("trigger.at", "When it ran");
        }
        _ => {}
    }
    out.insert("site.url", "This site's address");
    out
}

#[cfg(test)]
mod tests {
    use super::{branch_passes, fill, known_action, known_trigger, offered_by, trigger};
    use serde_json::json;

    fn seen() -> serde_json::Value {
        json!({
            "trigger": {
                "form": "iletisim",
                "fields": { "email": "biri@example.invalid", "mesaj": "Merhaba" }
            },
            "site": { "url": "https://example.invalid" }
        })
    }

    #[test]
    fn a_value_is_put_where_it_was_asked_for() {
        assert_eq!(
            fill("Yeni mesaj: {{ trigger.fields.mesaj }}", &seen()),
            "Yeni mesaj: Merhaba"
        );
        assert_eq!(
            fill("{{trigger.fields.email}}", &seen()),
            "biri@example.invalid"
        );
    }

    #[test]
    fn a_path_that_is_not_there_leaves_a_gap_rather_than_stopping() {
        // Somebody left the telephone box blank. That is not a reason to fail
        // to send the message.
        assert_eq!(
            fill("Tel: {{ trigger.fields.telefon }}.", &seen()),
            "Tel: ."
        );
    }

    #[test]
    fn text_that_only_looks_like_a_placeholder_survives() {
        assert_eq!(fill("{{ acik kalan", &seen()), "{{ acik kalan");
        assert_eq!(fill("bir { sey }", &seen()), "bir { sey }");
    }

    #[test]
    fn a_branch_compares_what_it_was_given() {
        let equals = json!({ "test": "equals", "left": "{{ trigger.form }}", "right": "iletisim" });
        assert!(branch_passes(&equals, &seen()));

        let other = json!({ "test": "equals", "left": "{{ trigger.form }}", "right": "baska" });
        assert!(!branch_passes(&other, &seen()));

        let has =
            json!({ "test": "contains", "left": "{{ trigger.fields.mesaj }}", "right": "merhaba" });
        assert!(branch_passes(&has, &seen()));

        let filled = json!({ "test": "not_empty", "left": "{{ trigger.fields.telefon }}" });
        assert!(!branch_passes(&filled, &seen()));
    }

    #[test]
    fn an_address_that_is_a_credential_is_not_written_into_the_record() {
        // A Slack webhook address is the credential, and a Telegram bot's
        // token is in its address. Whatever a library says about a failure has
        // the address in it, and the run record is read in the panel.
        let secret = "https://hooks.slack.com/services/T000/B000/xoxbSECRET";
        let said = format!("could not reach {secret}: connection refused");
        let hidden = super::hide(&said, secret, "the chat's address");

        assert!(!hidden.contains("xoxbSECRET"));
        assert!(hidden.contains("the chat's address"));
        assert!(hidden.contains("connection refused"));
    }

    #[test]
    fn a_plain_request_still_says_which_address_it_was() {
        let said = "https://example.invalid/ answered 500: oops";
        assert_eq!(
            super::hide(said, "https://example.invalid/", "https://example.invalid/"),
            said
        );
    }

    #[test]
    fn only_the_triggers_and_actions_that_exist_are_accepted() {
        assert!(known_trigger(trigger::FORM_SUBMITTED).is_ok());
        assert!(known_trigger("form.deleted").is_err());
        assert!(known_action("mail.send").is_ok());
        assert!(known_action("shell.run").is_err());
    }

    #[test]
    fn every_trigger_says_what_it_offers() {
        for kind in trigger::ALL {
            let offered = offered_by(kind);
            assert!(offered.contains_key("site.url"), "{kind} offers nothing");
        }
    }
}
