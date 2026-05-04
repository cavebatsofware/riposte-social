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
use uuid::Uuid;

pub mod access_log;
pub mod admin_auth;

pub use access_log::access_log_middleware;
pub use admin_auth::{
    require_admin, require_admin_or_poster, require_authenticated, require_role, AuthenticatedUser,
    ROLES_ADMIN_ONLY, ROLES_ADMIN_OR_POSTER,
};

/// Minimal admin user info for access logging. Only stores what's needed for audit trail.
/// Uuid is Copy (16 bytes), email is the only heap allocation.
#[derive(Clone)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
}

// Re-export CSRF from the axum-tower-sessions-csrf crate
pub use axum_tower_sessions_csrf::CsrfMiddleware;

/// CSRF middleware function (alias for CsrfMiddleware::middleware)
pub fn csrf_middleware(
    session: tower_sessions::Session,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl std::future::Future<Output = axum::response::Response> {
    CsrfMiddleware::middleware(session, request, next)
}
