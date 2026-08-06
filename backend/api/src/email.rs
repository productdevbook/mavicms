//! Sending mail through Amazon SES.
//!
//! Through Amazon's own SDK. Signature Version 4 is four HMACs in an order
//! nothing tells you is wrong — a signature that is off produces a refusal
//! that names no cause — and there is no reason for this project to be the
//! one maintaining it. The SDK is Apache-2.0, which an MIT project can carry.
//!
//! No part of this is derived from another project's source.

use aws_sdk_sesv2::{
    Client,
    config::{BehaviorVersion, Credentials, Region},
    error::ProvideErrorMetadata,
    types::{Body, Content, Destination, EmailContent, Message as SesMessage},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{AppError, AppResult};

/// How long to wait on the API before giving up.
///
/// Sending is done while somebody waits for a page, so this is short. A form
/// submission is stored whether or not the notification goes out.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailConfig {
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// The address mail comes from. SES will refuse to send from one it has
    /// not verified, which is the most common reason a first send fails.
    pub from_address: String,
    pub from_name: String,
    pub reply_to: String,
    /// An SES configuration set, if the account uses them for tracking or
    /// dedicated IPs. Left empty, SES uses the account default.
    pub configuration_set: String,
}

/// What the panel is shown. The secret key is not among the fields: once
/// stored it never comes back out.
#[derive(Debug, Serialize, ToSchema)]
pub struct EmailSettingsResponse {
    pub enabled: bool,
    pub region: String,
    pub access_key_id: String,
    pub from_address: String,
    pub from_name: String,
    pub reply_to: String,
    pub configuration_set: String,
    pub has_secret_access_key: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmailSettingsRequest {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub access_key_id: String,
    /// Left out to keep the stored secret unchanged.
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
    #[serde(default)]
    pub reply_to: String,
    #[serde(default)]
    pub configuration_set: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TestEmailRequest {
    /// Where to send it. In a new SES account this has to be a verified
    /// address too, because the account is in the sandbox until Amazon takes
    /// it out.
    pub to: String,
}

/// One message.
pub struct Message<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub text: &'a str,
    /// Sent alongside the text part when present. A message with both lets a
    /// reader's own client decide, and one with only HTML looks like spam to
    /// several filters that count parts.
    pub html: Option<&'a str>,
}

/// An address as SES wants it: `Name <someone@example.com>`, or the bare
/// address when there is no name.
fn addressed(name: &str, address: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return address.trim().to_string();
    }
    // A name with a quote or a backslash in it would end the quoted string
    // early; both are escaped rather than stripped, so somebody called
    // O"Brien still gets their name on the message.
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\" <{}>", address.trim())
}

/// Not an address grammar — the mistakes worth catching here are the ones a
/// person can see and fix. Whether an address receives mail is a question only
/// sending to it answers.
pub fn looks_like_an_address(value: &str) -> bool {
    let value = value.trim();

    // Characters that need quoting to appear in an address at all. A comma is
    // the one that matters: an address with one in it is nearly always a row
    // of a spreadsheet that was split in the wrong place, and storing it
    // means SES refuses it on every send from now on.
    let impossible = |c: char| c.is_whitespace() || ",;<>()[]\\\"".contains(c);

    match value.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
                && !value.contains(impossible)
        }
        None => false,
    }
}

/// The subject and body, as the SDK's types.
fn content_of(message: &Message<'_>) -> AppResult<EmailContent> {
    let utf8 = |data: &str| {
        Content::builder()
            .data(data)
            .charset("UTF-8")
            .build()
            .map_err(|err| AppError::Internal(format!("could not build the message: {err}")))
    };

    let mut body = Body::builder().text(utf8(message.text)?);
    if let Some(html) = message.html {
        body = body.html(utf8(html)?);
    }

    Ok(EmailContent::builder()
        .simple(
            SesMessage::builder()
                .subject(utf8(message.subject)?)
                .body(body.build())
                .build(),
        )
        .build())
}

fn client_for(config: &EmailConfig) -> Client {
    // Static credentials rather than the SDK's provider chain: these belong to
    // one site and are read from its own settings, not from the environment
    // the server happens to be running in. A site must not send as another.
    let credentials = Credentials::new(
        config.access_key_id.trim(),
        config.secret_access_key.trim(),
        None,
        None,
        "mavicms-site-settings",
    );

    Client::from_conf(
        aws_sdk_sesv2::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region.trim().to_string()))
            .credentials_provider(credentials)
            .build(),
    )
}

