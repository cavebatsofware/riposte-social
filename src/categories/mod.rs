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
//! Categories: a flat admin-managed taxonomy that groups posts and albums.
//!
//! - Public read of `/api/categories` returns the full ordered list (used
//!   by the social-frontend's left rail and the admin UI).
//! - Admin-only create/update/delete via `/api/admin/categories[/{id}]`.
//! - Posts and albums each have a nullable `category_id` FK; the feed and
//!   album list endpoints accept `?category=<slug>` to filter.
//!
//! Slug rules: lowercase ASCII / digits / `-`, 2..=50 chars. Names are
//! free-form (whitespace-trimmed, length 1..=80). Slugs are derived from
//! the name when not supplied at create-time, but admins can override.

pub mod routes;

/// Slug shape rules — lowercase ASCII letters/digits/`-`, length 2..=50.
/// `validate_slug_shape` returns an error message on failure.
pub fn validate_slug_shape(slug: &str) -> Result<(), String> {
    let len = slug.chars().count();
    if !(2..=50).contains(&len) {
        return Err("Slug must be 2-50 characters".to_string());
    }
    for c in slug.chars() {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if !ok {
            return Err(
                "Slug may only contain lowercase ASCII letters, digits, and hyphens".to_string(),
            );
        }
    }
    Ok(())
}

/// Best-effort slug derivation from a free-form name. Lowercase, replace
/// every non-allowed char with `-`, collapse repeated dashes, trim leading
/// or trailing dashes, truncate to 50 chars. Padding-up isn't necessary
/// because the validator's min-length is 2 and the caller should guard
/// against empty inputs.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_lowercase() || lc.is_ascii_digit() || lc == '-' {
            out.push(lc);
        } else {
            out.push('-');
        }
    }
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
    collapsed.trim_matches('-').chars().take(50).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowers_and_dashes() {
        assert_eq!(slugify("Family Photos!"), "family-photos");
        assert_eq!(slugify("Vacation 2024"), "vacation-2024");
    }

    #[test]
    fn slugify_collapses_runs() {
        assert_eq!(slugify("a   b"), "a-b");
        assert_eq!(slugify("--leading-and-trailing--"), "leading-and-trailing");
    }

    #[test]
    fn validate_slug_shape_accepts_valid() {
        validate_slug_shape("ab").unwrap();
        validate_slug_shape("family-photos-2024").unwrap();
    }

    #[test]
    fn validate_slug_shape_rejects() {
        assert!(validate_slug_shape("a").is_err());
        assert!(validate_slug_shape("Foo").is_err());
        assert!(validate_slug_shape("foo bar").is_err());
        assert!(validate_slug_shape(&"a".repeat(51)).is_err());
    }
}
