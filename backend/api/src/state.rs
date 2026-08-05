use std::{path::PathBuf, sync::Arc};

use sea_orm::DatabaseConnection;

use crate::{crypto::SecretBox, error::AppError};

#[derive(Clone)]
pub struct AppState {
    /// `None` until the setup wizard configures a database (see `config.rs`).
    pub db: Option<DatabaseConnection>,
    pub data_dir: PathBuf,
    pub media_root: PathBuf,
    pub secrets: Arc<SecretBox>,
}

impl AppState {
    /// Access the database from a route protected by `require_database`,
    /// which guarantees `db` is `Some` before the handler ever runs.
    pub fn db(&self) -> &DatabaseConnection {
        self.db
            .as_ref()
            .expect("require_database middleware ensures the database is configured")
    }

    /// Access the database from an unprotected route (setup) that must
    /// handle the not-yet-configured case itself rather than panicking.
    pub fn db_or_unavailable(&self) -> Result<&DatabaseConnection, AppError> {
        self.db
            .as_ref()
            .ok_or_else(|| AppError::Unavailable("database is not configured yet".to_string()))
    }
}
