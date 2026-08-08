//! Where a lesson's video actually lives.
//!
//! Not in `storage.rs` and not in `media.rs`. An uploaded image is at most ten
//! megabytes, is buffered whole in memory on its way through, and is served
//! publicly from `/uploads` on purpose. A one-hour lesson is one to three
//! gigabytes, has to arrive in several bitrates so it plays on a phone on 4G,
//! and its address has to stop working — otherwise the first person to pay for
//! a course can hand it to everyone who did not.
//!
//! So the bytes never come through this server at all. The panel asks for an
//! upload ticket, the browser sends the file straight to whoever is hosting
//! it, and that host tells us when it has finished transcoding. What we keep
//! is a row saying which video this is and how long it runs.
//!
//! Two hosts, chosen because between them they cover both reasons anybody
//! picks one — Bunny because it is the cheapest way to reach Turkey, and
//! Cloudflare because it is the one you never have to think about again.
//! Neither needed a new dependency, which is not why they were chosen but is
//! why Mux is not here yet: Mux signs its playback tokens with an RSA key on
//! our side, and the other two do not.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{AppError, AppResult};

pub const VIDEO_PLUGIN: &str = "video";

/// How long a playback address lives.
///
/// Long enough to watch a lesson twice and short enough that a link pasted
/// into a group chat is dead before most of the group opens it. Sharing is a
/// social problem and this is a social answer; the technical one is DRM, which
/// costs more than it saves for everyone who is not a film studio.
pub const PLAYBACK_TTL: Duration = Duration::from_secs(4 * 60 * 60);

/// Which host this site's videos are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Bunny,
    Cloudflare,
}

impl Host {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_lowercase().as_str() {
            "bunny" => Ok(Host::Bunny),
            "cloudflare" => Ok(Host::Cloudflare),
            other => Err(AppError::Validation(format!(
                "\"{other}\" is not a video host this knows: bunny or cloudflare"
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Host::Bunny => "bunny",
            Host::Cloudflare => "cloudflare",
        }
    }
}

/// Everything either host needs, in one row, because that is the shape the
/// panel's form has and the shape `plugin_settings` stores. Only the fields
/// belonging to the chosen host are ever read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VideoConfig {
    #[serde(default)]
    pub host: String,

    /// Bunny: the video library's numeric id.
    #[serde(default)]
    pub library_id: String,
    /// Bunny: the library's pull zone, `vz-….b-cdn.net`.
    #[serde(default)]
    pub cdn_hostname: String,
    /// Bunny: the library's API key. Secret.
    #[serde(default)]
    pub api_key: String,
    /// Bunny: the library's token authentication key. Secret, and a different
    /// key from the one above — signing a URL and calling the API are separate
    /// permissions there.
    #[serde(default)]
    pub token_key: String,

    /// Cloudflare: the account id.
    #[serde(default)]
    pub account_id: String,
    /// Cloudflare: an API token with Stream read and edit. Secret.
    #[serde(default)]
    pub api_token: String,
    /// Cloudflare: `customer-….cloudflarestream.com`, which the account's
    /// first video reports and which never changes afterwards.
    #[serde(default)]
    pub customer_subdomain: String,

    /// The unguessable part of the address the host posts back to. Made here,
    /// once, the same way the mail plugin makes its own.
    #[serde(default)]
    pub events_token: String,
}

/// Where the browser sends the file, and what to send it with.
#[derive(Debug, Serialize, ToSchema)]
pub struct Ticket {
    /// The host's own id for the video, which the row is keyed on afterwards.
    pub provider_id: String,
    pub upload_url: String,
    /// `tus` where the host supports resuming, `put` where it does not. A
    /// two-gigabyte file on an office connection will be interrupted, and the
    /// difference between the two is whether that costs the whole upload.
    pub method: String,
    /// Sent with the upload. Empty for a one-time URL that carries its own
    /// authorisation, which is the point of one.
    pub headers: Vec<(String, String)>,
}

/// What the host says about a video when asked.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub status: Status,
    pub duration_seconds: i32,
    pub thumbnail_url: String,
    pub bytes: i64,
    pub error: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Uploading,
    Processing,
    Ready,
    Failed,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Uploading => "uploading",
            Status::Processing => "processing",
            Status::Ready => "ready",
            Status::Failed => "failed",
        }
    }
}

/// An address that plays, and the moment it stops.
#[derive(Debug, Serialize, ToSchema)]
pub struct Playback {
    /// An HLS playlist. Every player worth using takes one, and it is what
    /// makes the picture drop rather than the video stop when a phone moves
    /// off wifi.
    pub url: String,
    pub expires_at: String,
    pub thumbnail_url: String,
}

