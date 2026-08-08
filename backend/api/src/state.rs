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
    /// The control plane, when this is one of the sites being hosted.
    ///
    /// A site needs it to find out whether the server lends it an account to
    /// send mail with — and that answer cannot live in the site's own schema,
    /// because the credentials behind it are not the site's to hold. `None` on
    /// the server's own installation, which is the control plane.
    pub control: Option<DatabaseConnection>,
    /// The key those shared settings are encrypted with, which is the
    /// server's own and not this site's.
    pub control_secrets: Option<Arc<SecretBox>>,
    /// Which site this is, in the control plane's terms. `None` on the
    /// server's own installation.
    pub tenant_id: Option<uuid::Uuid>,
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
