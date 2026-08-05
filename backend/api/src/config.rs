use std::{env, fs, path::PathBuf};

/// Where a database url configured via the setup wizard is persisted, so it
/// survives the self-restart triggered by `POST /setup/database`.
const DATABASE_URL_FILE: &str = "database_url";

pub struct Config {
    /// `None` means no database has been configured yet (no `DATABASE_URL`
    /// env var and no persisted `database_url` file) — the server boots in
    /// setup-only mode until `POST /setup/database` configures one.
    pub database_url: Option<String>,
    pub data_dir: PathBuf,
    pub media_root: PathBuf,
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir =
            PathBuf::from(env::var("MAVICMS_DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
        fs::create_dir_all(&data_dir).expect("failed to create data directory");

        let media_root = env::var("MEDIA_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("media"));
        fs::create_dir_all(&media_root).expect("failed to create media directory");

        let database_url = env::var("DATABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                fs::read_to_string(data_dir.join(DATABASE_URL_FILE))
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8080);

        Self {
            database_url,
            data_dir,
            media_root,
            host,
            port,
        }
    }
}
