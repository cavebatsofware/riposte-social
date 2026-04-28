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
//! Authentication and role-based authorization middleware for the unified
//! `UserAuthBackend`.
//!
//! Layered usage:
//! - `require_authenticated`. apply to any route that needs a logged-in user
//!   (commenter, poster, or administrator). Inserts `UserAuth` into request
//!   extensions and `UserInfo` into response extensions for access logging.
//! - `require_admin`, `require_admin_or_poster`, or the parameterized
//!   `require_role`. apply *after* `require_authenticated` to gate by role.
use crate::admin::{UserAuth, UserAuthBackend, MFA_VERIFIED_KEY};
use crate::entities::user::{ROLE_ADMINISTRATOR, ROLE_POSTER};
use crate::middleware::UserInfo;
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use axum_login::AuthSession;
use serde_json::json;
use tower_sessions::Session;

/// Pre-built role lists for use with `require_role` via `from_fn_with_state`.
/// These are `'static` slices of `'static` strings so they can be Clone'd for
/// free into the middleware state.
pub const ROLES_ADMIN_ONLY: &[&str] = &[ROLE_ADMINISTRATOR];
pub const ROLES_ADMIN_OR_POSTER: &[&str] = &[ROLE_ADMINISTRATOR, ROLE_POSTER];

pub type UserAuthSession = AuthSession<UserAuthBackend>;

/// Require any authenticated user (administrator, poster, or commenter).
/// Enforces MFA-verified session for users with TOTP enabled, and the
/// force-password-change lockout for password-mode admin users.
pub async fn require_authenticated(
    auth_session: UserAuthSession,
    session: Session,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    tracing::debug!(
        "require_authenticated middleware called for: {}",
        request.uri()
    );
    tracing::debug!(
        "Auth session user present: {}",
        auth_session.user().await.is_some()
    );

    if let Some(user) = auth_session.user().await {
        tracing::debug!("User authenticated: {}", user.email);

        // Check if MFA is required but not verified in session
        if user.totp_enabled {
            let mfa_verified = session
                .get::<bool>(MFA_VERIFIED_KEY)
                .await
                .unwrap_or(None)
                .unwrap_or(false);

            if !mfa_verified {
                tracing::warn!("MFA required but not verified for user: {}", user.email);
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "MFA verification required"})),
                )
                    .into_response();
            }
        }

        // Check if user must change password (admin/poster password-mode only).
        // Allow access to change-password and a few read-only endpoints during
        // the lockout. Commenters and OIDC-linked users never have this flag set.
        if user.force_password_change {
            let path = request.uri().path();
            if path != "/api/me/password"
                && path != "/api/auth/logout"
                && path != "/api/me"
                && path != "/api/auth/csrf-token"
            {
                tracing::warn!("Password change required for user: {}", user.email);
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "Password change required"})),
                )
                    .into_response();
            }
        }

        // Store minimal info for access logging (id is Copy, email cloned once)
        let user_info = UserInfo {
            id: user.id,
            email: user.email.clone(),
        };
        request.extensions_mut().insert(user);
        let mut response = next.run(request).await;
        response.extensions_mut().insert(user_info);
        tracing::debug!("Handler completed with status: {}", response.status());
        response
    } else {
        tracing::warn!("Authentication required but user not present");
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Not authenticated"})),
        )
            .into_response()
    }
}

/// Require the administrator role. Apply *after* `require_authenticated`.
pub async fn require_admin(request: Request<Body>, next: Next) -> Response {
    match request.extensions().get::<UserAuth>() {
        Some(user) if user.role == ROLE_ADMINISTRATOR => next.run(request).await,
        Some(user) => deny_role(user, "administrator"),
        None => internal_misuse("require_admin"),
    }
}

/// Require administrator OR poster role. Apply *after* `require_authenticated`.
/// Used to gate post creation/edit endpoints that both tiers can perform.
pub async fn require_admin_or_poster(request: Request<Body>, next: Next) -> Response {
    match request.extensions().get::<UserAuth>() {
        Some(user) if user.role == ROLE_ADMINISTRATOR || user.role == ROLE_POSTER => {
            next.run(request).await
        }
        Some(user) => deny_role(user, "administrator or poster"),
        None => internal_misuse("require_admin_or_poster"),
    }
}

/// Generic role gate parameterized by an allowed role list. Apply *after*
/// `require_authenticated` via `from_fn_with_state`. Use this when the named
/// gates above don't fit (e.g. arbitrary role combinations or future tiers).
///
/// ```ignore
/// use crate::middleware::{require_authenticated, require_role, ROLES_ADMIN_OR_POSTER};
/// .layer(from_fn_with_state(ROLES_ADMIN_OR_POSTER, require_role))
/// .layer(from_fn(require_authenticated))
/// ```
///
/// Inline lists work too:
/// `from_fn_with_state(&[user::ROLE_ADMINISTRATOR][..], require_role)`.
pub async fn require_role(
    State(allowed): State<&'static [&'static str]>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match request.extensions().get::<UserAuth>() {
        Some(user) if allowed.iter().any(|r| *r == user.role) => next.run(request).await,
        Some(user) => {
            let label = format!("one of {:?}", allowed);
            deny_role(user, &label)
        }
        None => internal_misuse("require_role"),
    }
}

fn deny_role(user: &UserAuth, expected: &str) -> Response {
    tracing::warn!(
        "Access denied for user {} (role={}): {} required",
        user.email,
        user.role,
        expected
    );
    (
        StatusCode::FORBIDDEN,
        Json(json!({"error": "Insufficient permissions"})),
    )
        .into_response()
}

fn internal_misuse(gate: &str) -> Response {
    tracing::error!("{} called without require_authenticated", gate);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "Internal server error"})),
    )
        .into_response()
}

/// Extension type for accessing the authenticated user in handlers.
/// Usage:
/// ```ignore
/// async fn handler(Extension(user): Extension<UserAuth>) -> impl IntoResponse {
///     // user is guaranteed to be authenticated here
/// }
/// ```
pub type AuthenticatedUser = axum::extract::Extension<UserAuth>;
