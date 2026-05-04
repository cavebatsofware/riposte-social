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
use super::auth::verify_password;
use super::password::PasswordValidator;
use super::totp;
use super::{Credentials, UserAuthBackend};
use crate::email::EmailService;
use crate::errors::{AppError, AppResult};
use crate::security_callbacks::AppRateLimitCallbacks;
use crate::settings::SettingsService;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware::from_fn_with_state,
    response::Json,
    routing::{get, post},
    Router,
};
use axum_login::AuthSession;
use axum_tower_sessions_csrf::get_or_create_token;
use basic_axum_rate_limit::{rate_limit_middleware, RateLimiter};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_sessions::Session;

use super::MFA_VERIFIED_KEY;

pub type UserAuthSession = AuthSession<UserAuthBackend>;

#[derive(Clone)]
pub struct AdminState {
    pub auth_backend: UserAuthBackend,
    pub email_service: Arc<EmailService>,
    pub settings: SettingsService,
    pub oidc_enabled: bool,
    pub oidc_account_url: Option<String>,
}

pub fn admin_api_routes(
    auth_rate_limiter: RateLimiter<AppRateLimitCallbacks>,
) -> Router<AdminState> {
    // Pre-session / session-establishing endpoints live under /api/auth/*.
    // Self-service operations on the caller's own account live under /api/me/*.
    // Admin-on-others operations stay under /api/admin/* (handled in admin_users.rs).
    let rate_limited_routes = Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/mfa/verify", post(mfa_verify))
        .route("/api/auth/forgot-password", post(forgot_password))
        .route(
            "/api/auth/forgot-password/verify-mfa",
            post(forgot_password_verify_mfa),
        )
        .route("/api/auth/reset-password", post(reset_password))
        .route("/api/me/password", post(change_password))
        .route("/api/me/mfa/disable", post(mfa_disable))
        .layer(from_fn_with_state(auth_rate_limiter, rate_limit_middleware));

    let standard_routes = Router::new()
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/verify-email", get(verify_email))
        .route("/api/auth/csrf-token", get(get_csrf_token))
        .route("/api/auth/config", get(auth_config))
        .route("/api/site/config", get(site_config))
        .route("/api/me", get(me))
        .route("/api/me/mfa/setup", post(mfa_setup))
        .route("/api/me/mfa/confirm-setup", post(mfa_confirm_setup));

    rate_limited_routes.merge(standard_routes)
}

#[derive(Deserialize)]
struct RegisterRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct RegisterResponse {
    message: String,
    email: String,
}

