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
//! Local + OIDC authentication. Splits into:
//! - `backend.rs`     `AuthnBackend` impl and the surface UserAuthBackend exposes
//! - `credentials.rs` argon2 password hashing and verification
//! - `totp.rs`        TOTP secret generation, encryption, verification
//! - `oidc.rs`        OIDC claims extraction, JWT validation, sub mapping

pub mod backend;
pub mod credentials;
pub mod oidc;
pub mod totp;

pub use backend::{AuthError, Credentials, OidcAuthClaims, UserAuth, UserAuthBackend};
#[cfg(feature = "e2e_testing")]
pub use credentials::hash_password_for_seed;
pub use credentials::{placeholder_password_hash, verify_password};
