use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whether the site has completed first-run setup yet.
#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatusResponse {
    pub database_configured: bool,
    pub installed: bool,
    pub site_title: Option<String>,
    /// Whether this address is the server itself rather than one of the sites
    /// it hosts — which is what decides whether the sign-in page offers the
    /// agency console and a way through to somebody else's site.
    pub server: bool,
}

/// First-run setup payload: site info plus the initial administrator account.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupRequest {
    pub site_title: String,
    #[serde(default)]
    pub tagline: String,
    #[serde(default = "default_locale")]
    pub locale: String,
    pub admin_username: String,
    pub admin_email: String,
    pub admin_password: String,
}

fn default_locale() -> String {
    "en".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupResponse {
    pub site_title: String,
    pub admin_username: String,
}

/// Database connection payload: either a full connection url, or the parts
/// to build one from. `url` takes precedence when both are present.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct SetupDatabaseRequest {
    #[serde(default)]
    pub url: Option<String>,
    /// "postgres" | "mysql" | "sqlite"
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    /// Database name, or the file path when `engine` is "sqlite".
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupDatabaseResponse {
    pub database_configured: bool,
}
