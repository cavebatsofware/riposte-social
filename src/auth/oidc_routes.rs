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
    response::Redirect,
};
use axum_login::AuthSession;
use openidconnect::{Nonce, PkceCodeVerifier};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use tower_sessions::Session;

const OIDC_STATE_KEY: &str = "oidc_csrf_state";
const OIDC_NONCE_KEY: &str = "oidc_nonce";
const OIDC_PKCE_VERIFIER_KEY: &str = "oidc_pkce_verifier";
use crate::admin::MFA_VERIFIED_KEY;

type OidcAuthSession = AuthSession<UserAuthBackend>;

#[derive(Clone)]
pub struct OidcState {
    pub oidc_service: OidcService,
    pub db: DatabaseConnection,
}

/// GET /api/auth/oidc/login
/// Redirects user to Keycloak authorization endpoint.
pub async fn oidc_login(State(state): State<OidcState>, session: Session) -> AppResult<Redirect> {
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
    Query(params): Query<OidcCallbackParams>,
) -> AppResult<Redirect> {
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

    // Map IdP roles to an app role. OIDC logins without the admin claim are
    // provisioned as commenters; Phase 2 will gate this on invite acceptance,
    // but for now any successful OIDC login becomes a commenter if not flagged
    // as admin.
    let app_role = if user_info
        .roles
        .iter()
        .any(|r| r == &state.oidc_service.config.admin_role)
    {
        user::ROLE_ADMINISTRATOR
    } else {
        user::ROLE_COMMENTER
    };

    let creds = Credentials::Oidc {
        sub: user_info.sub,
        email: user_info.email.clone(),
        email_verified: user_info.email_verified,
        role: app_role.to_string(),
        display_name: user_info.display_name,
        invite_code: None, // Phase 2 wires this from the pending_invite cookie.
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
        app_role
    );

    // Administrators land in the admin panel; posters and commenters land on
    // the social feed where they spend most of their time.
    let redirect_path = if app_role == user::ROLE_ADMINISTRATOR {
        "/admin"
    } else {
        "/"
    };

    Ok(Redirect::temporary(redirect_path))
}