/// Sends one message, and says what went wrong in words rather than a status
/// code — SES puts the useful part in the message, and it is usually
/// actionable ("Email address is not verified").
pub async fn send(config: &EmailConfig, message: Message<'_>) -> AppResult<()> {
    if config.region.trim().is_empty() {
        return Err(AppError::Validation("no AWS region is set".to_string()));
    }
    if config.access_key_id.trim().is_empty() || config.secret_access_key.is_empty() {
        return Err(AppError::Validation(
            "no access key is set for SES".to_string(),
        ));
    }
    if !looks_like_an_address(&config.from_address) {
        return Err(AppError::Validation(
            "the address mail comes from is not an email address".to_string(),
        ));
    }
    if !looks_like_an_address(message.to) {
        return Err(AppError::Validation(
            "that is not an email address".to_string(),
        ));
    }

    let mut request = client_for(config)
        .send_email()
        .from_email_address(addressed(&config.from_name, &config.from_address))
        .destination(
            Destination::builder()
                .to_addresses(message.to.trim())
                .build(),
        )
        .content(content_of(&message)?);

    if !config.reply_to.trim().is_empty() {
        request = request.reply_to_addresses(config.reply_to.trim());
    }
    if !config.configuration_set.trim().is_empty() {
        request = request.configuration_set_name(config.configuration_set.trim());
    }

    // Somebody is waiting on a page while this happens, so it is not allowed
    // to take as long as the SDK would wait by default.
    let sent = tokio::time::timeout(TIMEOUT, request.send())
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?;

    sent.map(|_| ()).map_err(|err| {
        // The sentence SES wrote, not the debug rendering of the whole error
        // chain. "Email address is not verified" tells somebody what to do;
        // four hundred characters of nested structs tell them to give up.
        let detail = err
            .message()
            .map(str::to_string)
            .or_else(|| err.code().map(|code| code.to_string()))
            .unwrap_or_else(|| aws_sdk_sesv2::error::DisplayErrorContext(&err).to_string());

        AppError::Validation(format!("SES refused it: {detail}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_quoted_and_a_missing_one_is_not_invented() {
        assert_eq!(addressed("Sema", "a@b.com"), "\"Sema\" <a@b.com>");
        assert_eq!(addressed("  ", "a@b.com"), "a@b.com");
        assert_eq!(addressed("O\"Brien", "a@b.com"), "\"O\\\"Brien\" <a@b.com>");
    }

    #[test]
    fn a_message_with_html_carries_both_parts() {
        // Text as well as HTML: a message with only HTML looks like spam to
        // several filters that count parts.
        let content = content_of(&Message {
            to: "to@example.com",
            subject: "Subject",
            text: "plain",
            html: Some("<p>rich</p>"),
        })
        .unwrap();

        let body = content.simple().unwrap().body().unwrap();
        assert_eq!(body.text().unwrap().data(), "plain");
        assert_eq!(body.html().unwrap().data(), "<p>rich</p>");
    }

    #[test]
    fn a_message_without_html_has_no_empty_html_part() {
        let content = content_of(&Message {
            to: "to@example.com",
            subject: "Subject",
            text: "plain",
            html: None,
        })
        .unwrap();

        assert!(content.simple().unwrap().body().unwrap().html().is_none());
    }

    #[test]
    fn addresses_are_checked_loosely_but_not_pointlessly() {
        assert!(looks_like_an_address("someone@example.com"));
        assert!(!looks_like_an_address("someone@example"));
        assert!(!looks_like_an_address("@example.com"));
        assert!(!looks_like_an_address("two words@example.com"));
        assert!(!looks_like_an_address("no-at-sign"));
        // A row of a spreadsheet that was split in the wrong place.
        assert!(!looks_like_an_address("bir,iki@example.com"));
        assert!(!looks_like_an_address("Ada <ada@example.com>"));
        assert!(!looks_like_an_address("a@b@example.com"));
        // Still ordinary addresses.
        assert!(looks_like_an_address("ada.lovelace+news@example.co.uk"));
        assert!(looks_like_an_address("a_b-c@sub.example.com"));
    }
}

/// What Amazon will say about the account behind these credentials.
///
/// The panel shows it because every question people ask about SES is
/// answered here: why a message did not arrive (the account is in the
/// sandbox), why sending stopped (enforcement), how much is left today
/// (quota), and which addresses SES will refuse (identities).
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountStatus {
    /// False while the account is in the sandbox, where SES will only write
    /// to addresses it has verified. This is what a limit increase changes.
    pub production_access: bool,
    pub sending_enabled: bool,
    /// "HEALTHY", or the word Amazon uses when it has paused an account.
    pub enforcement_status: String,
    pub max_24_hour_send: f64,
    pub max_send_rate: f64,
    pub sent_last_24_hours: f64,
    /// What was said in the last production-access request, if one was made.
    pub mail_type: String,
    pub website_url: String,
    pub use_case_description: String,
    pub contact_language: String,
    pub additional_contacts: Vec<String>,
    /// Whether Amazon has answered the last request. Absent when none was
    /// made through this panel or the console.
    pub review_status: String,
}

/// One address or domain SES has been asked to trust.
#[derive(Debug, Serialize, ToSchema)]
pub struct Identity {
    pub name: String,
    /// "EMAIL_ADDRESS" or "DOMAIN".
    pub kind: String,
    pub verified: bool,
    /// The CNAME records a domain needs before SES will sign for it. Empty
    /// for an address, which is verified by clicking a link instead.
    pub dkim_tokens: Vec<String>,
    pub dkim_status: String,
}

/// A request to be let out of the sandbox.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProductionAccessRequest {
    /// "TRANSACTIONAL" or "MARKETING". Amazon asks, and it decides what it
    /// expects of the sending.
    pub mail_type: String,
    pub website_url: String,
    /// What the mail is for, in a sentence or two. Amazon reads this.
    pub use_case_description: String,
    /// An ISO language for Amazon's reply: "EN" or "JA" are the only two it
    /// takes, so anything else is sent as English.
    #[serde(default)]
    pub contact_language: String,
    #[serde(default)]
    pub additional_contacts: Vec<String>,
}

fn account_settings(config: &EmailConfig) -> AppResult<()> {
    if config.region.trim().is_empty() {
        return Err(AppError::Validation("no AWS region is set".to_string()));
    }
    if config.access_key_id.trim().is_empty() || config.secret_access_key.is_empty() {
        return Err(AppError::Validation(
            "no access key is set for SES".to_string(),
        ));
    }
    Ok(())
}

/// Turns an SDK error into the sentence Amazon wrote.
fn refused(what: &str, err: impl ProvideErrorMetadata) -> AppError {
    let detail = err
        .message()
        .map(str::to_string)
        .or_else(|| err.code().map(|code| code.to_string()))
        .unwrap_or_else(|| "no reason given".to_string());

    AppError::Validation(format!("SES refused to {what}: {detail}"))
}

pub async fn account(config: &EmailConfig) -> AppResult<AccountStatus> {
    account_settings(config)?;

    let found = tokio::time::timeout(TIMEOUT, client_for(config).get_account().send())
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
        .map_err(|err| refused("say anything about the account", err))?;

    let quota = found.send_quota();
    let details = found.details();

    Ok(AccountStatus {
        production_access: found.production_access_enabled(),
        sending_enabled: found.sending_enabled(),
        enforcement_status: found.enforcement_status().unwrap_or_default().to_string(),
        max_24_hour_send: quota.map(|q| q.max24_hour_send()).unwrap_or(0.0),
        max_send_rate: quota.map(|q| q.max_send_rate()).unwrap_or(0.0),
        sent_last_24_hours: quota.map(|q| q.sent_last24_hours()).unwrap_or(0.0),
        mail_type: details
            .and_then(|d| d.mail_type())
            .map(|m| m.as_str().to_string())
            .unwrap_or_default(),
        website_url: details
            .and_then(|d| d.website_url())
            .unwrap_or_default()
            .to_string(),
        use_case_description: details
            .and_then(|d| d.use_case_description())
            .unwrap_or_default()
            .to_string(),
        contact_language: details
            .and_then(|d| d.contact_language())
            .map(|l| l.as_str().to_string())
            .unwrap_or_default(),
        additional_contacts: details
            .map(|d| d.additional_contact_email_addresses().to_vec())
            .unwrap_or_default(),
        review_status: details
            .and_then(|d| d.review_details())
            .and_then(|r| r.status())
            .map(|s| s.as_str().to_string())
            .unwrap_or_default(),
    })
}

/// Asks Amazon to take the account out of the sandbox.
///
/// This is the same request as the one in the AWS console, made through the
/// API so that whoever runs a site never has to be given an AWS login. Amazon
/// answers by mail, usually within a day, and `account()` shows where it got
/// to.
pub async fn request_production_access(
    config: &EmailConfig,
    request: ProductionAccessRequest,
) -> AppResult<()> {
    use aws_sdk_sesv2::types::{ContactLanguage, MailType};

    account_settings(config)?;

    if request.use_case_description.trim().len() < 30 {
        return Err(AppError::Validation(
            "Amazon reads this: say what the mail is for, in a sentence or two".to_string(),
        ));
    }
    if !request.website_url.starts_with("http") {
        return Err(AppError::Validation(
            "the website address should start with http:// or https://".to_string(),
        ));
    }

    let mail_type = match request.mail_type.to_uppercase().as_str() {
        "MARKETING" => MailType::Marketing,
        _ => MailType::Transactional,
    };
    // Amazon takes English or Japanese and refuses anything else, so a
    // Turkish panel asks in English rather than failing.
    let language = match request.contact_language.to_uppercase().as_str() {
        "JA" => ContactLanguage::Ja,
        _ => ContactLanguage::En,
    };

    let mut call = client_for(config)
        .put_account_details()
        .mail_type(mail_type)
        .website_url(request.website_url.trim())
        .use_case_description(request.use_case_description.trim())
        .contact_language(language)
        .production_access_enabled(true);

    for address in request
        .additional_contacts
        .iter()
        .filter(|a| looks_like_an_address(a))
    {
        call = call.additional_contact_email_addresses(address.trim());
    }

    tokio::time::timeout(TIMEOUT, call.send())
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
        .map_err(|err| refused("take the request", err))?;

    Ok(())
}

/// The addresses and domains SES will send from.
pub async fn identities(config: &EmailConfig) -> AppResult<Vec<Identity>> {
    account_settings(config)?;
    let client = client_for(config);

    let listed = tokio::time::timeout(
        TIMEOUT,
        client.list_email_identities().page_size(100).send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("list the identities", err))?;

    let mut out = Vec::new();
    for entry in listed.email_identities() {
        let name = entry.identity_name().unwrap_or_default().to_string();

        // The list says whether it is verified but not the DKIM records a
        // domain still needs, and those records are the whole of what
        // somebody has to do next.
        let detail = client
            .get_email_identity()
            .email_identity(&name)
            .send()
            .await
            .ok();

        out.push(Identity {
            kind: entry
                .identity_type()
                .map(|t| t.as_str().to_string())
                .unwrap_or_default(),
            verified: entry.sending_enabled(),
            dkim_tokens: detail
                .as_ref()
                .and_then(|d| d.dkim_attributes())
                .map(|d| d.tokens().to_vec())
                .unwrap_or_default(),
            dkim_status: detail
                .as_ref()
                .and_then(|d| d.dkim_attributes())
                .and_then(|d| d.status())
                .map(|s| s.as_str().to_string())
                .unwrap_or_default(),
            name,
        });
    }
    Ok(out)
}

/// Asks SES to trust an address or a domain.
///
/// An address gets a message with a link in it. A domain gets three CNAME
/// records to publish, which `identities` then hands back.
pub async fn add_identity(config: &EmailConfig, name: &str) -> AppResult<()> {
    account_settings(config)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation(
            "an address or a domain is needed".to_string(),
        ));
    }

    tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .create_email_identity()
            .email_identity(name)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("add it", err))?;

    Ok(())
}

