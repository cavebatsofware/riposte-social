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
//! Request and response DTOs for the admin auth + self-service endpoints.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RegisterResponse {
    pub message: String,
    pub email: String,
}

#[derive(Deserialize, TS)]
#[ts(export, rename = "AdminVerifyQuery")]
pub struct VerifyQuery {
    pub token: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct VerifyResponse {
    pub message: String,
    pub email: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct FeatureFlags {
    pub access_codes_enabled: bool,
    pub contact_form_enabled: bool,
    pub subscriptions_enabled: bool,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CsrfTokenResponse {
    pub token: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AuthConfigResponse {
    pub oidc_enabled: bool,
    pub login_url: Option<String>,
    pub account_url: Option<String>,
    /// The deployment's domain, gating the admin email on the register page.
    /// Sourced from the runtime SITE_DOMAIN env so one image serves any site;
    /// empty when unset.
    pub site_domain: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MfaSetupResponse {
    pub secret: String,
    pub qr_code: String,
    pub otpauth_url: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct MfaConfirmRequest {
    pub secret: String,
    pub code: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MfaConfirmResponse {
    pub message: String,
    pub totp_enabled: bool,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct MfaVerifyRequest {
    pub code: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MfaVerifyResponse {
    pub message: String,
    pub id: Uuid,
    pub email: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct MfaDisableRequest {
    pub password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MfaDisableResponse {
    pub message: String,
    pub totp_enabled: bool,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ChangePasswordResponse {
    pub message: String,
}

#[derive(Deserialize, TS)]
#[allow(dead_code)]
#[ts(export)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ForgotPasswordResponse {
    pub requires_mfa: bool,
    pub message: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ForgotPasswordVerifyMfaRequest {
    pub email: String,
    pub code: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ForgotPasswordVerifyMfaResponse {
    pub message: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ResetPasswordResponse {
    pub message: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub totp_enabled: bool,
    pub mfa_required: bool,
    pub active: bool,
    pub force_password_change: bool,
    pub role: String,
    /// Public handle for the caller. Surfaced here so the social-frontend's
    /// header dropdown can deep-link to `/u/{handle}` without an extra
    /// fetch.
    pub handle: Option<String>,
    pub avatar_url: Option<String>,
    pub avatar_icon_data: Option<String>,
    /// Saved UI locale. NULL when the user has never explicitly chosen one;
    /// the frontend's i18next browser-language detector fills the gap.
    pub locale: Option<String>,
    pub features: FeatureFlags,
}