/// What this site has cost so far this month.
///
/// Read from the host rather than counted here: what we could count is what we
/// served, and we serve none of it. Storage is ours to work out — it is the
/// videos we know about — but delivery is only knowable where it happened.
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct Usage {
    pub stored_seconds: i64,
    pub stored_bytes: i64,
    pub delivered_bytes: i64,
    /// Where the host bills by the minute rather than by the byte.
    pub delivered_minutes: i64,
    pub since: String,
}

impl VideoConfig {
    pub fn host(&self) -> AppResult<Host> {
        Host::parse(&self.host)
    }

    /// Everything that must be filled in before this can be switched on.
    pub fn validate(&self) -> AppResult<()> {
        let missing = |what: &str| Err(AppError::Validation(format!("{what} is needed")));

        match self.host()? {
            Host::Bunny => {
                if self.library_id.trim().is_empty() {
                    return missing("the library id");
                }
                if self.api_key.trim().is_empty() {
                    return missing("the library's API key");
                }
                if self.cdn_hostname.trim().is_empty() {
                    return missing("the pull zone hostname");
                }
                if self.token_key.trim().is_empty() {
                    return Err(AppError::Validation(
                        "the token authentication key is needed: without it every video address \
                         works for ever and for anybody"
                            .to_string(),
                    ));
                }
            }
            Host::Cloudflare => {
                if self.account_id.trim().is_empty() {
                    return missing("the account id");
                }
                if self.api_token.trim().is_empty() {
                    return missing("an API token");
                }
            }
        }
        Ok(())
    }

