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
use crate::admin::auth::AdminAuthBackend;
use crate::admin::AdminUserAuth;
use crate::entities::{admin_user, AdminUser};
use crate::errors::{AppError, AppResult};
use crate::oidc::OidcService;
use axum::{
    extract::{Query, State},
    response::Redirect,
};
use axum_login::AuthSession;
use chrono::Utc;
use openidconnect::{Nonce, PkceCodeVerifier};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use tower_sessions::Session;
use uuid::Uuid;

const OIDC_STATE_KEY: &str = "oidc_csrf_state";
const OIDC_NONCE_KEY: &str = "oidc_nonce";
const OIDC_PKCE_VERIFIER_KEY: &str = "oidc_pkce_verifier";
use super::MFA_VERIFIED_KEY;

type OidcAuthSession = AuthSession<AdminAuthBackend>;

#[derive(Clone)]
pub struct OidcState {
    pub oidc_service: OidcService,
    pub db: DatabaseConnection,
}

/// GET /api/admin/oidc/login
/// Redirects user to Keycloak authorization endpoint
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

/// GET /api/admin/oidc/callback
/// Handles the redirect back from Keycloak after authentication
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

    // Find or create local admin user
    let admin = find_or_create_oidc_user(
        &state.db,
        &user_info.email,
        user_info.email_verified,
        &user_info.roles,
        &state.oidc_service.config.admin_role,
    )
    .await
    .map_err(|e| AppError::AuthError(format!("User sync failed: {}", e)))?;

    if !admin.active {
        return Err(AppError::AuthError(
            "Account has been deactivated".to_string(),
        ));
    }

    // Create AdminUserAuth and log in via axum-login session
    let session_hash = crate::admin::auth::compute_session_hash(
        &admin.password_hash,
        &admin.email,
        admin.totp_secret.as_deref(),
    );
    let user_auth = AdminUserAuth {
        id: admin.id,
        email: admin.email.clone(),
        email_verified: admin.email_verified,
        totp_enabled: false, // MFA handled by Keycloak
        mfa_verified: true,  // Already authenticated via OIDC
        active: admin.active,
        force_password_change: false, // Not applicable for OIDC users
        role: admin.role,
        session_hash,
    };

    auth_session
        .login(&user_auth)
        .await
        .map_err(|e| AppError::AuthError(format!("Session login failed: {}", e)))?;

    // Mark MFA as verified (OIDC handles MFA at the IdP level)
    session
        .insert(MFA_VERIFIED_KEY, true)
        .await
        .map_err(|e| AppError::AuthError(format!("Session error: {}", e)))?;

    tracing::info!("OIDC login successful for user: {}", admin.email);

    Ok(Redirect::temporary("/admin"))
}

/// Find existing user by email or create a new one from OIDC claims.
/// Role is mapped from Keycloak claims on every login.
async fn find_or_create_oidc_user(
    db: &DatabaseConnection,
    email: &str,
    email_verified: bool,
    roles: &[String],
    admin_role_name: &str,
) -> anyhow::Result<admin_user::Model> {
    let app_role = if roles.iter().any(|r| r == admin_role_name) {
        "administrator"
    } else {
        "viewer"
    };

    let existing = AdminUser::find()
        .filter(admin_user::Column::Email.eq(email))
        .one(db)
        .await?;

    if let Some(existing_user) = existing {
        let mut active: admin_user::ActiveModel = existing_user.into();
        active.email_verified = Set(email_verified);
        active.role = Set(app_role.to_string());
        active.updated_at = Set(Utc::now().into());
        let updated = active.update(db).await?;
        return Ok(updated);
    }

    // Create new user from OIDC claims
    // Password hash is set to a random unusable value since OIDC users don't use passwords
    let random_hash = format!("oidc_user_{}", Uuid::new_v4());

    let new_user = admin_user::ActiveModel {
        id: Set(Uuid::new_v4()),
        email: Set(email.to_string()),
        password_hash: Set(random_hash),
        email_verified: Set(email_verified),
        verification_token: Set(None),
        verification_token_expires_at: Set(None),
        created_at: Set(Utc::now().into()),
        updated_at: Set(Utc::now().into()),
        totp_secret: Set(None),
        totp_enabled: Set(Some(false)),
        totp_enabled_at: Set(None),
        mfa_failed_attempts: Set(Some(0)),
        mfa_locked_until: Set(None),
        active: Set(true),
        deactivated_at: Set(None),
        force_password_change: Set(false),
        password_reset_token: Set(None),
        password_reset_token_expires_at: Set(None),
        role: Set(app_role.to_string()),
    };

    let result = new_user.insert(db).await?;
    tracing::info!("Created new OIDC user: {} with role: {}", email, app_role);
    Ok(result)
}
