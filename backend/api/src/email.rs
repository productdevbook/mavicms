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

/// One address this site may send as.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct Sender {
    pub address: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailConfig {
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// The address mail comes from when nothing says otherwise. SES will
    /// refuse to send from one it has not verified, which is the most common
    /// reason a first send fails.
    pub from_address: String,
    pub from_name: String,
    /// The others. A site sends notifications as one address and its
    /// newsletter as another often enough that one field was never going to
    /// be enough — and every one of them still has to be verified in SES.
    #[serde(default)]
    pub senders: Vec<Sender>,
    /// The unguessable part of the address Amazon posts events to. Made once,
    /// when the pipeline is built.
    #[serde(default)]
    pub events_token: String,
    /// The topic those events come from, so one that did not is refused.
    #[serde(default)]
    pub events_topic_arn: String,
    pub reply_to: String,
    /// An SES configuration set, if the account uses them for tracking or
    /// dedicated IPs. Left empty, SES uses the account default.
    pub configuration_set: String,
    /// Whose mail this is, when the account is shared.
    ///
    /// Amazon keeps a tenant's reputation and sending status apart from every
    /// other tenant's, so one site collecting complaints is stopped on its
    /// own rather than taking the account down with it. Empty on an account
    /// with one user, which is every site sending with its own keys.
    #[serde(default)]
    pub tenant: String,
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
    /// Every address this site may send as, the default first.
    pub senders: Vec<Sender>,
    /// The unguessable part of the address Amazon posts events to. Shown so
    /// somebody who would rather wire the topic up themselves can, and so
    /// that when nothing arrives there is an address to check.
    pub events_token: String,
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
    /// The addresses beyond the default one.
    #[serde(default)]
    pub senders: Vec<Sender>,
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
    /// Which of the site's addresses to send as. The default when absent.
    pub from: Option<&'a Sender>,
    /// Where the reader's own mail client should point its unsubscribe
    /// button. Gmail and Yahoo have required this of bulk senders since
    /// February 2024; without it a newsletter lands in spam however clean the
    /// list is.
    pub unsubscribe_url: Option<&'a str>,
    /// Name and value pairs that come back on every event Amazon sends about
    /// this message. This is how a bounce arriving twenty minutes later is
    /// known to belong to one campaign and one subscriber, without keeping a
    /// table of message ids to look them up in.
    pub tags: &'a [(String, String)],
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
/// The unsubscribe headers, when the message has somewhere to point them.
///
/// Two of them, and both are needed: the first says where, the second says
/// that a single POST is enough and no page has to be visited. Gmail and
/// Yahoo check for the pair.
fn unsubscribe_headers(url: &str) -> Vec<aws_sdk_sesv2::types::MessageHeader> {
    use aws_sdk_sesv2::types::MessageHeader;

    [
        ("List-Unsubscribe", format!("<{url}>")),
        (
            "List-Unsubscribe-Post",
            "List-Unsubscribe=One-Click".to_string(),
        ),
    ]
    .into_iter()
    .filter_map(|(name, value)| {
        MessageHeader::builder()
            .name(name)
            .value(value)
            .build()
            .ok()
    })
    .collect()
}

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

    let mut simple = SesMessage::builder()
        .subject(utf8(message.subject)?)
        .body(body.build());

    if let Some(url) = message.unsubscribe_url {
        simple = simple.set_headers(Some(unsubscribe_headers(url)));
    }

    Ok(EmailContent::builder().simple(simple.build()).build())
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
    let from = message.from.cloned().unwrap_or_else(|| Sender {
        address: config.from_address.clone(),
        name: config.from_name.clone(),
    });

    if !looks_like_an_address(&from.address) {
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
        .from_email_address(addressed(&from.name, &from.address))
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
    // The whole of what keeps a shared account survivable: Amazon attributes
    // this message to one site, judges that site on its own, and can stop it
    // without stopping anybody else.
    if !config.tenant.trim().is_empty() {
        request = request.tenant_name(config.tenant.trim());
    }

    for (name, value) in message.tags {
        // Amazon takes letters, numbers, dash and underscore and refuses the
        // whole send over anything else, so a uuid is fine and a subject line
        // would not be.
        if let Ok(tag) = aws_sdk_sesv2::types::MessageTag::builder()
            .name(name)
            .value(value)
            .build()
        {
            request = request.email_tags(tag);
        }
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
            from: None,
            unsubscribe_url: None,
            tags: &[],
        })
        .unwrap();

        let body = content.simple().unwrap().body().unwrap();
        assert_eq!(body.text().unwrap().data(), "plain");
        assert_eq!(body.html().unwrap().data(), "<p>rich</p>");
    }

    #[test]
    fn an_unsubscribe_link_becomes_the_two_headers_gmail_looks_for() {
        let content = content_of(&Message {
            to: "to@example.com",
            subject: "s",
            text: "t",
            html: None,
            from: None,
            unsubscribe_url: Some("https://example.com/u/abc"),
            tags: &[],
        })
        .unwrap();

        let headers = content.simple().unwrap().headers();
        let named = |name: &str| {
            headers
                .iter()
                .find(|h| h.name().eq_ignore_ascii_case(name))
                .map(|h| h.value().to_string())
        };

        assert_eq!(
            named("List-Unsubscribe").as_deref(),
            Some("<https://example.com/u/abc>")
        );
        // Without the second one, a one-click button is not offered at all.
        assert_eq!(
            named("List-Unsubscribe-Post").as_deref(),
            Some("List-Unsubscribe=One-Click")
        );
    }

    #[test]
    fn a_message_with_nowhere_to_unsubscribe_carries_no_such_header() {
        let content = content_of(&Message {
            to: "to@example.com",
            subject: "s",
            text: "t",
            html: None,
            from: None,
            unsubscribe_url: None,
            tags: &[],
        })
        .unwrap();

        assert!(content.simple().unwrap().headers().is_empty());
    }

    #[test]
    fn a_message_without_html_has_no_empty_html_part() {
        let content = content_of(&Message {
            to: "to@example.com",
            subject: "Subject",
            text: "plain",
            html: None,
            from: None,
            unsubscribe_url: None,
            tags: &[],
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

    #[test]
    fn a_new_domain_gets_every_record_it_has_to_publish() {
        let tokens = ["aaa".to_string(), "bbb".to_string(), "ccc".to_string()];

        // Before a bounce subdomain is chosen: proof of ownership and DMARC.
        let fresh = domain_records("example.com", &tokens, "PENDING", "", "", "eu-central-1");
        assert_eq!(fresh.len(), 4);
        assert_eq!(fresh[0].kind, "CNAME");
        assert_eq!(fresh[0].host, "aaa._domainkey.example.com");
        assert_eq!(fresh[0].value, "aaa.dkim.amazonses.com");
        assert_eq!(fresh[0].status, "waiting");
        assert!(fresh.iter().all(|r| r.required));
        assert_eq!(fresh[3].host, "_dmarc.example.com");
        assert!(fresh[3].value.contains("p=none"));
        // Amazon neither sets nor reads DMARC, so nothing can claim it is up.
        assert_eq!(fresh[3].status, "unchecked");

        // After: the MX and SPF that make bounces come back to the site.
        let full = domain_records(
            "example.com",
            &tokens,
            "SUCCESS",
            "mail.example.com",
            "PENDING",
            "eu-central-1",
        );
        assert_eq!(full.len(), 6);
        assert_eq!(full[0].status, "verified");
        assert_eq!(full[3].kind, "MX");
        assert_eq!(full[3].host, "mail.example.com");
        assert_eq!(full[3].value, "10 feedback-smtp.eu-central-1.amazonses.com");
        assert_eq!(full[4].kind, "TXT");
        assert!(full[4].value.contains("include:amazonses.com"));

        // The MX has to name the region the identity lives in; a record copied
        // from another region's console is accepted by DNS and never works.
        let elsewhere = domain_records(
            "example.com",
            &tokens,
            "SUCCESS",
            "mail.example.com",
            "SUCCESS",
            "us-east-1",
        );
        assert!(elsewhere[3].value.contains("us-east-1"));
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

/// One line of DNS somebody has to publish.
///
/// Given whole rather than described: the point of this panel is that nobody
/// has to work out from Amazon's documentation what `_dmarc` is or which of
/// three CNAMEs goes where. Copy the row, paste it at the registrar, carry on.
#[derive(Debug, Serialize, ToSchema)]
pub struct DnsRecord {
    /// "CNAME", "MX" or "TXT".
    pub kind: String,
    pub host: String,
    pub value: String,
    /// What it buys: "dkim", "mail_from" or "dmarc".
    pub purpose: String,
    /// What Amazon says about it — "verified", "waiting", "failed" — or
    /// "unchecked" for DMARC, which Amazon does not look at.
    pub status: String,
    /// Whether the site must publish it, or only should.
    pub required: bool,
}

/// One address or domain SES has been asked to trust.
#[derive(Debug, Serialize, ToSchema)]
pub struct Identity {
    pub name: String,
    /// "EMAIL_ADDRESS" or "DOMAIN".
    pub kind: String,
    pub verified: bool,
    pub dkim_status: String,
    /// The subdomain bounces come back to, when one is set up.
    pub mail_from_domain: String,
    pub mail_from_status: String,
    /// Everything to publish, in one list. Empty for an address, which is
    /// verified by following a link instead.
    pub records: Vec<DnsRecord>,
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

    let region = config.region.trim();
    let mut out = Vec::new();

    for entry in listed.email_identities() {
        let name = entry.identity_name().unwrap_or_default().to_string();
        let kind = entry
            .identity_type()
            .map(|t| t.as_str().to_string())
            .unwrap_or_default();

        // The list says whether it is verified but not the records a domain
        // still needs, and those records are the whole of what somebody has
        // to do next.
        let detail = client
            .get_email_identity()
            .email_identity(&name)
            .send()
            .await
            .ok();

        let dkim = detail.as_ref().and_then(|d| d.dkim_attributes());
        let dkim_status = dkim
            .and_then(|d| d.status())
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        let mail_from = detail.as_ref().and_then(|d| d.mail_from_attributes());
        let mail_from_domain = mail_from
            .map(|m| m.mail_from_domain().to_string())
            .unwrap_or_default();
        let mail_from_status = mail_from
            .map(|m| m.mail_from_domain_status().as_str().to_string())
            .unwrap_or_default();

        let records = if kind == "DOMAIN" {
            domain_records(
                &name,
                dkim.map(|d| d.tokens()).unwrap_or_default(),
                &dkim_status,
                &mail_from_domain,
                &mail_from_status,
                region,
            )
        } else {
            Vec::new()
        };

        out.push(Identity {
            kind,
            verified: entry.sending_enabled(),
            dkim_status,
            mail_from_domain,
            mail_from_status,
            records,
            name,
        });
    }
    Ok(out)
}

/// Everything a domain has to publish, in the order somebody should do it.
///
/// Three CNAMEs so Amazon can sign; an MX and an SPF line so bounces come back
/// to a subdomain the site owns rather than to amazonses.com, which is what
/// makes the visible sender and the envelope sender agree; and DMARC, which
/// Gmail and Yahoo have required of bulk senders since 2024 and which nothing
/// in AWS will tell you is missing.
fn domain_records(
    domain: &str,
    dkim_tokens: &[String],
    dkim_status: &str,
    mail_from: &str,
    mail_from_status: &str,
    region: &str,
) -> Vec<DnsRecord> {
    let readable = |status: &str| {
        match status {
            "SUCCESS" => "verified",
            "PENDING" | "NOT_STARTED" => "waiting",
            "FAILED" | "TEMPORARY_FAILURE" => "failed",
            "" => "waiting",
            other => other,
        }
        .to_string()
    };

    let mut out: Vec<DnsRecord> = dkim_tokens
        .iter()
        .map(|token| DnsRecord {
            kind: "CNAME".to_string(),
            host: format!("{token}._domainkey.{domain}"),
            value: format!("{token}.dkim.amazonses.com"),
            purpose: "dkim".to_string(),
            status: readable(dkim_status),
            required: true,
        })
        .collect();

    if !mail_from.is_empty() {
        out.push(DnsRecord {
            kind: "MX".to_string(),
            host: mail_from.to_string(),
            value: format!("10 feedback-smtp.{region}.amazonses.com"),
            purpose: "mail_from".to_string(),
            status: readable(mail_from_status),
            required: true,
        });
        out.push(DnsRecord {
            kind: "TXT".to_string(),
            host: mail_from.to_string(),
            value: "\"v=spf1 include:amazonses.com ~all\"".to_string(),
            purpose: "mail_from".to_string(),
            status: readable(mail_from_status),
            required: true,
        });
    }

    out.push(DnsRecord {
        kind: "TXT".to_string(),
        host: format!("_dmarc.{domain}"),
        // p=none to begin with: a policy that rejects before anybody has seen
        // a report is how a domain silently stops delivering its own mail.
        value: format!("\"v=DMARC1; p=none; rua=mailto:dmarc@{domain}\""),
        purpose: "dmarc".to_string(),
        // Amazon neither sets nor checks this, so nothing here can say
        // whether it is published.
        status: "unchecked".to_string(),
        required: true,
    });

    out
}

/// Sets the subdomain bounces are returned to.
///
/// Without one, the envelope sender is amazonses.com and SPF is aligned to
/// Amazon rather than to the site — which passes, but tells a receiving server
/// less about whoever actually sent it, and fails DMARC alignment on SPF.
pub async fn set_mail_from(config: &EmailConfig, identity: &str, subdomain: &str) -> AppResult<()> {
    use aws_sdk_sesv2::types::BehaviorOnMxFailure;

    account_settings(config)?;
    let subdomain = subdomain.trim();

    let mut call = client_for(config)
        .put_email_identity_mail_from_attributes()
        .email_identity(identity.trim());

    if subdomain.is_empty() {
        // Left empty, SES goes back to its own domain.
        call = call.behavior_on_mx_failure(BehaviorOnMxFailure::UseDefaultValue);
    } else {
        if !subdomain.ends_with(identity.trim()) {
            return Err(AppError::Validation(format!(
                "the bounce subdomain has to be under {}",
                identity.trim()
            )));
        }
        call = call
            .mail_from_domain(subdomain)
            // Rejecting on MX failure would stop the site's mail the moment
            // the MX record was mistyped. Falling back keeps it sending while
            // somebody fixes the DNS.
            .behavior_on_mx_failure(BehaviorOnMxFailure::UseDefaultValue);
    }

    tokio::time::timeout(TIMEOUT, call.send())
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
        .map_err(|err| refused("set the bounce subdomain", err))?;

    Ok(())
}

/// Makes a configuration set, so that too is one less reason to open the AWS
/// console.
pub async fn create_configuration_set(config: &EmailConfig, name: &str) -> AppResult<()> {
    account_settings(config)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("it needs a name".to_string()));
    }

    tokio::time::timeout(
        TIMEOUT,
        client_for(config)
            .create_configuration_set()
            .configuration_set_name(name)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
    .map_err(|err| refused("make it", err))?;

    Ok(())
}

/// Asks SES to trust an address or a domain.
///
/// An address gets a message with a link in it. A domain gets three CNAME
/// records to publish, which `identities` then hands back.
/// Makes a container for one site's sending inside a shared account.
///
/// Amazon keeps a tenant's reputation and its sending status apart from every
/// other tenant's, which is the whole reason a shared account is survivable:
/// a site that collects complaints is paused on its own rather than taking
/// the account down with it. Making one that is already there is not an
/// error worth passing on — the answer to "does this site have a tenant" is
/// yes either way.
pub async fn create_tenant(config: &EmailConfig, name: &str) -> AppResult<()> {
    match client_for(config)
        .create_tenant()
        .tenant_name(name)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => {
            let said = err
                .message()
                .map(str::to_string)
                .unwrap_or_else(|| aws_sdk_sesv2::error::DisplayErrorContext(&err).to_string());
            if said.to_lowercase().contains("already exists") {
                return Ok(());
            }
            Err(AppError::Validation(format!(
                "SES would not make the tenant: {said}"
            )))
        }
    }
}

/// Ties an identity — a domain, or one address — to a site's tenant, so that
/// what it sends is judged as that site's and nobody else's.
pub async fn associate_with_tenant(
    config: &EmailConfig,
    tenant: &str,
    identity: &str,
) -> AppResult<()> {
    match client_for(config)
        .create_tenant_resource_association()
        .tenant_name(tenant)
        .resource_arn(identity)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => {
            let said = err
                .message()
                .map(str::to_string)
                .unwrap_or_else(|| aws_sdk_sesv2::error::DisplayErrorContext(&err).to_string());
            if said.to_lowercase().contains("already") {
                return Ok(());
            }
            Err(AppError::Validation(format!(
                "SES would not attach {identity} to {tenant}: {said}"
            )))
        }
    }
}

/// What Amazon says about one site's sending: whether it is still allowed to,
/// and what it has been doing.
pub async fn tenant_status(config: &EmailConfig, name: &str) -> AppResult<String> {
    let found = client_for(config)
        .get_tenant()
        .tenant_name(name)
        .send()
        .await
        .map_err(|err| {
            AppError::Validation(format!(
                "SES would not say: {}",
                err.message()
                    .map(str::to_string)
                    .unwrap_or_else(|| aws_sdk_sesv2::error::DisplayErrorContext(&err).to_string())
            ))
        })?;

    Ok(found
        .tenant()
        .and_then(|tenant| tenant.sending_status())
        .map(|status| status.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string()))
}

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

// ---------------------------------------------------------------------------
// The event pipeline: a configuration set, a topic, and everything Amazon
// knows coming back to this site.
// ---------------------------------------------------------------------------

/// What one press of "set up analytics" produced.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventPipeline {
    pub configuration_set: String,
    pub topic_arn: String,
    /// Where SNS was told to post. Kept so the panel can show what to check
    /// when nothing arrives.
    pub endpoint: String,
    /// Whether SNS has confirmed the subscription. It confirms itself by
    /// posting to the endpoint, so this is usually true within seconds.
    pub confirmed: bool,
}