pub async fn remove_identity(config: &EmailConfig, name: &str) -> AppResult<()> {
    account_settings(config)?;

    tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .delete_email_identity()
            .email_identity(name.trim())
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("remove it", err))?;

    Ok(())
}

/// An address SES has stopped writing to, and why.
#[derive(Debug, Serialize, ToSchema)]
pub struct Suppressed {
    pub address: String,
    /// "BOUNCE" or "COMPLAINT".
    pub reason: String,
    pub since: String,
}

/// SES keeps this list itself, per account, and honours it whatever this
/// program does. Reading Amazon's rather than keeping a second copy means the
/// panel cannot disagree with what actually happens.
pub async fn suppressed(config: &EmailConfig) -> AppResult<Vec<Suppressed>> {
    account_settings(config)?;

    let listed = tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .list_suppressed_destinations()
            .page_size(200)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("list the blocked addresses", err))?;

    Ok(listed
        .suppressed_destination_summaries()
        .iter()
        .map(|entry| Suppressed {
            address: entry.email_address().to_string(),
            reason: entry.reason().as_str().to_string(),
            since: entry.last_update_time().to_string(),
        })
        .collect())
}

/// Takes an address off that list, for when somebody fixed their mailbox.
pub async fn unsuppress(config: &EmailConfig, address: &str) -> AppResult<()> {
    account_settings(config)?;

    tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .delete_suppressed_destination()
            .email_address(address.trim())
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("unblock it", err))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Health: the numbers Amazon suspends accounts over
// ---------------------------------------------------------------------------

