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
//! reset), and encrypted `settings` rows. One symmetric key is loaded from
//! the `SECURE_VALUES_KEY` environment variable at startup and cached in
//! a `LazyLock`. After the runtime forces the lock, the env var is wiped
//! so later code and crash-time `/proc/self/environ` dumps can't read it.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::env;
use std::sync::LazyLock;

const ENV_VAR: &str = "SECURE_VALUES_KEY";

static ENCRYPTION_KEY: LazyLock<Option<Key<Aes256Gcm>>> = LazyLock::new(load_encryption_key);

fn load_encryption_key() -> Option<Key<Aes256Gcm>> {
    let key_hex = env::var(ENV_VAR).ok()?;

    let key_bytes = match hex::decode(&key_hex) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to decode {} as hex: {}", ENV_VAR, e);
            return None;
        }
    };

    if key_bytes.len() != 32 {
        tracing::error!(
            "{} must be 32 bytes (64 hex characters), got {} bytes",
            ENV_VAR,
            key_bytes.len()
        );
        return None;
    }

    let key: [u8; 32] = key_bytes.try_into().ok()?;
    Some(Key::<Aes256Gcm>::from(key))
}

/// Force the encryption-key `LazyLock` to initialize and panic if the key
/// is missing or malformed. Call at startup before any tokio task is
/// spawned, then call `wipe_encryption_key_from_env` to remove the var
/// from the process environment.
pub fn validate_encryption_key() {
    let _ = ENCRYPTION_KEY.as_ref().expect(
        "SECURE_VALUES_KEY environment variable is not set or invalid. \
        It must be a 64-character hex string (32 bytes).",
    );
}

/// Remove the encryption key from the process environment after the
/// `LazyLock` has cached it. This raises the bar against accidental
/// disclosure: a later `env`-dumping log line, a crash dump that
/// includes `/proc/self/environ`, or a sub-process that inherits the
/// parent environment will no longer expose the key. It does not
/// defend against in-process memory disclosure: the key is still
/// resident in `ENCRYPTION_KEY` for the rest of the process.
pub fn wipe_encryption_key_from_env() {
    // SAFETY: `env::remove_var` is unsafe because reads and writes of
    // the C `environ` block from other threads are not synchronized
    // against it. This helper is a synchronous, non-yielding call
    // invoked from the startup path in `main.rs` before
    // `AppState::new()` runs: no axum workers, sqlx pool, or
    // tokio::spawn task exists yet, so no other thread is currently
    // in a `getenv`/`setenv` call. Tokio's multi-thread runtime has
    // already spawned idle worker threads but they are parked, not
    // executing user code. Preserving this property requires that the
    // call site stay before `AppState::new()`; moving it after AppState
    // creation would break the contract.
    unsafe {
        env::remove_var(ENV_VAR);
    }
}

fn get_encryption_key() -> Result<&'static Key<Aes256Gcm>> {
    ENCRYPTION_KEY.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "SECURE_VALUES_KEY environment variable is not set or invalid. \
            It must be a 64-character hex string (32 bytes)."
        )
    })
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
    use std::sync::Mutex;

    /// Serializes env mutation across this module's tests. Cargo runs
    /// `#[test]` functions on multiple threads in the same process by
    /// default, so two `set_var` calls would otherwise race on the C
    /// `environ` block and corrupt the array. Other threads in the
    /// test binary (the harness, panic-time backtrace machinery) are
    /// not coordinated by this mutex; we mitigate by keeping env
    /// mutation rare and confined to this module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn generate_encryption_key() -> String {
        let key = Aes256Gcm::generate_key(OsRng);
        hex::encode(key.as_slice())
    }

    fn setup_test_key() -> String {
        let test_key_hex = generate_encryption_key();
        let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
        // SAFETY: `_guard` holds `ENV_LOCK` for the duration of the
        // mutation, so no other test in this binary is concurrently
        // in `set_var`/`remove_var`. No library code in the unit-test
        // binary reads env from another thread between the lock acq
        // and release.
        unsafe {
            env::set_var("SECURE_VALUES_KEY", &test_key_hex);
        }
        test_key_hex
    }

    #[test]
    fn test_generate_key_length() {
        let key = generate_encryption_key();
        assert_eq!(key.len(), 64, "Key should be 64 hex characters (32 bytes)");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // This test constructs its own cipher and never touches the
        // `ENCRYPTION_KEY` LazyLock, so no env mutation is needed.
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

    #[test]
    fn test_decrypt_malformed_data_fails() {
        setup_test_key();

        let result = decrypt_totp_secret("not-valid-base64!!!");
        assert!(result.is_err());

        let short_data = BASE64.encode([1, 2, 3, 4, 5]);
        let result = decrypt_totp_secret(&short_data);
        assert!(result.is_err());
    }
}
