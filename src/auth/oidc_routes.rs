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
use crate::admin::auth::{Credentials, UserAuthBackend};
use crate::entities::user;
use crate::errors::{AppError, AppResult};
use crate::oidc::OidcService;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use axum_login::AuthSession;
use openidconnect::{Nonce, PkceCodeVerifier};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use tower_sessions::Session;

const OIDC_STATE_KEY: &str = "oidc_csrf_state";
const OIDC_NONCE_KEY: &str = "oidc_nonce";
const OIDC_PKCE_VERIFIER_KEY: &str = "oidc_pkce_verifier";
const PENDING_INVITE_SESSION_KEY: &str = "pending_invite_code";
use crate::admin::MFA_VERIFIED_KEY;
use crate::invites::{remove_pending_invite_cookie, PENDING_INVITE_COOKIE};

type OidcAuthSession = AuthSession<UserAuthBackend>;

#[derive(Clone)]
pub struct OidcState {
    pub oidc_service: OidcService,
    pub db: DatabaseConnection,
    pub settings: crate::settings::SettingsService,
}

/// GET /api/auth/oidc/login
/// Redirects user to Keycloak authorization endpoint. If a `pending_invite`
/// cookie is present, its value is stashed in the tower-session alongside the
/// PKCE state so the callback can bind the new commenter to the right invite.
/// We only store the raw code here; full validation happens at callback time
/// to catch revocations/expiry that occur during the IdP round-trip.
pub async fn oidc_login(
    State(state): State<OidcState>,
    session: Session,
    cookies: CookieJar,
) -> AppResult<Redirect> {
    let (auth_url, csrf_token, nonce, pkce_verifier) = state
        .oidc_service
        .authorization_url()
        .map_err(|e| AppError::AuthError(format!("OIDC configuration error: {}", e)))?;

    session
        .insert(OIDC_STATE_KEY, csrf_token.secret().clone())
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;
    session
        .insert(OIDC_NONCE_KEY, nonce.secret().clone())
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;
    session
        .insert(OIDC_PKCE_VERIFIER_KEY, pkce_verifier.secret().clone())
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;

    if let Some(cookie) = cookies.get(PENDING_INVITE_COOKIE) {
        session
            .insert(PENDING_INVITE_SESSION_KEY, cookie.value().to_string())
            .await
            .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;
    } else {
        let _ = session.remove::<String>(PENDING_INVITE_SESSION_KEY).await;
    }

    Ok(Redirect::temporary(auth_url.as_str()))
}

#[derive(Deserialize)]
pub struct OidcCallbackParams {
    code: String,
    state: String,
}

/// GET /api/auth/oidc/callback
/// Handles the redirect back from Keycloak after authentication.
/// Exchanges the code, maps OIDC claims to a `Credentials::Oidc`, and lets the
/// unified `UserAuthBackend` upsert the user row and produce a session principal.
/// Redirects administrators to `/admin` and everyone else (poster, commenter)
/// to `/` since posters and commenters live primarily in the social frontend.
pub async fn oidc_callback(
    State(state): State<OidcState>,
    auth_session: OidcAuthSession,
    session: Session,
    cookies: CookieJar,
    Query(params): Query<OidcCallbackParams>,
) -> AppResult<Response> {
    // Validate CSRF state
    let stored_state: String = session
        .get(OIDC_STATE_KEY)
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?
        .ok_or_else(|| AppError::AuthError("Missing OIDC state in session".to_string()))?;

    if params.state != stored_state {
        return Err(AppError::AuthError("OIDC state mismatch".to_string()));
    }

    // Retrieve nonce and PKCE verifier from session
    let nonce_secret: String = session
        .get(OIDC_NONCE_KEY)
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?
        .ok_or_else(|| AppError::AuthError("Missing OIDC nonce in session".to_string()))?;
    let nonce = Nonce::new(nonce_secret);

    let pkce_secret: String = session
        .get(OIDC_PKCE_VERIFIER_KEY)
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?
        .ok_or_else(|| AppError::AuthError("Missing OIDC PKCE verifier".to_string()))?;
    let pkce_verifier = PkceCodeVerifier::new(pkce_secret);

    // Pull the pending invite stashed by oidc_login, if any. Removed from the
    // session unconditionally; a stale entry must not bleed into a later login.
    let pending_invite: Option<String> = session
        .remove(PENDING_INVITE_SESSION_KEY)
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;

    // Kill switch: when `commenter_invites_enabled` is off, refuse any
    // invite-driven OIDC acceptance. Already-bound users keep working
    // (Flow B in `authenticate_oidc` does not require an invite_code).
    // Fail closed on settings read errors — a 500 surfaces the underlying
    // DB problem to the operator instead of silently permitting a gated
    // acceptance.
    if pending_invite.is_some() {
        let invites_enabled = state
            .settings
            .get_commenter_invites_enabled()
            .await
            .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
        if !invites_enabled {
            return Err(AppError::AuthError(
                "Invite acceptance is currently disabled by an administrator".to_string(),
            ));
        }
    }

    // Clean up OIDC session data
    let _ = session.remove::<String>(OIDC_STATE_KEY).await;
    let _ = session.remove::<String>(OIDC_NONCE_KEY).await;
    let _ = session.remove::<String>(OIDC_PKCE_VERIFIER_KEY).await;

    // Exchange code for tokens
    let user_info = state
        .oidc_service
        .exchange_code(&params.code, pkce_verifier, &nonce)
        .await
        .map_err(|e| AppError::AuthError(format!("OIDC token exchange failed: {}", e)))?;

    // Resolve the IdP's effective tier from its role claims (most-privileged
    // wins). The backend uses this purely to validate against the DB-
    // authoritative role; it never uses the claim to assign role.
    let idp_tier = crate::oidc::resolve_idp_tier(&user_info.roles, &state.oidc_service.config);

    let creds = Credentials::Oidc {
        sub: user_info.sub,
        email: user_info.email.clone(),
        email_verified: user_info.email_verified,
        idp_tier: idp_tier.to_string(),
        display_name: user_info.display_name,
        invite_code: pending_invite,
    };

    let user_auth = auth_session
        .authenticate(creds)
        .await
        .map_err(|e| AppError::AuthError(format!("OIDC authentication failed: {}", e)))?
        .ok_or_else(|| AppError::AuthError("OIDC authentication rejected".to_string()))?;

    auth_session
        .login(&user_auth)
        .await
        .map_err(|e| AppError::AuthError(format!("Session login failed: {}", e)))?;

    // Mark MFA as verified (OIDC handles MFA at the IdP level)
    session
        .insert(MFA_VERIFIED_KEY, true)
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;

    tracing::info!(
        "OIDC login successful for user: {} role={}",
        user_info.email,
        user_auth.role
    );

    // Administrators land in the admin panel; posters and commenters land on
    // the social feed where they spend most of their time.
    let redirect_path = if user_auth.role == user::ROLE_ADMINISTRATOR {
        "/admin"
    } else {
        "/"
    };

    // Always strip the pending_invite cookie on success, whether it was
    // consumed (commenter onboarded) or ignored (admin/poster login). Keeps
    // the splash from re-surfacing for an already-bound user.
    let cleared = remove_pending_invite_cookie(cookies);

    Ok((cleared, Redirect::temporary(redirect_path)).into_response())
}
