//! Sending through somebody's own mail account.
//!
//! A site that has SES set up sends through it and needs none of this. A site
//! that has a mailbox at its hosting company, or a Google Workspace account,
//! has an SMTP server and a password and nothing else — and telling that
//! customer to open an AWS account before their contact form can email them is
//! not an answer.

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::header::ContentType,
    transport::smtp::{authentication::Credentials, client::Tls, client::TlsParameters},
};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// What a site is asked for, and what is kept encrypted.
///
/// `password` never comes back out of the API; the panel is told whether one
/// is stored, in the same way it is told about an S3 secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpAccount {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// What the reader sees in the From line. Most servers refuse to send as
    /// an address the account does not own, so this is usually the username.
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
    /// Port 465 is TLS from the first byte; 587 starts in the clear and
    /// upgrades. Getting this the wrong way round is the usual reason an
    /// otherwise correct account will not connect, so it is asked rather than
    /// guessed — though the default follows the port.
    #[serde(default)]
    pub implicit_tls: Option<bool>,
}

impl SmtpAccount {
    fn wrapped_from_the_start(&self) -> bool {
        self.implicit_tls.unwrap_or(self.port == 465)
    }
}

/// Checks what can be checked without talking to anybody.
pub fn looks_usable(account: &SmtpAccount) -> AppResult<()> {
    if account.host.trim().is_empty() {
        return Err(AppError::Validation(
            "an SMTP account needs a server".to_string(),
        ));
    }
    if account.port == 0 {
        return Err(AppError::Validation("port 0 is not a port".to_string()));
    }
    if account.from_address.trim().is_empty() {
        return Err(AppError::Validation(
            "an SMTP account needs an address to send from".to_string(),
        ));
    }
    Ok(())
}

fn transport(account: &SmtpAccount) -> AppResult<AsyncSmtpTransport<Tokio1Executor>> {
    let host = account.host.trim();
    let tls = TlsParameters::new(host.to_string())
        .map_err(|err| AppError::Validation(format!("could not set up TLS for {host}: {err}")))?;

    let builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        .port(account.port)
        // Never `Tls::None`: a password over a plain connection is a password
        // read by whoever is between here and there. A server that offers
        // neither is a server this refuses to use.
        .tls(if account.wrapped_from_the_start() {
            Tls::Wrapper(tls)
        } else {
            Tls::Required(tls)
        });

    let builder = if account.username.trim().is_empty() {
        builder
    } else {
        builder.credentials(Credentials::new(
            account.username.trim().to_string(),
            account.password.clone(),
        ))
    };

    Ok(builder.build())
}

/// Sends one message and says what the server said.
pub async fn send(
    account: &SmtpAccount,
    to: &str,
    subject: &str,
    body: &str,
    html: bool,
) -> AppResult<String> {
    looks_usable(account)?;

    let from = if account.from_name.trim().is_empty() {
        account.from_address.trim().to_string()
    } else {
        format!(
            "{} <{}>",
            account.from_name.trim(),
            account.from_address.trim()
        )
    };

    let message = Message::builder()
        .from(from.parse().map_err(|err| {
            AppError::Validation(format!("{from} is not an address this can send as: {err}"))
        })?)
        .to(to.trim().parse().map_err(|err| {
            AppError::Validation(format!("{to} is not an address to send to: {err}"))
        })?)
        .subject(subject)
        .header(if html {
            ContentType::TEXT_HTML
        } else {
            ContentType::TEXT_PLAIN
        })
        .body(body.to_string())
        .map_err(|err| AppError::Validation(format!("could not build the message: {err}")))?;

    let sent = transport(account)?
        .send(message)
        .await
        // The server's own words: "535 Authentication failed" is actionable
        // and "could not send" is not.
        .map_err(|err| AppError::Validation(format!("the mail server refused it: {err}")))?;

    Ok(sent
        .message()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::{SmtpAccount, looks_usable};

    fn account() -> SmtpAccount {
        SmtpAccount {
            host: "mail.example.invalid".to_string(),
            port: 587,
            username: "biri@example.invalid".to_string(),
            password: "gizli".to_string(),
            from_address: "biri@example.invalid".to_string(),
            from_name: String::new(),
            implicit_tls: None,
        }
    }

    #[test]
    fn the_port_decides_which_kind_of_tls_unless_somebody_says() {
        let mut held = account();
        assert!(!held.wrapped_from_the_start());
        held.port = 465;
        assert!(held.wrapped_from_the_start());
        // And saying so wins over the port, because plenty of servers listen
        // for one on the other's number.
        held.implicit_tls = Some(false);
        assert!(!held.wrapped_from_the_start());
    }

    #[test]
    fn an_account_missing_what_it_needs_is_refused_before_anybody_is_dialled() {
        assert!(looks_usable(&account()).is_ok());

        let mut no_host = account();
        no_host.host = "  ".to_string();
        assert!(looks_usable(&no_host).is_err());

        let mut no_from = account();
        no_from.from_address = String::new();
        assert!(looks_usable(&no_from).is_err());

        let mut no_port = account();
        no_port.port = 0;
        assert!(looks_usable(&no_port).is_err());
    }
}