fn sns_client(config: &EmailConfig) -> aws_sdk_sns::Client {
    let credentials = aws_sdk_sns::config::Credentials::new(
        config.access_key_id.trim(),
        config.secret_access_key.trim(),
        None,
        None,
        "mavicms-site-settings",
    );

    aws_sdk_sns::Client::from_conf(
        aws_sdk_sns::Config::builder()
            .behavior_version(aws_sdk_sns::config::BehaviorVersion::latest())
            .region(aws_sdk_sns::config::Region::new(
                config.region.trim().to_string(),
            ))
            .credentials_provider(credentials)
            .build(),
    )
}

/// Every event worth hearing about.
///
/// Not RENDERING_FAILURE or SUBSCRIPTION: the first only happens with
/// Amazon's own templates, which this does not use, and the second is about
/// Amazon's subscription management rather than this site's.
fn wanted_events() -> Vec<aws_sdk_sesv2::types::EventType> {
    use aws_sdk_sesv2::types::EventType;

    vec![
        EventType::Send,
        EventType::Delivery,
        EventType::Bounce,
        EventType::Complaint,
        EventType::Reject,
        EventType::Open,
        EventType::Click,
        EventType::DeliveryDelay,
    ]
}

/// Builds the whole path an event travels, in one call.
///
/// A configuration set so sends can be grouped and measured; Virtual
/// Deliverability Manager on it, which is where Amazon's own analytics come
/// from now; an SNS topic; a subscription pointing back at this site; and an
/// event destination joining them. Every one of these is a screen in the AWS
/// console, and doing them in the wrong order leaves a configuration set that
/// silently measures nothing.
///
/// Safe to run twice: everything here is created only if it is missing.
pub async fn build_event_pipeline(
    config: &EmailConfig,
    set_name: &str,
    endpoint: &str,
) -> AppResult<EventPipeline> {
    use aws_sdk_sesv2::types::{
        DashboardOptions, EventDestinationDefinition, FeatureStatus, GuardianOptions,
        SnsDestination, VdmOptions,
    };

    account_settings(config)?;
    let set_name = set_name.trim();
    if set_name.is_empty() {
        return Err(AppError::Validation(
            "the configuration set needs a name".to_string(),
        ));
    }

    let ses = client_for(config);

    // 1. The configuration set. Already there is not an error — this is
    //    expected to be run again after somebody changes the address.
    let _ = ses
        .create_configuration_set()
        .configuration_set_name(set_name)
        .send()
        .await;

    // 2. Amazon's own deliverability analytics, on that set.
    let _ = ses
        .put_configuration_set_vdm_options()
        .configuration_set_name(set_name)
        .vdm_options(
            VdmOptions::builder()
                .dashboard_options(
                    DashboardOptions::builder()
                        .engagement_metrics(FeatureStatus::Enabled)
                        .build(),
                )
                .guardian_options(
                    GuardianOptions::builder()
                        .optimized_shared_delivery(FeatureStatus::Enabled)
                        .build(),
                )
                .build(),
        )
        .send()
        .await;

    // 3. A topic for the events to be published to.
    let topic = tokio::time::timeout(
        TIMEOUT,
        sns_client(config)
            .create_topic()
            .name(format!("{set_name}-events"))
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SNS did not answer in time".to_string()))?
    .map_err(|err| {
        use aws_sdk_sns::error::ProvideErrorMetadata as _;
        AppError::Validation(format!(
            "SNS would not make the topic: {}",
            err.message().unwrap_or("no reason given")
        ))
    })?;

    let topic_arn = topic.topic_arn().unwrap_or_default().to_string();

    // 4. Point it back here. SNS confirms by posting to the address, which
    //    the endpoint answers by itself.
    let subscription = tokio::time::timeout(
        TIMEOUT,
        sns_client(config)
            .subscribe()
            .topic_arn(&topic_arn)
            .protocol("https")
            .endpoint(endpoint)
            .return_subscription_arn(true)
            .send(),
    )
    .await
    .map_err(|_| AppError::Validation("SNS did not answer in time".to_string()))?
    .map_err(|err| {
        use aws_sdk_sns::error::ProvideErrorMetadata as _;
        AppError::Validation(format!(
            "SNS would not subscribe this site: {}",
            err.message().unwrap_or("no reason given")
        ))
    })?;

    let subscription_arn = subscription
        .subscription_arn()
        .unwrap_or_default()
        .to_string();

    // 5. And join the two. Updating rather than creating when it is already
    //    there, so changing the address does not need the set deleting.
    let destination = EventDestinationDefinition::builder()
        .enabled(true)
        .set_matching_event_types(Some(wanted_events()))
        .sns_destination(
            SnsDestination::builder()
                .topic_arn(&topic_arn)
                .build()
                .map_err(|err| {
                    AppError::Internal(format!("could not describe the topic: {err}"))
                })?,
        )
        .build();

    let made = ses
        .create_configuration_set_event_destination()
        .configuration_set_name(set_name)
        .event_destination_name("mavicms")
        .event_destination(destination.clone())
        .send()
        .await;

    if made.is_err() {
        tokio::time::timeout(
            TIMEOUT,
            ses.update_configuration_set_event_destination()
                .configuration_set_name(set_name)
                .event_destination_name("mavicms")
                .event_destination(destination)
                .send(),
        )
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
        .map_err(|err| refused("connect the events", err))?;
    }

    Ok(EventPipeline {
        configuration_set: set_name.to_string(),
        topic_arn,
        endpoint: endpoint.to_string(),
        // "pending confirmation" until SNS has posted to us and been answered.
        confirmed: !subscription_arn.contains("pending"),
    })
}

/// Amazon's own view of how mail is doing, from Virtual Deliverability
/// Manager — which is where SES keeps this now.
#[derive(Debug, Serialize, ToSchema)]
pub struct Deliverability {
    pub sent: i64,
    pub delivered: i64,
    pub opened: i64,
    pub clicked: i64,
    pub permanent_bounces: i64,
    pub transient_bounces: i64,
    pub complaints: i64,
    /// Of what was sent. Percentages, because that is how they are read.
    pub delivery_rate: f64,
    pub open_rate: f64,
    pub click_rate: f64,
}

/// The last `days` of it.
///
/// VDM has to be switched on for an account before it answers, which is what
/// `build_event_pipeline` does. Until then this is a refusal with a sentence
/// saying so rather than a silent zero.
pub async fn deliverability(config: &EmailConfig, days: i64) -> AppResult<Deliverability> {
    use aws_sdk_sesv2::types::{
        BatchGetMetricDataQuery, Metric, MetricDimensionName, MetricNamespace,
    };

    account_settings(config)?;

    let now = chrono::Utc::now();
    let from = now - chrono::Duration::days(days.clamp(1, 60));
    let as_smithy = |when: chrono::DateTime<chrono::Utc>| {
        aws_smithy_types::DateTime::from_secs(when.timestamp())
    };

    let wanted = [
        ("SEND", Metric::Send),
        ("DELIVERY", Metric::Delivery),
        ("OPEN", Metric::Open),
        ("CLICK", Metric::Click),
        ("PERMANENT_BOUNCE", Metric::PermanentBounce),
        ("TRANSIENT_BOUNCE", Metric::TransientBounce),
        ("COMPLAINT", Metric::Complaint),
    ];

    let mut call = client_for(config).batch_get_metric_data();
    for (id, metric) in &wanted {
        let query = BatchGetMetricDataQuery::builder()
            .id(id.to_lowercase())
            .namespace(MetricNamespace::Vdm)
            .metric(metric.clone())
            .start_date(as_smithy(from))
            .end_date(as_smithy(now))
            // Everything the account sent, not one identity's worth.
            .dimensions(MetricDimensionName::EmailIdentity, "*")
            .build()
            .map_err(|err| AppError::Internal(format!("could not ask for {id}: {err}")))?;
        call = call.queries(query);
    }

    let answered = tokio::time::timeout(TIMEOUT, call.send())
        .await
        .map_err(|_| AppError::Validation("SES did not answer in time".to_string()))?
        .map_err(|err| refused("give the deliverability figures", err))?;

    let total = |id: &str| -> i64 {
        answered
            .results()
            .iter()
            .find(|result| result.id() == Some(id))
            .map(|result| result.values().iter().sum::<i64>())
            .unwrap_or(0)
    };

    let sent = total("send");
    let delivered = total("delivery");
    let opened = total("open");
    let clicked = total("click");

    let of_sent = |count: i64| {
        if sent == 0 {
            0.0
        } else {
            (count as f64 / sent as f64) * 100.0
        }
    };

    Ok(Deliverability {
        sent,
        delivered,
        opened,
        clicked,
        permanent_bounces: total("permanent_bounce"),
        transient_bounces: total("transient_bounce"),
        complaints: total("complaint"),
        delivery_rate: of_sent(delivered),
        open_rate: of_sent(opened),
        click_rate: of_sent(clicked),
    })
}
