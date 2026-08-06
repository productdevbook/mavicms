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

    /// The address this configuration actually talks to.
    fn effective_endpoint(&self) -> String {
        if self.endpoint.trim().is_empty() {
            format!("https://s3.{}.amazonaws.com", self.region)
        } else {
            self.endpoint.trim_end_matches('/').to_string()
        }
    }

    /// Refuses to talk to an endpoint that is not on the public internet.
    ///
    /// The endpoint is typed in by whoever runs a site, and the server is what
    /// connects to it — so without this, a site could point it at the cluster
    /// and read the first part of whatever answered back out of the error
    /// message. Checked on every call rather than only when it is saved,
    /// because the name behind it is not ours to trust twice.
    pub async fn ensure_reachable(&self) -> AppResult<()> {
        crate::fetch::ensure_public_host(&self.effective_endpoint())
            .await
            .map_err(|err| AppError::Validation(format!("the storage endpoint {err}")))
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

    /// Deletes an object, signing the request here rather than through the S3
    /// client.
    ///
    /// The client signs `DELETE` with `content-length` and `content-type`
    /// among the signed headers but sends neither on a request with no body.
    /// AWS lets that pass; R2 refuses it as a signature mismatch, and the
    /// failure is invisible unless you go looking — every deleted image would
    /// stay in the bucket for ever.
    async fn delete_object(&self, key: &str) -> Result<(), String> {
        use hmac::{Hmac, Mac};
        use sha2::{Digest, Sha256};

        type HmacSha256 = Hmac<Sha256>;

        fn sign(key: &[u8], message: &str) -> Vec<u8> {
            let mut mac = HmacSha256::new_from_slice(key).expect("hmac takes any key length");
            mac.update(message.as_bytes());
            mac.finalize().into_bytes().to_vec()
        }

        let endpoint = if self.endpoint.trim().is_empty() {
            format!("https://s3.{}.amazonaws.com", self.region)
        } else {
            self.endpoint.trim_end_matches('/').to_string()
        };
        let host = endpoint
            .split("://")
            .nth(1)
            .ok_or_else(|| format!("endpoint has no host: {endpoint}"))?
            .trim_end_matches('/')
            .to_string();

        // Path style, matching how the bucket is addressed elsewhere. Each
        // segment is encoded, the separators are not.
        let encoded: String = self
            .object_key(key)
            .split('/')
            .map(|segment| {
                percent_encoding::utf8_percent_encode(segment, percent_encoding::NON_ALPHANUMERIC)
                    .to_string()
                    .replace("%2D", "-")
                    .replace("%2E", ".")
                    .replace("%5F", "_")
                    .replace("%7E", "~")
            })
            .collect::<Vec<_>>()
            .join("/");
        let uri = format!("/{}/{}", self.bucket, encoded);

        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date = now.format("%Y%m%d").to_string();
        let region = if self.region.trim().is_empty() {
            "auto"
        } else {
            self.region.trim()
        };
        let scope = format!("{date}/{region}/s3/aws4_request");
        // The payload is empty, and this is its hash.
        let payload_hash = hex::encode(Sha256::digest(b""));

        let canonical = format!(
            "DELETE\n{uri}\n\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload_hash}"
        );
        let to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        );

        let key_date = sign(format!("AWS4{}", self.secret_access_key).as_bytes(), &date);
        let key_region = sign(&key_date, region);
        let key_service = sign(&key_region, "s3");
        let key_signing = sign(&key_service, "aws4_request");
        let signature = hex::encode(sign(&key_signing, &to_sign));

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}",
            self.access_key_id
        );

        let response = reqwest::Client::new()
            .delete(format!("{endpoint}{uri}"))
            .header("host", &host)
            .header("x-amz-content-sha256", &payload_hash)
            .header("x-amz-date", &amz_date)
            .header("authorization", authorization)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let status = response.status();
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "{status}: {}",
                body.chars().take(200).collect::<String>()
            ));
        }
        Ok(())
    }

    /// The objects under a prefix: key, size and when they were last written.
    /// Used to show what backups exist and to drop the ones past their keep.
    pub async fn list(
        &self,
        prefix: &str,
    ) -> AppResult<Vec<(String, u64, chrono::DateTime<chrono::FixedOffset>)>> {
        self.ensure_reachable().await?;
        let bucket = self.bucket()?;
        let results = bucket
            .list(self.object_key(prefix), None)
            .await
            .map_err(|err| AppError::Validation(format!("could not list the bucket: {err}")))?;

        Ok(results
            .into_iter()
            .flat_map(|page| page.contents)
            .map(|object| {
                let modified = chrono::DateTime::parse_from_rfc3339(&object.last_modified)
                    .unwrap_or_else(|_| chrono::Utc::now().fixed_offset());
                (object.key, object.size, modified)
            })
            .collect())
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
        self.ensure_reachable().await?;
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
                config.ensure_reachable().await?;
                let bucket = config.bucket()?;
                bucket
                    .put_object_with_content_type(config.object_key(key), bytes, mime_type)
                    .await
                    .map_err(|err| AppError::Internal(format!("failed to upload to S3: {err}")))?;
                Ok(config.public_url(key))
            }
        }
    }

    /// Best-effort removal — a missing object should not block deleting the
    /// database row the user actually asked to remove.
    /// Removes the stored file. A failure here leaves an orphan that nobody
    /// will ever look for again, so it is reported rather than swallowed —
    /// including the case where the bucket cannot even be built.
    /// Reads a stored file back. `None` when it is not there any more — a
    /// backup skips what it cannot find rather than failing outright.
    pub async fn read(&self, key: &str) -> Option<Vec<u8>> {
        match self {
            MediaStorage::Local { root } => tokio::fs::read(root.join(key)).await.ok(),
            MediaStorage::S3(config) => {
                config.ensure_reachable().await.ok()?;
                let bucket = config.bucket().ok()?;
                let response = bucket.get_object(config.object_key(key)).await.ok()?;
                (response.status_code() < 300).then(|| response.bytes().to_vec())
            }
        }
    }

    pub async fn delete(&self, key: &str) {
        match self {
            MediaStorage::Local { root } => {
                if let Err(err) = tokio::fs::remove_file(root.join(key)).await
                    && err.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(error = %err, key, "failed to delete media file");
                }
            }
            MediaStorage::S3(config) => {
                if let Err(err) = config.ensure_reachable().await {
                    tracing::warn!(error = %err, key, "refused to reach the storage endpoint");
                    return;
                }
                if let Err(err) = config.delete_object(key).await {
                    tracing::warn!(error = %err, key, "failed to delete object");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(endpoint: &str) -> S3Config {
        S3Config {
            endpoint: endpoint.to_string(),
            region: "auto".to_string(),
            bucket: "kova".to_string(),
            access_key_id: "x".to_string(),
            secret_access_key: "y".to_string(),
            public_base_url: "https://example.invalid".to_string(),
            path_prefix: String::new(),
        }
    }

    #[tokio::test]
    async fn the_storage_endpoint_cannot_point_inside() {
        // The addresses that make this server a way to reach its own cluster.
        for endpoint in [
            "http://127.0.0.1:8080",
            "http://169.254.169.254",
            "http://10.0.0.5",
            "http://[::1]:9000",
            "http://192.168.1.10",
        ] {
            assert!(
                at(endpoint).ensure_reachable().await.is_err(),
                "{endpoint} should be refused"
            );
        }
    }

    #[tokio::test]
    async fn an_ordinary_endpoint_is_allowed() {
        assert!(
            at("https://s3.eu-central-1.amazonaws.com")
                .ensure_reachable()
                .await
                .is_ok()
        );
    }
}