async fn register(
    State(state): State<AdminState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<RegisterResponse>> {
    require_local_auth(state.oidc_enabled)?;

    // Check if registration is enabled
    let registration_enabled = state
        .settings
        .get_admin_registration_enabled()
        .await
        .unwrap_or(false);

    if !registration_enabled {
        return Err(AppError::AuthError(
            "Registration is currently disabled".to_string(),
        ));
    }

    // Validate password strength
    if let Err(errors) = PasswordValidator::validate(&req.password, &req.email) {
        return Err(AppError::ValidationError(errors.join("; ")));
    }

    // Create admin user
    let (admin, verification_token) = state
        .auth_backend
        .create_admin(&req.email, &req.password)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    // Send verification email
    state
        .email_service
        .send_verification_email(&admin.email, &verification_token)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to send verification email: {}", e)))?;

    Ok(Json(RegisterResponse {
        message: "Registration successful. Please check your email to verify your account."
            .to_string(),
        email: admin.email,
    }))
}

async fn login(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<UserResponse>> {
    require_local_auth(state.oidc_enabled)?;

    let creds = Credentials::Password {
        email: req.email,
        password: req.password,
    };

    // Don't surface the underlying error string to the client. Every
    // authenticate failure (deactivated row, inert/invite-pending row,
    // unverified email, malformed hash, DB error, etc.) becomes the same
    // generic 401 to deny enumeration via response-body discrimination.
    // The actual error is logged server-side for ops debugging.
    let user = auth_session
        .authenticate(creds)
        .await
        .map_err(|e| {
            tracing::warn!("Login authenticate failed: {}", e);
            AppError::AuthError("Invalid email or password".to_string())
        })?
        .ok_or_else(|| AppError::AuthError("Invalid email or password".to_string()))?;

    auth_session
        .login(&user)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    // If MFA is enabled but not yet verified, indicate that MFA is required
    let mfa_required = user.totp_enabled && !user.mfa_verified;

    let features = FeatureFlags {
        access_codes_enabled: state
            .settings
            .get_access_codes_enabled()
            .await
            .unwrap_or(true),
        contact_form_enabled: state
            .settings
            .get_contact_form_enabled()
            .await
            .unwrap_or(true),
        subscriptions_enabled: state
            .settings
            .get_subscriptions_enabled()
            .await
            .unwrap_or(true),
    };

    let (handle, avatar_url, locale) = lookup_handle_avatar_locale(&state, user.id).await;
    Ok(Json(UserResponse {
        id: user.id,
        email: user.email,
        email_verified: user.email_verified,
        totp_enabled: user.totp_enabled,
        mfa_required,
        active: user.active,
        force_password_change: user.force_password_change,
        role: user.role,
        handle,
        avatar_url,
        locale,
        features,
    }))
}

async fn logout(auth_session: UserAuthSession) -> AppResult<StatusCode> {
    auth_session
        .logout()
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct VerifyQuery {
    token: String,
}

#[derive(Serialize)]
struct VerifyResponse {
    message: String,
    email: String,
}

async fn verify_email(
    State(state): State<AdminState>,
    Query(query): Query<VerifyQuery>,
) -> AppResult<Json<VerifyResponse>> {
    let admin = state
        .auth_backend
        .verify_email(&query.token)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    Ok(Json(VerifyResponse {
        message: "Email verified successfully. You can now log in.".to_string(),
        email: admin.email,
    }))
}

#[derive(Serialize)]
struct FeatureFlags {
    access_codes_enabled: bool,
    contact_form_enabled: bool,
    subscriptions_enabled: bool,
}

#[derive(Serialize)]
struct UserResponse {
    id: uuid::Uuid,
    email: String,
    email_verified: bool,
    totp_enabled: bool,
    mfa_required: bool,
    active: bool,
    force_password_change: bool,
    role: String,
    /// Public handle for the caller. Surfaced here so the social-frontend's
    /// header dropdown can deep-link to `/u/{handle}` without an extra
    /// fetch.
    handle: Option<String>,
    /// Caller's avatar URL (`/avatars/{user_id}` when set, else None).
    /// Same rationale as `handle`.
    avatar_url: Option<String>,
    /// Saved UI locale (Phase 11e). NULL when the user has never explicitly
    /// chosen one — the frontend's i18next browser-language detector
    /// fills the gap. Surfaced here so AuthContext can sync `i18n.changeLanguage`
    /// once on first login.
    locale: Option<String>,
    features: FeatureFlags,
}

async fn me(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
    session: Session,
) -> AppResult<Json<UserResponse>> {
    let user = auth_session
        .user()
        .await
        .ok_or_else(|| AppError::AuthError("Not authenticated".to_string()))?;

    // Check session for MFA verified status
    let mfa_verified = session
        .get::<bool>(MFA_VERIFIED_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    // MFA is required if TOTP is enabled and not yet verified in this session
    let mfa_required = user.totp_enabled && !mfa_verified;

    let features = FeatureFlags {
        access_codes_enabled: state
            .settings
            .get_access_codes_enabled()
            .await
            .unwrap_or(true),
        contact_form_enabled: state
            .settings
            .get_contact_form_enabled()
            .await
            .unwrap_or(true),
        subscriptions_enabled: state
            .settings
            .get_subscriptions_enabled()
            .await
            .unwrap_or(true),
    };

    let (handle, avatar_url, locale) = lookup_handle_avatar_locale(&state, user.id).await;
    Ok(Json(UserResponse {
        id: user.id,
        email: user.email,
        email_verified: user.email_verified,
        totp_enabled: user.totp_enabled,
        mfa_required,
        active: user.active,
        force_password_change: user.force_password_change,
        role: user.role,
        handle,
        avatar_url,
        locale,
        features,
    }))
}

/// Look up the caller's `handle` + derived `avatar_url` for inclusion in
/// the `/api/me` and `/api/auth/login` payloads. Failures here do not
/// block the response: the caller is already authenticated, and the
/// missing fields just mean the social-frontend's avatar dropdown falls
/// back to its initials path.
async fn lookup_handle_avatar_locale(
    state: &AdminState,
    user_id: uuid::Uuid,
) -> (Option<String>, Option<String>, Option<String>) {
    match state.auth_backend.get_admin_by_id(user_id).await {
        Ok(Some(model)) => (
            Some(model.handle.clone()),
            crate::profile::avatar_url_for(&model),
            model.locale.clone(),
        ),
        _ => (None, None, None),
    }
}

#[derive(Serialize)]
struct CsrfTokenResponse {
    token: String,
}

/// Get CSRF token for the current session
async fn get_csrf_token(session: Session) -> AppResult<Json<CsrfTokenResponse>> {
    let token = get_or_create_token(&session)
        .await
        .map_err(AppError::AuthError)?;

    Ok(Json(CsrfTokenResponse { token }))
}

// ==================== Auth Config Endpoint ====================

#[derive(Serialize)]
struct AuthConfigResponse {
    oidc_enabled: bool,
    login_url: Option<String>,
    account_url: Option<String>,
}

/// Returns auth configuration so the frontend knows whether to use OIDC or local login
async fn auth_config(State(state): State<AdminState>) -> Json<AuthConfigResponse> {
    let login_url = if state.oidc_enabled {
        Some("/api/auth/oidc/login".to_string())
    } else {
        None
    };

    Json(AuthConfigResponse {
        oidc_enabled: state.oidc_enabled,
        login_url,
        account_url: state.oidc_account_url.clone(),
    })
}

/// Per-tier site configuration. Returns only the keys the caller can act
/// on so the frontend never sees gates that are irrelevant to them:
/// - everyone gets `site_name` + `public_feed_enabled` (the latter switches
///   the empty-state copy and gates anonymous reads).
/// - posters additionally see `poster_posting_enabled` (gates the Compose
///   button).
/// - admins additionally see `commenter_invites_enabled` and
///   `fb_import_enabled` (gates the matching admin pages).
///
/// No auth required; the response just looks at the optional auth session.
/// Each gate read is best-effort — a transient settings DB hiccup falls
/// back to the safe default (true) so a flaky query never accidentally
/// disables features in the UI.
async fn site_config(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
) -> AppResult<Json<serde_json::Value>> {
    let user = auth_session.user().await;
    let role = user.as_ref().map(|u| u.role.as_str());
    let is_admin = role == Some(crate::entities::user::ROLE_ADMINISTRATOR);
    let is_poster_or_admin = matches!(
        role,
        Some(crate::entities::user::ROLE_ADMINISTRATOR) | Some(crate::entities::user::ROLE_POSTER)
    );

    // Settings reads bubble up as 500s rather than silently defaulting.
    // The frontend's `SiteConfigContext` keeps gated affordances hidden
    // until this fetch returns, so any read failure here means the SPA
    // never reveals a feature that might actually be off — fail closed
    // end-to-end.
    fn read_err(e: anyhow::Error) -> AppError {
        AppError::InternalError(format!("settings read failed: {:#}", e))
    }

    let mut payload = serde_json::Map::new();
    payload.insert(
        "site_name".to_string(),
        serde_json::Value::String(state.settings.get_site_name().await.map_err(read_err)?),
    );
    payload.insert(
        "public_feed_enabled".to_string(),
        serde_json::Value::Bool(
            state
                .settings
                .get_public_feed_enabled()
                .await
                .map_err(read_err)?,
        ),
    );
    if is_poster_or_admin {
        payload.insert(
            "poster_posting_enabled".to_string(),
            serde_json::Value::Bool(
                state
                    .settings
                    .get_poster_posting_enabled()
                    .await
                    .map_err(read_err)?,
            ),
        );
        payload.insert(
            "poster_category_management_enabled".to_string(),
            serde_json::Value::Bool(
                state
                    .settings
                    .get_poster_category_management_enabled()
                    .await
                    .map_err(read_err)?,
            ),
        );
    }
    if is_admin {
        payload.insert(
            "commenter_invites_enabled".to_string(),
            serde_json::Value::Bool(
                state
                    .settings
                    .get_commenter_invites_enabled()
                    .await
                    .map_err(read_err)?,
            ),
        );
        payload.insert(
            "fb_import_enabled".to_string(),
            serde_json::Value::Bool(
                state
                    .settings
                    .get_fb_import_enabled()
                    .await
                    .map_err(read_err)?,
            ),
        );
    }
    Ok(Json(serde_json::Value::Object(payload)))
}

/// Guard that returns an error when OIDC is enabled, directing users to SSO
fn require_local_auth(oidc_enabled: bool) -> AppResult<()> {
    if oidc_enabled {
        return Err(AppError::AuthError(
            "Local authentication is disabled. Please use Single Sign-On (SSO).".to_string(),
        ));
    }
    Ok(())
}

// ==================== MFA Endpoints ====================

/// Helper to get authenticated user, returning error if not logged in
async fn get_authenticated_user(auth_session: &UserAuthSession) -> AppResult<super::UserAuth> {
    auth_session
        .user()
        .await
        .ok_or_else(|| AppError::AuthError("Not authenticated".to_string()))
}

#[derive(Serialize)]
struct MfaSetupResponse {
    secret: String,
    qr_code: String,
    otpauth_url: String,
}

/// Generate a new TOTP secret and QR code for MFA setup
/// Requires full authentication (not pending MFA)
async fn mfa_setup(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
) -> AppResult<Json<MfaSetupResponse>> {
    require_local_auth(state.oidc_enabled)?;
    let user = get_authenticated_user(&auth_session).await?;

    // Don't allow setup if MFA is already pending verification
    if user.totp_enabled && !user.mfa_verified {
        return Err(AppError::AuthError(
            "Please complete MFA verification first".to_string(),
        ));
    }

    let setup = totp::generate_secret(&user.email)
        .map_err(|e| AppError::AuthError(format!("Failed to generate TOTP secret: {}", e)))?;

    Ok(Json(MfaSetupResponse {
        secret: setup.secret_base32,
        qr_code: setup.qr_code_base64,
        otpauth_url: setup.otpauth_url,
    }))
}

#[derive(Deserialize)]
struct MfaConfirmRequest {
    secret: String,
    code: String,
}

#[derive(Serialize)]
struct MfaConfirmResponse {
    message: String,
    totp_enabled: bool,
}

/// Confirm MFA setup by verifying the code matches the secret
async fn mfa_confirm_setup(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
    session: Session,
    Json(req): Json<MfaConfirmRequest>,
) -> AppResult<Json<MfaConfirmResponse>> {
    require_local_auth(state.oidc_enabled)?;
    let user = get_authenticated_user(&auth_session).await?;

    // Verify and save the secret in one step
    // First verify the code is correct
    let is_valid = totp::verify_code(&req.secret, &req.code, &user.email)
        .map_err(|e| AppError::AuthError(format!("Failed to verify code: {}", e)))?;

    if !is_valid {
        return Err(AppError::AuthError("Invalid verification code".to_string()));
    }

    // Enable TOTP for the user (save secret to DB)
    state
        .auth_backend
        .update_totp(user.id, Some(req.secret), true)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to enable MFA: {}", e)))?;

    // Mark MFA as verified in the session (user just proved they have the authenticator)
    session
        .insert(MFA_VERIFIED_KEY, true)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to update session: {}", e)))?;

    Ok(Json(MfaConfirmResponse {
        message: "MFA enabled successfully".to_string(),
        totp_enabled: true,
    }))
}

#[derive(Deserialize)]
struct MfaVerifyRequest {
    code: String,
}

#[derive(Serialize)]
struct MfaVerifyResponse {
    message: String,
    id: uuid::Uuid,
    email: String,
}

/// Verify MFA code during login (after password authentication)
async fn mfa_verify(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
    session: Session,
    Json(req): Json<MfaVerifyRequest>,
) -> AppResult<Json<MfaVerifyResponse>> {
    require_local_auth(state.oidc_enabled)?;
    let user = get_authenticated_user(&auth_session).await?;

    // Must have MFA enabled
    if !user.totp_enabled {
        return Err(AppError::AuthError("MFA is not enabled".to_string()));
    }

    // Check if account is locked out

    // Check if already verified in this session
    let already_verified = session
        .get::<bool>(MFA_VERIFIED_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    if already_verified {
        return Err(AppError::AuthError("MFA already verified".to_string()));
    }

    // Get the decrypted TOTP secret from database
    let totp_secret = state
        .auth_backend
        .get_totp_secret(user.id)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?
        .ok_or_else(|| AppError::AuthError("TOTP secret not configured".to_string()))?;

    // Verify the code first (before checking lockout status)
    let is_valid = totp::verify_code(&totp_secret, &req.code, &user.email)
        .map_err(|e| AppError::AuthError(format!("Failed to verify code: {}", e)))?;

    if !is_valid {
        // Record failed attempt
        let (attempts, is_now_locked) = state
            .auth_backend
            .record_mfa_failure(user.id)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?;

        if is_now_locked {
            tracing::warn!(
                "MFA lockout triggered for user {} after {} failed attempts",
                user.email,
                attempts
            );
            return Err(AppError::AuthError(
                "Too many failed attempts. Account is now temporarily locked.".to_string(),
            ));
        }

        return Err(AppError::AuthError("Invalid verification code".to_string()));
    }

    // Code is valid - reset any failed attempts and mark as verified
    state
        .auth_backend
        .reset_mfa_failures(user.id)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    // Store MFA verified status in session
    session
        .insert(MFA_VERIFIED_KEY, true)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to update session: {}", e)))?;

    Ok(Json(MfaVerifyResponse {
        message: "MFA verified successfully".to_string(),
        id: user.id,
        email: user.email,
    }))
}

#[derive(Deserialize)]
struct MfaDisableRequest {
    password: String,
}

#[derive(Serialize)]
struct MfaDisableResponse {
    message: String,
    totp_enabled: bool,
}

/// Disable MFA for the user (requires password confirmation)
async fn mfa_disable(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
    session: Session,
    Json(req): Json<MfaDisableRequest>,
) -> AppResult<Json<MfaDisableResponse>> {
    require_local_auth(state.oidc_enabled)?;
    let user = get_authenticated_user(&auth_session).await?;

    // Must be fully authenticated - check session for MFA verification
    let mfa_verified = session
        .get::<bool>(MFA_VERIFIED_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    if user.totp_enabled && !mfa_verified {
        return Err(AppError::AuthError(
            "Please complete MFA verification first".to_string(),
        ));
    }

    // Get the user from database to verify password
    let admin = state
        .auth_backend
        .get_admin_by_id(user.id)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    // Verify password
    let password_valid = verify_password(&req.password, &admin.password_hash)
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    if !password_valid {
        return Err(AppError::AuthError("Invalid password".to_string()));
    }

    // Disable TOTP
    state
        .auth_backend
        .update_totp(user.id, None, false)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to disable MFA: {}", e)))?;

    // Remove MFA verified flag from session (no longer needed)
    session
        .remove::<bool>(MFA_VERIFIED_KEY)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to update session: {}", e)))?;

    Ok(Json(MfaDisableResponse {
        message: "MFA disabled successfully".to_string(),
        totp_enabled: false,
    }))
}

// ==================== Password Management Endpoints ====================

#[derive(Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

#[derive(Serialize)]
struct ChangePasswordResponse {
    message: String,
}

/// Change password for the authenticated user (requires current password)
async fn change_password(
    State(state): State<AdminState>,
    auth_session: UserAuthSession,
    session: Session,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<Json<ChangePasswordResponse>> {
    require_local_auth(state.oidc_enabled)?;
    let user = get_authenticated_user(&auth_session).await?;

    // Must be fully authenticated (MFA verified if enabled)
    let mfa_verified = session
        .get::<bool>(MFA_VERIFIED_KEY)
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    if user.totp_enabled && !mfa_verified {
        return Err(AppError::AuthError(
            "Please complete MFA verification first".to_string(),
        ));
    }

    // Get the user from database to verify current password
    let admin = state
        .auth_backend
        .get_admin_by_id(user.id)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    // Verify current password
    let password_valid = verify_password(&req.current_password, &admin.password_hash)
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    if !password_valid {
        return Err(AppError::AuthError(
            "Current password is incorrect".to_string(),
        ));
    }

    // Validate new password
    if let Err(errors) = PasswordValidator::validate(&req.new_password, &user.email) {
        return Err(AppError::ValidationError(errors.join("; ")));
    }

    // Change the password (force_change = false since user is changing their own)
    state
        .auth_backend
        .change_password(user.id, &req.new_password, false)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to change password: {}", e)))?;

    // Send notification email
    if let Err(e) = state
        .email_service
        .send_password_changed_notification(&user.email, false)
        .await
    {
        tracing::warn!("Failed to send password change notification: {}", e);
    }

    Ok(Json(ChangePasswordResponse {
        message: "Password changed successfully".to_string(),
    }))
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ForgotPasswordRequest {
    email: String,
}

#[derive(Serialize)]
struct ForgotPasswordResponse {
    requires_mfa: bool,
    message: String,
}

/// Request password reset (always returns requires_mfa: true for enumeration protection)
async fn forgot_password(
    State(state): State<AdminState>,
    Json(_req): Json<ForgotPasswordRequest>,
) -> AppResult<Json<ForgotPasswordResponse>> {
    require_local_auth(state.oidc_enabled)?;

    // Always return the same response regardless of whether user exists or has an active
    // cooldown. This prevents email enumeration via response differences.
    // The actual cooldown enforcement happens in create_password_reset_token,
    // which checks token expiry before creating a new one.
    Ok(Json(ForgotPasswordResponse {
        requires_mfa: true,
        message: "Please enter your MFA code to continue".to_string(),
    }))
}

#[derive(Deserialize)]
struct ForgotPasswordVerifyMfaRequest {
    email: String,
    code: String,
}

#[derive(Serialize)]
struct ForgotPasswordVerifyMfaResponse {
    message: String,
}

/// Verify MFA for password reset (uses strict verification with zero grace period)
async fn forgot_password_verify_mfa(
    State(state): State<AdminState>,
    Json(req): Json<ForgotPasswordVerifyMfaRequest>,
) -> AppResult<Json<ForgotPasswordVerifyMfaResponse>> {
    require_local_auth(state.oidc_enabled)?;

    // Get user by email
    let admin = state
        .auth_backend
        .get_admin_by_email(&req.email)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    // For enumeration protection: if user doesn't exist, return same error as wrong MFA code
    let admin = match admin {
        Some(a) => a,
        None => {
            return Err(AppError::AuthError("Invalid verification code".to_string()));
        }
    };

    // Re-check cooldown
    if let Some(expires_at) = admin.password_reset_token_expires_at {
        if chrono::Utc::now() < expires_at.with_timezone(&chrono::Utc) {
            return Err(AppError::AuthError(
                "Password reset already requested. Please wait for the current request to expire."
                    .to_string(),
            ));
        }
    }

    // Verify MFA if enabled; non-MFA users skip straight to reset token creation
    let totp_enabled = admin.totp_enabled.unwrap_or(false);
    if totp_enabled {
        // Check if account is locked out from too many failed MFA attempts
        let is_locked = state
            .auth_backend
            .is_mfa_locked(admin.id)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?;

        if is_locked {
            return Err(AppError::AuthError(
                "Account is temporarily locked due to too many failed attempts.".to_string(),
            ));
        }

        let totp_secret = state
            .auth_backend
            .get_totp_secret(admin.id)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?
            .ok_or_else(|| AppError::AuthError("Invalid verification code".to_string()))?;

        let is_valid = totp::verify_code_strict(&totp_secret, &req.code, &admin.email)
            .map_err(|e| AppError::AuthError(format!("Failed to verify code: {}", e)))?;

        if !is_valid {
            // Record failed attempt and check if lockout should trigger
            let (_attempts, is_now_locked) = state
                .auth_backend
                .record_mfa_failure(admin.id)
                .await
                .map_err(|e| AppError::AuthError(e.to_string()))?;

            if is_now_locked {
                return Err(AppError::AuthError(
                    "Too many failed attempts. Account is now temporarily locked.".to_string(),
                ));
            }

            return Err(AppError::AuthError("Invalid verification code".to_string()));
        }

        // Reset failed attempts on success
        state
            .auth_backend
            .reset_mfa_failures(admin.id)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?;
    }

    // Create reset token
    let reset_token = state
        .auth_backend
        .create_password_reset_token(&req.email)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?
        .ok_or_else(|| AppError::AuthError("Invalid verification code".to_string()))?;

    // Send password reset email
    state
        .email_service
        .send_password_reset_email(&admin.email, &reset_token)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to send reset email: {}", e)))?;

    Ok(Json(ForgotPasswordVerifyMfaResponse {
        message: "Password reset email sent. Please check your inbox.".to_string(),
    }))
}

#[derive(Deserialize)]
struct ResetPasswordRequest {
    token: String,
    new_password: String,
}

#[derive(Serialize)]
struct ResetPasswordResponse {
    message: String,
}

/// Complete password reset using token
async fn reset_password(
    State(state): State<AdminState>,
    Json(req): Json<ResetPasswordRequest>,
) -> AppResult<Json<ResetPasswordResponse>> {
    require_local_auth(state.oidc_enabled)?;

    // Validate the token first to get the user email for password validation
    let admin = state
        .auth_backend
        .validate_reset_token(&req.token)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?
        .ok_or_else(|| {
            AppError::AuthError("Invalid or expired password reset token".to_string())
        })?;

    // Validate new password
    if let Err(errors) = PasswordValidator::validate(&req.new_password, &admin.email) {
        return Err(AppError::ValidationError(errors.join("; ")));
    }

    // Reset the password
    state
        .auth_backend
        .reset_password_with_token(&req.token, &req.new_password)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    // Send notification email
    if let Err(e) = state
        .email_service
        .send_password_changed_notification(&admin.email, false)
        .await
    {
        tracing::warn!("Failed to send password change notification: {}", e);
    }

    Ok(Json(ResetPasswordResponse {
        message: "Password reset successfully. You can now log in with your new password."
            .to_string(),
    }))
}
