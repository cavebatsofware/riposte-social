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
pub mod access_codes;
pub mod access_logs;
pub mod admin_users;
pub mod auth;
pub mod oidc_routes;
pub mod pagination;
pub mod password;
pub mod routes;
pub mod settings;
pub mod totp;

pub use auth::{AdminAuthBackend, AdminUserAuth, Credentials};

/// Session key for MFA verification status. Shared across routes and middleware.
pub const MFA_VERIFIED_KEY: &str = "mfa_verified";