/// Amazon reviews an account whose bounce rate reaches this, and may pause it
/// at twice this. Published in their sending-review policy.
const BOUNCE_REVIEW: f64 = 5.0;
const BOUNCE_PAUSE: f64 = 10.0;
/// The same for complaints, which are judged far more harshly.
const COMPLAINT_REVIEW: f64 = 0.1;
const COMPLAINT_PAUSE: f64 = 0.5;

#[derive(Debug, Serialize, ToSchema)]
pub struct SendingHealth {
    /// Over the last two weeks, which is as far back as SES keeps this.
    pub delivery_attempts: i64,
    pub bounces: i64,
    pub complaints: i64,
    /// Refused before sending — a suppressed address, a malformed message.
    pub rejects: i64,
    pub bounce_rate: f64,
    pub complaint_rate: f64,
    /// "healthy", "watch" or "danger", against Amazon's own thresholds, so
    /// somebody can see trouble coming rather than read about it in a
    /// suspension notice.
    pub bounce_standing: String,
    pub complaint_standing: String,
    pub bounce_review_at: f64,
    pub bounce_pause_at: f64,
    pub complaint_review_at: f64,
    pub complaint_pause_at: f64,
    /// A point per day, oldest first, for drawing the shape of it.
    pub days: Vec<HealthDay>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthDay {
    pub day: String,
    pub delivery_attempts: i64,
    pub bounces: i64,
    pub complaints: i64,
}

fn standing(rate: f64, review: f64, pause: f64) -> String {
    if rate >= pause {
        "danger"
    } else if rate >= review {
        "watch"
    } else {
        "healthy"
    }
    .to_string()
}

fn v1_client(config: &EmailConfig) -> aws_sdk_ses::Client {
    let credentials = aws_sdk_ses::config::Credentials::new(
        config.access_key_id.trim(),
        config.secret_access_key.trim(),
        None,
        None,
        "mavicms-site-settings",
    );

    aws_sdk_ses::Client::from_conf(
        aws_sdk_ses::Config::builder()
            .behavior_version(aws_sdk_ses::config::BehaviorVersion::latest())
            .region(aws_sdk_ses::config::Region::new(
                config.region.trim().to_string(),
            ))
            .credentials_provider(credentials)
            .build(),
    )
}

/// How the account has been behaving.
///
/// From the v1 API, which is the only place these numbers are free: v2 offers
/// them through the deliverability dashboard or Virtual Deliverability
/// Manager, both of which cost money, and an account's bounce rate should not
/// be behind a paywall in a panel whose whole job is to keep it out of
/// trouble.
pub async fn health(config: &EmailConfig) -> AppResult<SendingHealth> {
    account_settings(config)?;

    let stats = tokio::time::timeout(TIMEOUT, v1_client(config).get_send_statistics().send())
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
        .map_err(|err| refused("give the sending statistics", err))?;

    let points = stats.send_data_points();

    let mut attempts = 0i64;
    let mut bounces = 0i64;
    let mut complaints = 0i64;
    let mut rejects = 0i64;
    // SES answers in fifteen-minute buckets; a fortnight of them is 1,344
    // rows, which is a chart nobody reads. Folded into days.
    let mut by_day: std::collections::BTreeMap<String, (i64, i64, i64)> =
        std::collections::BTreeMap::new();

    for point in points {
        attempts += point.delivery_attempts();
        bounces += point.bounces();
        complaints += point.complaints();
        rejects += point.rejects();

        if let Some(when) = point.timestamp() {
            let day = chrono::DateTime::from_timestamp(when.secs(), 0)
                .map(|t| t.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            let entry = by_day.entry(day).or_default();
            entry.0 += point.delivery_attempts();
            entry.1 += point.bounces();
            entry.2 += point.complaints();
        }
    }

    // Against attempts, which is how Amazon works it out.
    let rate = |count: i64| {
        if attempts == 0 {
            0.0
        } else {
            (count as f64 / attempts as f64) * 100.0
        }
    };
    let bounce_rate = rate(bounces);
    let complaint_rate = rate(complaints);

    Ok(SendingHealth {
        delivery_attempts: attempts,
        bounces,
        complaints,
        rejects,
        bounce_rate,
        complaint_rate,
        bounce_standing: standing(bounce_rate, BOUNCE_REVIEW, BOUNCE_PAUSE),
        complaint_standing: standing(complaint_rate, COMPLAINT_REVIEW, COMPLAINT_PAUSE),
        bounce_review_at: BOUNCE_REVIEW,
        bounce_pause_at: BOUNCE_PAUSE,
        complaint_review_at: COMPLAINT_REVIEW,
        complaint_pause_at: COMPLAINT_PAUSE,
        days: by_day
            .into_iter()
            .map(|(day, (attempts, bounces, complaints))| HealthDay {
                day,
                delivery_attempts: attempts,
                bounces,
                complaints,
            })
            .collect(),
    })
}

/// Stop or resume everything this account sends.
///
/// The switch Amazon itself uses when it pauses an account, offered here so
/// somebody who has just noticed a mistake can stop it in one press rather
/// than deleting settings.
pub async fn set_sending(config: &EmailConfig, enabled: bool) -> AppResult<()> {
    account_settings(config)?;

    tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .put_account_sending_attributes()
            .sending_enabled(enabled)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("change it", err))?;

    Ok(())
}

/// The configuration sets this account has, so the setting is a choice rather
/// than a name somebody has to remember exactly.
pub async fn configuration_sets(config: &EmailConfig) -> AppResult<Vec<String>> {
    account_settings(config)?;

    let listed = tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .list_configuration_sets()
            .page_size(100)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("list the configuration sets", err))?;