    fn client(&self) -> AppResult<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| AppError::Internal(format!("could not build an HTTP client: {err}")))
    }

    /// Ask, and turn anything that is not a 2xx into the host's own words.
    async fn ok(response: reqwest::Response, doing: &str) -> AppResult<serde_json::Value> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Validation(format!(
                "the video host refused to {doing} ({status}): {}",
                body.chars().take(300).collect::<String>()
            )));
        }
        Ok(serde_json::from_str(&body).unwrap_or(serde_json::Value::Null))
    }

    /// Prove the credentials before anybody uploads two gigabytes against them.
    pub async fn test_connection(&self) -> AppResult<()> {
        self.validate()?;
        let client = self.client()?;

        match self.host()? {
            Host::Bunny => {
                let url = format!(
                    "https://video.bunnycdn.com/library/{}/videos?page=1&itemsPerPage=1",
                    self.library_id.trim()
                );
                let response = client
                    .get(url)
                    .header("AccessKey", self.api_key.trim())
                    .send()
                    .await
                    .map_err(|err| AppError::Validation(format!("could not reach Bunny: {err}")))?;
                Self::ok(response, "answer for that library").await?;
            }
            Host::Cloudflare => {
                let url = format!(
                    "https://api.cloudflare.com/client/v4/accounts/{}/stream?per_page=1",
                    self.account_id.trim()
                );
                let response = client
                    .get(url)
                    .bearer_auth(self.api_token.trim())
                    .send()
                    .await
                    .map_err(|err| {
                        AppError::Validation(format!("could not reach Cloudflare: {err}"))
                    })?;
                Self::ok(response, "answer for that account").await?;
            }
        }
        Ok(())
    }

    /// Make a place for a file and say where to send it.
    pub async fn upload_ticket(&self, title: &str) -> AppResult<Ticket> {
        self.validate()?;
        let client = self.client()?;

        match self.host()? {
            Host::Bunny => {
                let library = self.library_id.trim();
                let made = client
                    .post(format!(
                        "https://video.bunnycdn.com/library/{library}/videos"
                    ))
                    .header("AccessKey", self.api_key.trim())
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(serde_json::json!({ "title": title }).to_string())
                    .send()
                    .await
                    .map_err(|err| AppError::Validation(format!("could not reach Bunny: {err}")))?;

                let body = Self::ok(made, "make a video").await?;
                let id = body
                    .get("guid")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        AppError::Internal("Bunny made a video with no guid".to_string())
                    })?
                    .to_string();

                Ok(Ticket {
                    upload_url: format!("https://video.bunnycdn.com/library/{library}/videos/{id}"),
                    provider_id: id,
                    method: "put".to_string(),
                    headers: vec![("AccessKey".to_string(), self.api_key.trim().to_string())],
                })
            }
            Host::Cloudflare => {
                let account = self.account_id.trim();
                let response = client
                    .post(format!(
                        "https://api.cloudflare.com/client/v4/accounts/{account}/stream/direct_upload"
                    ))
                    .bearer_auth(self.api_token.trim())
                    // Long enough for somebody to pick the file and for the
                    // upload itself; not a URL worth keeping.
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(
                        serde_json::json!({
                            "maxDurationSeconds": 4 * 60 * 60,
                            "expiry": rfc3339_in(Duration::from_secs(2 * 60 * 60)),
                            "meta": { "name": title },
                            "requireSignedURLs": true,
                        })
                        .to_string(),
                    )
                    .send()
                    .await
                    .map_err(|err| {
                        AppError::Validation(format!("could not reach Cloudflare: {err}"))
                    })?;

                let body = Self::ok(response, "make an upload").await?;
                let result = body.get("result").unwrap_or(&serde_json::Value::Null);

                Ok(Ticket {
                    provider_id: result
                        .get("uid")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    upload_url: result
                        .get("uploadURL")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    // The one-time URL is the authorisation. Sending the API
                    // token as well would put it in the browser for nothing.
                    method: "tus".to_string(),
                    headers: Vec::new(),
                })
            }
        }
    }

    /// What the host says about it now.
    pub async fn facts(&self, provider_id: &str) -> AppResult<Facts> {
        let client = self.client()?;

        match self.host()? {
            Host::Bunny => {
                let library = self.library_id.trim();
                let response = client
                    .get(format!(
                        "https://video.bunnycdn.com/library/{library}/videos/{provider_id}"
                    ))
                    .header("AccessKey", self.api_key.trim())
                    .send()
                    .await
                    .map_err(|err| AppError::Validation(format!("could not reach Bunny: {err}")))?;

                let body = Self::ok(response, "describe that video").await?;
                let number = |key: &str| body.get(key).and_then(serde_json::Value::as_i64);

                Ok(Facts {
                    // 0 queued, 1 processing, 2 encoding, 3 finished,
                    // 4 resolution finished, 5 failed. 3 and 4 both play.
                    status: match number("status").unwrap_or(0) {
                        3 | 4 => Status::Ready,
                        5 => Status::Failed,
                        0 => Status::Uploading,
                        _ => Status::Processing,
                    },
                    duration_seconds: number("length").unwrap_or(0) as i32,
                    thumbnail_url: match body.get("thumbnailFileName").and_then(|v| v.as_str()) {
                        Some(name) if !name.is_empty() => {
                            format!("https://{}/{provider_id}/{name}", self.cdn_hostname.trim())
                        }
                        _ => String::new(),
                    },
                    bytes: number("storageSize").unwrap_or(0),
                    error: String::new(),
                })
            }
            Host::Cloudflare => {
                let account = self.account_id.trim();
                let response = client
                    .get(format!(
                        "https://api.cloudflare.com/client/v4/accounts/{account}/stream/{provider_id}"
                    ))
                    .bearer_auth(self.api_token.trim())
                    .send()
                    .await
                    .map_err(|err| {
                        AppError::Validation(format!("could not reach Cloudflare: {err}"))
                    })?;

                let body = Self::ok(response, "describe that video").await?;
                let result = body.get("result").cloned().unwrap_or_default();
                let state = result
                    .pointer("/status/state")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");

                Ok(Facts {
                    status: match state {
                        "ready" => Status::Ready,
                        "error" => Status::Failed,
                        "inprogress" | "queued" => Status::Processing,
                        _ => Status::Uploading,
                    },
                    duration_seconds: result
                        .get("duration")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0)
                        .max(0.0) as i32,
                    thumbnail_url: result
                        .get("thumbnail")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    bytes: result
                        .get("size")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0),
                    error: result
                        .pointer("/status/errorReasonText")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            }
        }
    }

    /// An address that plays, and stops playing.
    ///
    /// Called once per lesson opened, *after* whatever decided the person may
    /// watch it. Nothing here checks that; this only signs.
    pub async fn playback(&self, provider_id: &str, thumbnail: &str) -> AppResult<Playback> {
        let expires = expires_in(PLAYBACK_TTL);

        let url = match self.host()? {
            Host::Bunny => {
                // Bunny signs on our side: the key, the path and the moment it
                // dies, hashed together. No round trip, which matters when it
                // is on the way to every lesson anybody opens.
                let path = format!("/{provider_id}/playlist.m3u8");
                let token = bunny_token(self.token_key.trim(), &path, expires);
                format!(
                    "https://{}{path}?token={token}&expires={expires}",
                    self.cdn_hostname.trim()
                )
            }
            Host::Cloudflare => {
                // Cloudflare will sign it for us, which costs a round trip and
                // saves carrying an RSA key. Worth it: the key would have to
                // be stored, rotated, and got right.
                let account = self.account_id.trim();
                let response = self
                    .client()?
                    .post(format!(
                        "https://api.cloudflare.com/client/v4/accounts/{account}/stream/{provider_id}/token"
                    ))
                    .bearer_auth(self.api_token.trim())
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(serde_json::json!({ "exp": expires }).to_string())
                    .send()
                    .await
                    .map_err(|err| {
                        AppError::Validation(format!("could not reach Cloudflare: {err}"))
                    })?;

                let body = Self::ok(response, "sign that video").await?;
                let token = body
                    .pointer("/result/token")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| AppError::Internal("Cloudflare signed nothing".to_string()))?;

                format!(
                    "https://{}/{token}/manifest/video.m3u8",
                    self.customer_subdomain.trim()
                )
            }
        };

        Ok(Playback {
            url,
            expires_at: chrono::DateTime::from_timestamp(expires, 0)
                .unwrap_or_default()
                .to_rfc3339(),
            thumbnail_url: thumbnail.to_string(),
        })
    }

    /// Take it off the host. Best effort: the row is going either way, and a
    /// video left behind costs storage, not correctness.
    pub async fn remove(&self, provider_id: &str) {
        let Ok(client) = self.client() else { return };
        let _ = match self.host() {
            Ok(Host::Bunny) => {
                client
                    .delete(format!(
                        "https://video.bunnycdn.com/library/{}/videos/{provider_id}",
                        self.library_id.trim()
                    ))
                    .header("AccessKey", self.api_key.trim())
                    .send()
                    .await
            }
            Ok(Host::Cloudflare) => {
                client
                    .delete(format!(
                        "https://api.cloudflare.com/client/v4/accounts/{}/stream/{provider_id}",
                        self.account_id.trim()
                    ))
                    .bearer_auth(self.api_token.trim())
                    .send()
                    .await
            }
            Err(_) => return,
        };
    }
}

