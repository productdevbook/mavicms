use std::{fs, path::Path};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, Key, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::error::AppError;

/// Where the generated master key is persisted when `MAVICMS_SECRET_KEY` is
/// not supplied, mirroring how `config.rs` persists the database url.
const SECRET_KEY_FILE: &str = "secret_key";
const NONCE_LEN: usize = 12;

/// Encrypts and decrypts the credentials that plugins need to keep in a
/// *recoverable* form — an S3 secret key has to be replayable to sign
/// requests, so unlike a password it cannot simply be hashed.
pub struct SecretBox {
    cipher: Aes256Gcm,
}

impl SecretBox {
    /// Takes the master key from `MAVICMS_SECRET_KEY` (base64, 32 bytes) when
    /// set, so operators can manage it as a real secret. Otherwise generates
    /// one and persists it next to the rest of the app's state, owner-readable
    /// only.
    pub fn load_or_create(data_dir: &Path) -> Result<Self, String> {
        let key_bytes = match std::env::var("MAVICMS_SECRET_KEY") {
            Ok(value) if !value.trim().is_empty() => BASE64
                .decode(value.trim())
                .map_err(|err| format!("MAVICMS_SECRET_KEY is not valid base64: {err}"))?,
            _ => Self::key_from_disk(data_dir)?,
        };

        if key_bytes.len() != 32 {
            return Err(format!(
                "master key must be 32 bytes, got {}",
                key_bytes.len()
            ));
        }

        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|err| format!("invalid master key: {err}"))?;
        Ok(Self { cipher })
    }

    fn key_from_disk(data_dir: &Path) -> Result<Vec<u8>, String> {
        let path = data_dir.join(SECRET_KEY_FILE);

        if let Ok(existing) = fs::read_to_string(&path) {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                return BASE64
                    .decode(trimmed)
                    .map_err(|err| format!("stored master key is corrupt: {err}"));
            }
        }

        let key = Key::<Aes256Gcm>::generate();
        let encoded = BASE64.encode(key);
        fs::write(&path, &encoded).map_err(|err| format!("failed to store master key: {err}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }

        tracing::info!("generated a new master key at {}", path.display());
        Ok(key.to_vec())
    }

    /// Returns `base64(nonce ‖ ciphertext‖tag)`.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, AppError> {
        let nonce = Nonce::generate();
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| AppError::Internal("failed to encrypt settings".to_string()))?;

        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        Ok(BASE64.encode(combined))
    }

    pub fn decrypt(&self, token: &str) -> Result<String, AppError> {
        // A master key that changed (or a restored backup missing the key file)
        // surfaces here. It is recoverable by re-entering the credentials, so
        // this stays a 400-class error rather than a crash.
        let reenter = || {
            AppError::Validation(
                "stored credentials could not be decrypted — re-enter them".to_string(),
            )
        };

        let combined = BASE64.decode(token).map_err(|_| reenter())?;
        if combined.len() <= NONCE_LEN {
            return Err(reenter());
        }

        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce_bytes).map_err(|_| reenter())?;
        let plaintext = self
            .cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| reenter())?;

        String::from_utf8(plaintext).map_err(|_| reenter())
    }
}