    Ok(listed.configuration_sets().to_vec())
}

// ---------------------------------------------------------------------------
// Asking for more: the sandbox first, then the quota
// ---------------------------------------------------------------------------

/// A request to be allowed to send more than the account's current quota.
#[derive(Debug, Deserialize, ToSchema)]
pub struct QuotaIncreaseRequest {
    /// Messages a day. Ask for what the next year needs, not the next week —
    /// Amazon reads a second request from the same account differently.
    pub daily_limit: u32,
    /// Messages a second. Left at zero, only the daily figure is asked for.
    #[serde(default)]
    pub send_rate: u32,
    pub website_url: String,
    /// Who receives the mail, how they came to be on the list, how they stop
    /// it, and how bounces and complaints are handled. A person reads this.
    pub use_case_description: String,
    /// "en" or "ja". Amazon answers in one of those two.
    #[serde(default)]
    pub language: String,
}

/// One request and where it got to.
#[derive(Debug, Serialize, ToSchema)]
pub struct SupportCase {
    pub id: String,
    pub subject: String,
    /// Amazon's own word: "opened", "pending-customer-action", "resolved" and
    /// so on.
    pub status: String,
    pub created_at: String,
    /// The last thing said, whoever said it.
    pub latest: String,
}

fn support_client(config: &EmailConfig) -> aws_sdk_support::Client {
    let credentials = aws_sdk_support::config::Credentials::new(
        config.access_key_id.trim(),
        config.secret_access_key.trim(),
        None,
        None,
        "mavicms-site-settings",
    );

    // Support is a global service and only answers in us-east-1, whatever
    // region the mail goes through.
    aws_sdk_support::Client::from_conf(
        aws_sdk_support::Config::builder()
            .behavior_version(aws_sdk_support::config::BehaviorVersion::latest())
            .region(aws_sdk_support::config::Region::new("us-east-1"))
            .credentials_provider(credentials)
            .build(),
    )
}