/// Bunny's token authentication, which is a SHA-256 of the key, the path and
/// the expiry, base64 in the URL-safe alphabet with the padding taken off.
fn bunny_token(key: &str, path: &str, expires: i64) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(format!("{key}{path}{expires}").as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn expires_in(ttl: Duration) -> i64 {
    chrono::Utc::now().timestamp() + ttl.as_secs() as i64
}

/// The same moment, for the one field Cloudflare wants as a date rather than
/// as a count of seconds.
fn rfc3339_in(ttl: Duration) -> String {
    (chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default()).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_one_of_two_and_says_so_when_it_is_not() {
        assert_eq!(Host::parse("bunny").unwrap(), Host::Bunny);
        assert_eq!(Host::parse("  Cloudflare ").unwrap(), Host::Cloudflare);
        let refused = Host::parse("mux").unwrap_err().to_string();
        assert!(refused.contains("bunny or cloudflare"), "{refused}");
    }

    /// The one that would be silently wrong: a token computed over a different
    /// path than the one requested plays for nobody, and a token computed with
    /// no key at all plays for everybody.
    #[test]
    fn a_bunny_token_is_the_key_the_path_and_the_expiry() {
        let a = bunny_token("secret", "/abc/playlist.m3u8", 1_800_000_000);
        assert_eq!(
            a,
            bunny_token("secret", "/abc/playlist.m3u8", 1_800_000_000)
        );
        assert_ne!(a, bunny_token("other", "/abc/playlist.m3u8", 1_800_000_000));
        assert_ne!(
            a,
            bunny_token("secret", "/def/playlist.m3u8", 1_800_000_000)
        );
        assert_ne!(
            a,
            bunny_token("secret", "/abc/playlist.m3u8", 1_800_000_001)
        );
        // URL-safe and unpadded, because it goes in a query string.
        assert!(
            !a.contains('+') && !a.contains('/') && !a.contains('='),
            "{a}"
        );
    }

    #[test]
    fn switching_it_on_needs_the_key_that_makes_addresses_expire() {
        let mut config = VideoConfig {
            host: "bunny".to_string(),
            library_id: "1".to_string(),
            api_key: "k".to_string(),
            cdn_hostname: "vz-test.b-cdn.net".to_string(),
            ..Default::default()
        };
        let refused = config.validate().unwrap_err().to_string();
        assert!(refused.contains("works for ever"), "{refused}");

        config.token_key = "t".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn cloudflare_needs_an_account_and_a_token() {
        let config = VideoConfig {
            host: "cloudflare".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = VideoConfig {
            host: "cloudflare".to_string(),
            account_id: "a".to_string(),
            api_token: "t".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }
}
