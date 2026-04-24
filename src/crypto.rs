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
//! Cryptographic utilities for encrypting sensitive data at rest.
//!
//! This module provides AES-256-GCM encryption for TOTP secrets stored in the database.
//! The encryption key is loaded from the TOTP_ENCRYPTION_KEY environment variable
//! and must be a 64-character hex string (32 bytes).

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::env;
use std::sync::LazyLock;

// The encryption key loaded from environment variable
static ENCRYPTION_KEY: LazyLock<Option<Key<Aes256Gcm>>> = LazyLock::new(load_encryption_key);

/// Load the encryption key from environment variable
fn load_encryption_key() -> Option<Key<Aes256Gcm>> {
    let key_hex = env::var("TOTP_ENCRYPTION_KEY").ok()?;

    let key_bytes = match hex::decode(&key_hex) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to decode TOTP_ENCRYPTION_KEY as hex: {}", e);
            return None;
        }
    };

    if key_bytes.len() != 32 {
        tracing::error!(
            "TOTP_ENCRYPTION_KEY must be 32 bytes (64 hex characters), got {} bytes",
            key_bytes.len()
        );
        return None;
    }

    let key: [u8; 32] = key_bytes.try_into().ok()?;
    Some(Key::<Aes256Gcm>::from(key))
}

/// Validate that the encryption key is configured. Call at startup.
/// Panics with a clear message if TOTP_ENCRYPTION_KEY is missing or invalid.
pub fn validate_encryption_key() {
    let _ = ENCRYPTION_KEY.as_ref().expect(
        "TOTP_ENCRYPTION_KEY environment variable is not set or invalid. \
        It must be a 64-character hex string (32 bytes).",
    );
}

/// Get the encryption key, returning an error if not configured
fn get_encryption_key() -> Result<&'static Key<Aes256Gcm>> {
    ENCRYPTION_KEY.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "TOTP_ENCRYPTION_KEY environment variable is not set or invalid. \
            It must be a 64-character hex string (32 bytes)."
        )
    })
}

/// Encrypt a TOTP secret for storage
///
/// Returns a base64-encoded string containing the nonce and ciphertext.
/// Format: base64(nonce || ciphertext)
///
/// Fails if the encryption key is not configured.
pub fn encrypt_totp_secret(plaintext: &str) -> Result<String> {
    let key = get_encryption_key()?;
    let cipher = Aes256Gcm::new(key);

    // Generate a random 96-bit nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // Encrypt the plaintext
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Combine nonce and ciphertext: nonce (12 bytes) || ciphertext
    let mut combined = Vec::with_capacity(nonce.len() + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);

    // Encode as base64 for storage
    Ok(BASE64.encode(&combined))
}

/// Decrypt a TOTP secret from storage
///
/// Expects a base64-encoded string containing the nonce and ciphertext.
/// Format: base64(nonce || ciphertext)
///
/// Fails if the encryption key is not configured or the data is malformed.
pub fn decrypt_totp_secret(stored_value: &str) -> Result<String> {
    let key = get_encryption_key()?;

    // Decode from base64
    let combined = BASE64
        .decode(stored_value)
        .context("Failed to decode stored TOTP secret as base64")?;

    // Nonce is 12 bytes for AES-256-GCM
    const NONCE_LEN: usize = 12;

    if combined.len() <= NONCE_LEN {
        bail!(
            "Encrypted TOTP secret is too short: expected at least {} bytes, got {}",
            NONCE_LEN + 1,
            combined.len()
        );
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))
        .context("Failed to decrypt TOTP secret - the data may be corrupted or the encryption key may have changed")?;

    String::from_utf8(plaintext).context("Decrypted TOTP secret is not valid UTF-8")
}

/// Encrypt a token for storage (verification tokens, password reset tokens, etc.)
///
/// Returns a base64-encoded string containing the nonce and ciphertext.
/// Format: base64(nonce || ciphertext)
///
/// Fails if the encryption key is not configured.
pub fn encrypt_token(plaintext: &str) -> Result<String> {
    let key = get_encryption_key()?;
    let cipher = Aes256Gcm::new(key);

    // Generate a random 96-bit nonce
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // Encrypt the plaintext
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Token encryption failed: {}", e))?;

    // Combine nonce and ciphertext: nonce (12 bytes) || ciphertext
    let mut combined = Vec::with_capacity(nonce.len() + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);

    // Encode as base64 for storage
    Ok(BASE64.encode(&combined))
}

/// Decrypt a token from storage (verification tokens, password reset tokens, etc.)
///
/// Expects a base64-encoded string containing the nonce and ciphertext.
/// Format: base64(nonce || ciphertext)
///
/// Fails if the encryption key is not configured or the data is malformed.
pub fn decrypt_token(stored_value: &str) -> Result<String> {
    let key = get_encryption_key()?;

    // Decode from base64
    let combined = BASE64
        .decode(stored_value)
        .context("Failed to decode stored token as base64")?;

    // Nonce is 12 bytes for AES-256-GCM
    const NONCE_LEN: usize = 12;

    if combined.len() <= NONCE_LEN {
        bail!(
            "Encrypted token is too short: expected at least {} bytes, got {}",
            NONCE_LEN + 1,
            combined.len()
        );
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Token decryption failed: {}", e))
        .context("Failed to decrypt token - the data may be corrupted or the encryption key may have changed")?;

    String::from_utf8(plaintext).context("Decrypted token is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a new random encryption key (for testing)
    ///
    /// Returns a 64-character hex string suitable for TOTP_ENCRYPTION_KEY
    fn generate_encryption_key() -> String {
        let key = Aes256Gcm::generate_key(OsRng);
        hex::encode(key.as_slice())
    }

    fn setup_test_key() -> String {
        let test_key_hex = generate_encryption_key();
        env::set_var("TOTP_ENCRYPTION_KEY", &test_key_hex);
        test_key_hex
    }

    #[test]
    fn test_generate_key_length() {
        let key = generate_encryption_key();
        assert_eq!(key.len(), 64, "Key should be 64 hex characters (32 bytes)");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let test_key_hex = setup_test_key();

        // Need to manually construct the key since LazyLock is already initialized
        let key_bytes = hex::decode(&test_key_hex).unwrap();
        let key: [u8; 32] = key_bytes.try_into().unwrap();
        let key = Key::<Aes256Gcm>::from(key);

        let secret = "JBSWY3DPEHPK3PXP";

        // Encrypt
        let cipher = Aes256Gcm::new(&key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher.encrypt(&nonce, secret.as_bytes()).unwrap();

        let mut combined = Vec::new();
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(&ciphertext);
        let encrypted = BASE64.encode(&combined);

        // Decrypt
        let decoded = BASE64.decode(&encrypted).unwrap();
        let (nonce_bytes, ct) = decoded.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let decrypted = cipher.decrypt(nonce, ct).unwrap();

        assert_eq!(String::from_utf8(decrypted).unwrap(), secret);
    }

    #[test]
    fn test_decrypt_malformed_data_fails() {
        setup_test_key();

        // Test that invalid base64 fails
        let result = decrypt_totp_secret("not-valid-base64!!!");
        assert!(result.is_err());

        // Test that too-short data fails
        let short_data = BASE64.encode([1, 2, 3, 4, 5]);
        let result = decrypt_totp_secret(&short_data);
        assert!(result.is_err());
    }
}
