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

/// Everything the backup screen needs in one response: the settings, what has
/// been written, and whether the bucket is there to be chosen.
#[derive(Debug, Serialize, ToSchema)]
pub struct BackupSettingsResponse {
    pub enabled: bool,
    pub config: crate::backup::BackupConfig,
    pub backups: Vec<crate::backup::BackupFile>,
    pub s3_available: bool,
    pub s3_bucket: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBackupRequest {
    pub enabled: bool,
    pub config: crate::backup::BackupConfig,
}

/// What somebody needs in a `.env` to run a site's front end on their own
/// machine.
///
/// The question this answers used to be asked by message: a designer clones
/// the project, runs it, gets nothing, and writes to the agency to ask which
/// address and which password. The agency then sends a password over chat,
/// which is the worst possible way for one to travel.
#[derive(Debug, Serialize, ToSchema)]
pub struct DevelopmentResponse {
    /// What CMS_API_URL should be: this site's API, on this site's address.
    pub api_url: String,
    pub site_url: String,
    /// The names of the variables this site's build runs with. Only the names
    /// — the values are encrypted on the server and do not come back, so a
    /// person setting up a machine still has to be told them.
    pub variables: Vec<String>,
    pub tokens: Vec<DevelopmentToken>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DevelopmentToken {
    pub id: String,
    pub created_at: String,
    pub expires_at: String,
}

/// A token, the once. It is not stored anywhere it can be read back.
#[derive(Debug, Serialize, ToSchema)]
pub struct NewDevelopmentToken {
    pub token: String,
    pub expires_at: String,
}
