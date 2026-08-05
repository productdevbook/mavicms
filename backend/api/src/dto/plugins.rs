use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// Whether it has ever been configured, so the UI can distinguish
    /// "switched off" from "never set up".
    pub configured: bool,
}

/// S3 settings as returned to the panel. The secret access key is
/// deliberately absent — it never leaves the server once stored.
#[derive(Debug, Serialize, ToSchema)]
pub struct S3SettingsResponse {
    pub enabled: bool,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub public_base_url: String,
    pub path_prefix: String,
    pub has_secret_access_key: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct S3SettingsRequest {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub access_key_id: String,
    /// Left empty to keep the stored secret unchanged.
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub public_base_url: String,
    #[serde(default)]
    pub path_prefix: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConnectionTestResponse {
    pub ok: bool,
    pub message: String,
}