/// Turns the two refusals people actually hit into sentences that say what to
/// do, because Amazon's own wording for them explains nothing.
fn support_refusal(code: Option<&str>, message: Option<&str>) -> AppError {
    let detail = match code {
        Some("SubscriptionRequiredException") => {
            "this AWS account's support plan does not allow cases to be opened through the API. \
             Ask on the AWS console instead, under Support → Create case → Service limit increase, \
             or move the account to a Business plan"
                .to_string()
        }
        Some("AccessDeniedException") | Some("AccessDenied") => {
            "these keys may send mail but may not open a support case. \
             Add support:CreateCase and support:DescribeCases to the IAM user, \
             or ask on the AWS console instead"
                .to_string()
        }
        _ => message.unwrap_or("no reason given").to_string(),
    };

    AppError::Validation(format!("Amazon would not take the request: {detail}"))
}

/// Asks Amazon for a bigger quota.
///
/// Leaving the sandbox is one request and this is the other. `PutAccountDetails`
/// handles the first; there is no API for the second, because Amazon treats it
/// as a support case — so this opens one, in the same words the console would.
pub async fn request_quota_increase(
    config: &EmailConfig,
    request: QuotaIncreaseRequest,
) -> AppResult<SupportCase> {
    use aws_sdk_support::error::ProvideErrorMetadata as _;

    account_settings(config)?;

    if request.use_case_description.trim().len() < 50 {
        return Err(AppError::Validation(
            "Amazon reads this: say who receives the mail, how they came to be on the list, \
             how they stop it, and what happens to bounces"
                .to_string(),
        ));
    }
    if request.daily_limit == 0 {
        return Err(AppError::Validation(
            "say how many messages a day you need".to_string(),
        ));
    }
    if !request.website_url.starts_with("http") {
        return Err(AppError::Validation(
            "the website address should start with http:// or https://".to_string(),
        ));
    }

    let region = config.region.trim();
    let mut body = format!(
        "Requesting a sending quota increase for Amazon SES in {region}.\n\n\
         Desired daily sending quota: {} messages per 24 hours\n",
        request.daily_limit
    );
    if request.send_rate > 0 {
        body.push_str(&format!(
            "Desired maximum send rate: {} messages per second\n",
            request.send_rate
        ));
    }
    body.push_str(&format!(
        "\nWebsite: {}\n\nHow the mail is used:\n{}\n",
        request.website_url.trim(),
        request.use_case_description.trim()
    ));

    let language = if request.language.to_lowercase() == "ja" {
        "ja"
    } else {
        "en"
    };

    let made = tokio::time::timeout(
        TIMEOUT,
        support_client(config)
            .create_case()
            .subject(format!(
                "SES sending quota increase — {} per day ({region})",
                request.daily_limit
            ))
            .service_code("amazon-ses")
            .category_code("sending-limits")
            // "low" is what a limit increase is; anything higher asks for a
            // response time this does not need and some plans do not allow.
            .severity_code("low")
            .communication_body(body)
            .language(language)
            .issue_type("service-limit-increase")
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("Amazon did not answer in time".to_string()))?
    .map_err(|err| support_refusal(err.code(), err.message()))?;

    let id = made.case_id().unwrap_or_default().to_string();

    Ok(SupportCase {
        subject: format!(
            "SES sending quota increase — {} per day",
            request.daily_limit
        ),
        status: "opened".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        latest: String::new(),
        id,
    })
}

