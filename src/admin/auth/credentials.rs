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
//! Argon2 password hashing and verification, plus the session-hash and
//! verification-token helpers used by the local-password auth flow.

use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::RngExt;
use std::sync::LazyLock;
use uuid::Uuid;

/// Precomputed dummy hash for timing-attack mitigation. Password
/// verification takes the same time regardless of whether the user exists.
pub(crate) static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(b"dummy_password_for_timing_attack_mitigation", &salt)
        .expect("Failed to generate dummy hash")
        .to_string()
});

pub(crate) fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();
    Ok(password_hash)
}

/// Public re-export of the password hashing routine used by the test
/// admin seed in `main.rs`. Lives on the auth module so the seed path
/// uses the exact same hash format the runtime auth flow expects.
///
/// Compiled only under the `e2e_testing` feature so the production binary
/// cannot reach a path that produces valid auth hashes from arbitrary
/// plaintext.
#[cfg(feature = "e2e_testing")]
pub fn hash_password_for_seed(password: &str) -> Result<String> {
    hash_password(password)
}

/// Argon2 hash of a fresh random throwaway secret. Used to seed
/// `password_hash` on inert invite-pending rows so `verify_password`
/// returns `Ok(false)` (just like the dummy-hash branch for non-existent
/// users) instead of an `Err` that would surface to the client and
/// discriminate inert rows from non-existent ones.
pub fn placeholder_password_hash() -> Result<String> {
    let plaintext = format!("placeholder_{}", Uuid::new_v4());
    hash_password(&plaintext)
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(password_hash).map_err(|e| anyhow::anyhow!("Invalid hash: {}", e))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub(crate) fn generate_verification_token() -> String {
    let token_bytes: [u8; 32] = rand::rng().random();
    hex::encode(token_bytes)
}

/// Compute a BLAKE2b digest of security-relevant fields. Changes to
/// password, email, or TOTP secret will invalidate existing sessions.
/// Roughly equal to SHA256 and faster; argon2 already pulls blake2 in.
pub(crate) fn compute_session_hash(
    password_hash: &str,
    email: &str,
    totp_secret: Option<&str>,
) -> Vec<u8> {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(password_hash.as_bytes());
    hasher.update(email.as_bytes());
    hasher.update(totp_secret.unwrap_or("").as_bytes());
    hasher.finalize().to_vec()
}
