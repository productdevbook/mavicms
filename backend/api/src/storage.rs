use std::path::PathBuf;

use s3::{Bucket, Region, creds::Credentials};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const LOCAL: &str = "local";
pub const S3: &str = "s3";

/// Persisted (encrypted) configuration for the S3 storage plugin.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct S3Config {
    /// Custom endpoint for S3-compatible providers (R2, MinIO, Spaces).
    /// Empty means real AWS S3.
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    /// Where readers fetch the objects from — the bucket's public URL or the
    /// CDN in front of it. Stored in each post's HTML, so it must be stable.
    #[serde(default)]
    pub public_base_url: String,
    #[serde(default)]
    pub path_prefix: String,
}

impl S3Config {
    pub fn validate(&self) -> AppResult<()> {
        for (value, name) in [
            (&self.region, "region"),
            (&self.bucket, "bucket"),
            (&self.access_key_id, "access key id"),
            (&self.secret_access_key, "secret access key"),
            (&self.public_base_url, "public base URL"),
        ] {
            if value.trim().is_empty() {
                return Err(AppError::Validation(format!("{name} is required")));
            }
        }

        // This value is concatenated into image `src` attributes, so anything
        // other than a plain http(s) prefix would be a script-injection vector.
        if !(self.public_base_url.starts_with("https://")
            || self.public_base_url.starts_with("http://"))
        {
            return Err(AppError::Validation(
                "public base URL must start with http:// or https://".to_string(),
            ));
        }
        if !(self.endpoint.is_empty()
            || self.endpoint.starts_with("https://")
            || self.endpoint.starts_with("http://"))
        {
            return Err(AppError::Validation(
                "endpoint must start with http:// or https://".to_string(),
            ));
        }
        if self.path_prefix.contains("..") {
            return Err(AppError::Validation(
                "path prefix must not contain ..".to_string(),
            ));
        }

        Ok(())
    }

    fn bucket(&self) -> AppResult<Box<Bucket>> {
        let credentials = Credentials::new(
            Some(&self.access_key_id),
            Some(&self.secret_access_key),
            None,
            None,
            None,
        )
        .map_err(|err| AppError::Validation(format!("invalid S3 credentials: {err}")))?;

        let region = if self.endpoint.trim().is_empty() {
            self.region
                .parse()
                .map_err(|_| AppError::Validation(format!("unknown region: {}", self.region)))?
        } else {
            Region::Custom {
                region: self.region.clone(),
                endpoint: self.endpoint.trim_end_matches('/').to_string(),
            }
        };

        let bucket = Bucket::new(&self.bucket, region, credentials)
            .map_err(|err| AppError::Validation(format!("invalid S3 configuration: {err}")))?;
        // Most S3-compatible providers (R2, MinIO) expect path-style requests.
        Ok(bucket.with_path_style())
    }

    fn object_key(&self, key: &str) -> String {
        let prefix = self.path_prefix.trim_matches('/');
        if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}/{key}")
        }
    }

    fn public_url(&self, key: &str) -> String {
        format!(
            "{}/{}",
            self.public_base_url.trim_end_matches('/'),
            self.object_key(key)
        )
    }

    /// Round-trips a small object so the credentials are checked for write
    /// access, not just connectivity.
    pub async fn test_connection(&self) -> AppResult<()> {
        let bucket = self.bucket()?;
        let key = self.object_key(".mavicms-connection-test");

        bucket
            .put_object_with_content_type(&key, b"mavicms", "text/plain")
            .await
            .map_err(|err| AppError::Validation(format!("could not write to bucket: {err}")))?;
        bucket
            .delete_object(&key)
            .await
            .map_err(|err| AppError::Validation(format!("could not delete from bucket: {err}")))?;

        Ok(())
    }
}

/// Where uploaded media actually lives.
pub enum MediaStorage {
    Local { root: PathBuf },
    S3(Box<S3Config>),
}

impl MediaStorage {
    pub fn name(&self) -> &'static str {
        match self {
            MediaStorage::Local { .. } => LOCAL,
            MediaStorage::S3(_) => S3,
        }
    }

    /// Stores `bytes` under `key` and returns the URL browsers should use.
    pub async fn put(&self, key: &str, bytes: &[u8], mime_type: &str) -> AppResult<String> {
        match self {
            MediaStorage::Local { root } => {
                let path = root.join(key);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|err| {
                        AppError::Internal(format!("failed to create media directory: {err}"))
                    })?;
                }
                tokio::fs::write(&path, bytes)
                    .await
                    .map_err(|err| AppError::Internal(format!("failed to save file: {err}")))?;
                Ok(format!("/uploads/{key}"))
            }
            MediaStorage::S3(config) => {
                let bucket = config.bucket()?;
                bucket
                    .put_object_with_content_type(config.object_key(key), bytes, mime_type)
                    .await
                    .map_err(|err| {
                        AppError::Internal(format!("failed to upload to S3: {err}"))
                    })?;
                Ok(config.public_url(key))
            }
        }
    }

    /// Best-effort removal — a missing object should not block deleting the
    /// database row the user actually asked to remove.
    pub async fn delete(&self, key: &str) {
        match self {
            MediaStorage::Local { root } => {
                let _ = tokio::fs::remove_file(root.join(key)).await;
            }
            MediaStorage::S3(config) => {
                if let Ok(bucket) = config.bucket()
                    && let Err(err) = bucket.delete_object(config.object_key(key)).await
                {
                    tracing::warn!(error = %err, "failed to delete object from S3");
                }
            }
        }
    }
}