/// The requests this account has made about sending, and where they got to.
///
/// So a request is something you can follow rather than something you send and
/// then wait for an email about.
pub async fn support_cases(config: &EmailConfig) -> AppResult<Vec<SupportCase>> {
    use aws_sdk_support::error::ProvideErrorMetadata as _;

    account_settings(config)?;

    let found = tokio::time::timeout(
        TIMEOUT,
        support_client(config)
            .describe_cases()
            .include_resolved_cases(true)
            .max_results(20)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("Amazon did not answer in time".to_string()))?
    .map_err(|err| support_refusal(err.code(), err.message()))?;

    Ok(found
        .cases()
        .iter()
        // Only what this panel is about. An account's other support cases are
        // none of a website panel's business.
        .filter(|case| case.service_code() == Some("amazon-ses"))
        .map(|case| SupportCase {
            id: case.case_id().unwrap_or_default().to_string(),
            subject: case.subject().unwrap_or_default().to_string(),
            status: case.status().unwrap_or_default().to_string(),
            created_at: case.time_created().unwrap_or_default().to_string(),
            latest: case
                .recent_communications()
                .and_then(|recent| recent.communications().first())
                .and_then(|note| note.body())
                .unwrap_or_default()
                .chars()
                .take(400)
                .collect(),
        })
        .collect())
}
