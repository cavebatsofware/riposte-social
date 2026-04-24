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
use crate::admin::{AdminAuthBackend, AdminUserAuth, MFA_VERIFIED_KEY};
use crate::middleware::AdminUserInfo;
use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use axum_login::AuthSession;
use tower_sessions::Session;

pub type AdminAuthSession = AuthSession<AdminAuthBackend>;

pub async fn require_admin_auth(
    auth_session: AdminAuthSession,
    session: Session,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    tracing::debug!(
        "require_admin_auth middleware called for: {}",
        request.uri()
    );
    tracing::debug!("Auth session user present: {}", auth_session.user().await.is_some());

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
                return (StatusCode::FORBIDDEN, Json(json!({"error": "MFA verification required"}))).into_response();
            }
        }

        // Check if user must change password
        // Allow access to change-password and logout endpoints
        if user.force_password_change {
            let path = request.uri().path();
            if path != "/api/admin/change-password"
                && path != "/api/admin/logout"
                && path != "/api/admin/me"
                && path != "/api/admin/csrf-token"
            {
                tracing::warn!("Password change required for user: {}", user.email);
                return (StatusCode::FORBIDDEN, Json(json!({"error": "Password change required"}))).into_response();
            }
        }

        // Store minimal info for access logging (id is Copy, email cloned once)
        let user_info = AdminUserInfo {
            id: user.id,
            email: user.email.clone(),
        };
        request.extensions_mut().insert(user);
        let mut response = next.run(request).await;
        // Insert into response extensions for access_log_middleware
        response.extensions_mut().insert(user_info);
        tracing::debug!("Handler completed with status: {}", response.status());
        response
    } else {
        tracing::warn!("Authentication required but user not present");
        (StatusCode::UNAUTHORIZED, Json(json!({"error": "Not authenticated"}))).into_response()
    }
}

pub const ROLE_ADMINISTRATOR: &str = "administrator";

/// Middleware that checks if the authenticated user has the administrator role.
/// Must be applied AFTER require_admin_auth (which inserts the user into extensions).
pub async fn require_administrator(request: Request<Body>, next: Next) -> Response {
    let user = request.extensions().get::<AdminUserAuth>();

    match user {
        Some(user) if user.role == ROLE_ADMINISTRATOR => next.run(request).await,
        Some(user) => {
            tracing::warn!(
                "Access denied for user {} with role '{}': administrator required",
                user.email,
                user.role
            );
            (StatusCode::FORBIDDEN, Json(json!({"error": "Insufficient permissions"}))).into_response()
        }
        None => {
            tracing::error!("require_administrator called without require_admin_auth");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Internal server error"}))).into_response()
        }
    }
}

/// Extension type for accessing authenticated admin user in handlers
/// Usage in handlers:
/// ```ignore
/// async fn handler(Extension(user): Extension<AdminUserAuth>) -> impl IntoResponse {
///     // user is guaranteed to be authenticated here
/// }
/// ```
pub type AuthenticatedUser = axum::extract::Extension<AdminUserAuth>;
