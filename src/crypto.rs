/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
//! AES-256-GCM encryption for sensitive data at rest.
//!
//! Covers TOTP secrets, single-use tokens (email verification, password
//! reset), and encrypted `settings` rows. One symmetric key is resolved
//! from `SECURE_VALUES_KEY` through the [`crate::secret`] file-path chain
//! at startup and cached in a `LazyLock` for the process lifetime.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::LazyLock;

const SECRET_NAME: &str = "SECURE_VALUES_KEY";

static ENCRYPTION_KEY: LazyLock<Option<Key<Aes256Gcm>>> = LazyLock::new(load_encryption_key);

fn load_encryption_key() -> Option<Key<Aes256Gcm>> {
    let key_hex = crate::secret::load(SECRET_NAME)?;

    let key_bytes = match hex::decode(key_hex.as_bytes()) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to decode {} as hex: {}", SECRET_NAME, e);
            return None;
        }
    };

    let key: [u8; 32] = match key_bytes.try_into() {
        Ok(key) => key,
        Err(bytes) => {
            tracing::error!(
                "{} must be 32 bytes (64 hex characters), got {} bytes",
                SECRET_NAME,
                bytes.len()
            );
            return None;
        }
    };

    Some(Key::<Aes256Gcm>::from(key))
}

/// Force the encryption-key `LazyLock` to initialize and panic if the key
/// is missing or malformed. Call at startup so a misconfigured key fails
/// fast rather than on the first encrypt/decrypt.
pub fn validate_encryption_key() {
    let _ = ENCRYPTION_KEY.as_ref().expect(
        "SECURE_VALUES_KEY is not set or invalid. \
        It must be a 64-character hex string (32 bytes).",
    );
}

fn get_encryption_key() -> Result<&'static Key<Aes256Gcm>> {
    ENCRYPTION_KEY.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "SECURE_VALUES_KEY is not set or invalid. \
            It must be a 64-character hex string (32 bytes)."
        )
    })
}

/// Deterministic, non-reversible token for a phone number, used as the dedupe
/// key for the verification cache. HMAC-SHA256 keyed by `SECURE_VALUES_KEY` (the
/// same secret as the encryption key, with a domain-separation label) so it
/// cannot be brute-forced from the DB the way a bare hash of a low-entropy phone
/// number could. The same `e164` always maps to the same token.
pub fn hmac_phone(e164: &str) -> Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let key = get_encryption_key()?;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_slice())
        .map_err(|e| anyhow::anyhow!("HMAC init failed: {}", e))?;
    mac.update(b"phone-verify:");
    mac.update(e164.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn encrypt_bytes(plaintext: &[u8]) -> Result<String> {
    let key = get_encryption_key()?;
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut combined = Vec::with_capacity(nonce.len() + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

fn decrypt_bytes(stored_value: &str, label: &str) -> Result<Vec<u8>> {
    let key = get_encryption_key()?;

    let combined = BASE64
        .decode(stored_value)
        .with_context(|| format!("Failed to decode stored {} as base64", label))?;

    const NONCE_LEN: usize = 12;

    if combined.len() <= NONCE_LEN {
        bail!(
            "Encrypted {} is too short: expected at least {} bytes, got {}",
            label,
            NONCE_LEN + 1,
            combined.len()
        );
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))
        .with_context(|| {
            format!(
                "Failed to decrypt {} - the data may be corrupted or the encryption key may have changed",
                label
            )
        })
}

/// Encrypt a TOTP secret for storage as base64(nonce || ciphertext).
pub fn encrypt_totp_secret(plaintext: &str) -> Result<String> {
    encrypt_bytes(plaintext.as_bytes())
}

/// Decrypt a TOTP secret stored as base64(nonce || ciphertext).
pub fn decrypt_totp_secret(stored_value: &str) -> Result<String> {
    let plaintext = decrypt_bytes(stored_value, "TOTP secret")?;
    String::from_utf8(plaintext).context("Decrypted TOTP secret is not valid UTF-8")
}

/// Encrypt a single-use token (verification, password reset) for storage.
pub fn encrypt_token(plaintext: &str) -> Result<String> {
    encrypt_bytes(plaintext.as_bytes())
}

/// Decrypt a single-use token from storage.
pub fn decrypt_token(stored_value: &str) -> Result<String> {
    let plaintext = decrypt_bytes(stored_value, "token")?;
    String::from_utf8(plaintext).context("Decrypted token is not valid UTF-8")
}

/// Encrypt a settings value for storage in `settings.value` when the
/// row is marked `encrypted = true`.
pub fn encrypt_value(plaintext: &str) -> Result<String> {
    encrypt_bytes(plaintext.as_bytes())
}

/// Decrypt a settings value previously produced by `encrypt_value`.
pub fn decrypt_value(stored_value: &str) -> Result<String> {
    let plaintext = decrypt_bytes(stored_value, "setting value")?;
    String::from_utf8(plaintext).context("Decrypted setting value is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_encryption_key() -> String {
        let key = Aes256Gcm::generate_key(OsRng);
        hex::encode(key.as_slice())
    }

    #[test]
    fn test_generate_key_length() {
        let key = generate_encryption_key();
        assert_eq!(key.len(), 64, "Key should be 64 hex characters (32 bytes)");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // Constructs its own cipher and never touches the `ENCRYPTION_KEY`
        // LazyLock, so no key wiring is needed. The malformed-input paths
        // through the public API (which depend on a loaded key) are covered
        // in `tests/crypto_key_file_tests.rs`.
        let test_key_hex = generate_encryption_key();

        let key_bytes = hex::decode(&test_key_hex).unwrap();
        let key: [u8; 32] = key_bytes.try_into().unwrap();
        let key = Key::<Aes256Gcm>::from(key);

        let secret = "JBSWY3DPEHPK3PXP";

        let cipher = Aes256Gcm::new(&key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, secret.as_bytes()).unwrap();

        let mut combined = Vec::new();
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(&ciphertext);
        let encrypted = BASE64.encode(&combined);

        let decoded = BASE64.decode(&encrypted).unwrap();
        let (nonce_bytes, ct) = decoded.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let decrypted = cipher.decrypt(nonce, ct).unwrap();

        assert_eq!(String::from_utf8(decrypted).unwrap(), secret);
    }
}
