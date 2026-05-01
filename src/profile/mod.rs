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
//! Phase 8: profile management.
//!
//! Self-service endpoints for the authenticated viewer (`/api/me/profile`,
//! `/api/me/avatar`) and a public read endpoint (`/api/profiles/{handle}`).
//! Avatar bytes are served by the avatar route in this module.
//!
//! Handle constraints (length 3-30, lowercase ASCII / digits / `_` / `-`),
//! bio (500 char) and pronouns (30 char) caps live here as the single
//! source of truth, used both at write time and (via the migration backfill)
//! at first deployment.

pub mod routes;

use crate::entities::{user, User};
use anyhow::Result;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

/// Maximum length for a `handle`. GitHub uses 39, LinkedIn allows 100; 30
/// is plenty here and keeps URLs and admin tables tidy.
pub const HANDLE_MAX_LEN: usize = 30;

/// Minimum length for a `handle`. Below this, handles read as adversarial
/// noise (`a`, `12`) rather than identifiers.
pub const HANDLE_MIN_LEN: usize = 3;

/// Bio cap. Long enough to be useful, short enough that the social-frontend
/// profile card doesn't need to truncate.
pub const BIO_MAX_LEN: usize = 500;

/// Pronouns cap. Short field; longer entries probably belong in the bio.
pub const PRONOUNS_MAX_LEN: usize = 30;

/// Validate a handle string against the app-level rules:
/// - length in `[HANDLE_MIN_LEN, HANDLE_MAX_LEN]`
/// - chars in `[a-z0-9_-]` (lowercase ASCII letters, digits, underscore,
///   hyphen)
///
/// Returns a human-readable error message on failure.
pub fn validate_handle_shape(handle: &str) -> Result<(), String> {
    let len = handle.chars().count();
    if len < HANDLE_MIN_LEN {
        return Err(format!(
            "Handle must be at least {} characters",
            HANDLE_MIN_LEN
        ));
    }
    if len > HANDLE_MAX_LEN {
        return Err(format!(
            "Handle must be at most {} characters",
            HANDLE_MAX_LEN
        ));
    }
    for c in handle.chars() {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-';
        if !ok {
            return Err(
                "Handle may only contain lowercase ASCII letters, digits, underscore, and hyphen"
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Mint a unique handle for a new user. Strips disallowed chars from
/// `seed` (typically the email local-part), pads short seeds, and appends
/// a numeric suffix until uniqueness is found.
///
/// Used at every user-creation site so the NOT NULL handle column is always
/// populated. Suffix collision uses an incrementing `-1`, `-2`, ...; the
/// loop bounds the search so a pathological collision pattern still
/// terminates.
pub async fn mint_unique_handle<C: ConnectionTrait>(db: &C, seed: &str) -> Result<String> {
    let base = sanitize_seed(seed);
    if !is_handle_taken(db, &base).await? {
        return Ok(base);
    }
    // Cap suffix exploration so we don't grind forever; 10000 candidates
    // beyond the base is a very generous ceiling for collision resolution.
    for n in 1..10_000u32 {
        let suffix = format!("-{}", n);
        let trimmed = base
            .chars()
            .take(HANDLE_MAX_LEN.saturating_sub(suffix.chars().count()))
            .collect::<String>();
        let candidate = format!("{}{}", trimmed, suffix);
        if !is_handle_taken(db, &candidate).await? {
            return Ok(candidate);
        }
    }
    anyhow::bail!("Could not mint a unique handle for seed '{}'", seed)
}

/// Map an arbitrary seed string into something handle-shaped: lowercase,
/// keep `[a-z0-9_-]`, replace anything else with `-`, collapse repeated
/// dashes, trim leading/trailing dashes, and pad/truncate to the length
/// window.
fn sanitize_seed(seed: &str) -> String {
    let mut out = String::with_capacity(seed.len());
    for c in seed.chars() {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_lowercase() || lc.is_ascii_digit() || lc == '_' || lc == '-' {
            out.push(lc);
        } else {
            out.push('-');
        }
    }
    // Collapse runs of dashes.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_dash = false;
    for c in out.chars() {
        if c == '-' {
            if !prev_dash {
                collapsed.push(c);
            }
            prev_dash = true;
        } else {
            collapsed.push(c);
            prev_dash = false;
        }
    }
    let trimmed = collapsed.trim_matches('-').to_string();
    let mut padded = trimmed;
    while padded.chars().count() < HANDLE_MIN_LEN {
        padded.push('x');
    }
    padded.chars().take(HANDLE_MAX_LEN).collect()
}

async fn is_handle_taken<C: ConnectionTrait>(db: &C, candidate: &str) -> Result<bool> {
    let found = User::find()
        .filter(user::Column::Handle.eq(candidate))
        .one(db)
        .await?;
    Ok(found.is_some())
}

/// Render the URL the social-frontend should use for a user's avatar.
/// Prefers the locally-uploaded S3-served route over any externally
/// supplied `avatar_url` (the latter exists for a future OIDC claim path
/// and is currently always None).
pub fn avatar_url_for(model: &user::Model) -> Option<String> {
    if model.avatar_s3_key.is_some() {
        Some(format!("/avatars/{}", model.id))
    } else {
        model.avatar_url.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_handle_shape_accepts_valid() {
        validate_handle_shape("grant").unwrap();
        validate_handle_shape("abc").unwrap();
        validate_handle_shape("a-b_c1").unwrap();
        validate_handle_shape(&"a".repeat(30)).unwrap();
    }

    #[test]
    fn validate_handle_shape_rejects_too_short() {
        assert!(validate_handle_shape("ab").is_err());
    }

    #[test]
    fn validate_handle_shape_rejects_too_long() {
        assert!(validate_handle_shape(&"a".repeat(31)).is_err());
    }

    #[test]
    fn validate_handle_shape_rejects_uppercase() {
        assert!(validate_handle_shape("Grant").is_err());
    }

    #[test]
    fn validate_handle_shape_rejects_punct() {
        assert!(validate_handle_shape("a.b").is_err());
        assert!(validate_handle_shape("a b").is_err());
        assert!(validate_handle_shape("a@b").is_err());
    }

    #[test]
    fn sanitize_seed_strips_email_local_part() {
        assert_eq!(sanitize_seed("Grant.DeFayette+tag"), "grant-defayette-tag");
    }

    #[test]
    fn sanitize_seed_collapses_dashes() {
        assert_eq!(sanitize_seed("a..b"), "a-b");
    }

    #[test]
    fn sanitize_seed_pads_short_input() {
        // "a" + xx = "axx" (length 3 minimum)
        assert_eq!(sanitize_seed("a"), "axx");
    }

    #[test]
    fn sanitize_seed_truncates_long_input() {
        let long = "a".repeat(50);
        assert_eq!(sanitize_seed(&long).len(), 30);
    }
}
