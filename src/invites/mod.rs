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
//! Invite-code creation, validation, and acceptance.
//!
//! Commenter onboarding goes through this module. Admins create codes; visitors
//! land at `/invite/{code}` which sets a `pending_invite` cookie and serves the
//! social SPA. The SPA polls `/api/invites/current` to render a welcome splash;
//! clicking through into OIDC binds the code to the new commenter's user row.

pub mod handlers;
pub mod queries;
pub mod types;

use crate::settings::SettingsService;
use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::DatabaseConnection;

pub use handlers::{admin_invite_routes, auth_invite_routes, public_invite_routes};
pub use queries::{issue_invite_for_user, mark_used, validate_invite_code};
pub use types::InviteResponse;

/// Cookie carrying a pending invite from the splash page through OIDC sign-in.
/// The cookie's Max-Age matches `invite_code.expires_at`, so the splash returns
/// up to the admin-set expiry. SameSite=Lax keeps it on top-level navigations
/// (including the OIDC redirect chain), while Secure is set in production via
/// the same env-driven path used by tower-sessions.
pub const PENDING_INVITE_COOKIE: &str = "pending_invite";

/// Maximum lifetime an admin can set on an invite. Codes that linger longer
/// than this are a phishing footgun; the admin can always recreate.
pub const MAX_INVITE_LIFETIME_DAYS: i64 = 30;

/// Default lifetime when an admin doesn't specify `expires_in_hours`.
pub const DEFAULT_INVITE_LIFETIME_HOURS: i64 = 7 * 24;

#[derive(Clone)]
pub struct InviteState {
    pub db: DatabaseConnection,
    pub auth_backend: crate::admin::UserAuthBackend,
    /// Mirror of OidcConfig::enabled so the password-mode acceptance endpoint
    /// can short-circuit when SSO is the active auth mode.
    pub oidc_enabled: bool,
    pub settings: SettingsService,
}

/// 32-char alphanumeric invite code with ~190 bits of entropy, well past the
/// guessing threshold even with parallel attackers, and short enough to fit
/// neatly in a URL or chat message.
pub(crate) fn generate_invite_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Blake2b digest of the plaintext code, hex-encoded. The DB stores this
/// digest in the `code` column instead of the plaintext, so a database read
/// alone yields no usable invite codes.
pub(crate) fn hash_invite_code(plaintext: &str) -> String {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// Cookie `Secure` flag. Production builds always return true so a
/// release binary cannot be coerced into emitting non-Secure auth
/// cookies via a `DEV_MODE=true` env var. Under the `e2e_testing`
/// feature the override is restored so the test container (which
/// serves over plain HTTP locally) can set non-Secure cookies for
/// Cypress and hand-testing.
pub(crate) fn is_cookie_secure() -> bool {
    #[cfg(feature = "e2e_testing")]
    {
        if std::env::var("DEV_MODE").as_deref() == Ok("true") {
            return false;
        }
    }
    true
}

/// Build the `Set-Cookie` for a fresh pending invite. Max-Age matches the
/// invite's expiry exactly so the splash stops auto-surfacing once the code
/// can't be accepted anyway.
pub fn build_pending_invite_cookie(
    code: &str,
    expires_at: chrono::DateTime<Utc>,
    secure: bool,
) -> axum_extra::extract::cookie::Cookie<'static> {
    let max_age = (expires_at - Utc::now()).max(Duration::seconds(0));
    let mut c = axum_extra::extract::cookie::Cookie::new(PENDING_INVITE_COOKIE, code.to_owned());
    c.set_path("/");
    c.set_http_only(true);
    c.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
    c.set_secure(secure);
    c.set_max_age(time::Duration::seconds(max_age.num_seconds()));
    c
}

pub fn remove_pending_invite_cookie(
    cookies: axum_extra::extract::CookieJar,
) -> axum_extra::extract::CookieJar {
    let mut removal = axum_extra::extract::cookie::Cookie::from(PENDING_INVITE_COOKIE);
    removal.set_path("/");
    cookies.remove(removal)
}
