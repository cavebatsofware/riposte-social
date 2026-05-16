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
//! HTTP handlers for invite-code admin and acceptance flows.

use crate::entities::invite_code;
use crate::errors::{AppError, AppResult};
use crate::invites::queries;
use crate::invites::types::{
    AcceptInvitePasswordRequest, AcceptInvitePasswordResponse, ConfirmInviteRequest,
    CreateInviteRequest, CurrentInviteResponse, InviteResponse,
};
use crate::invites::{
    build_pending_invite_cookie, generate_invite_code, hash_invite_code, is_cookie_secure,
    remove_pending_invite_cookie, InviteState, DEFAULT_INVITE_LIFETIME_HOURS,
    MAX_INVITE_LIFETIME_DAYS, PENDING_INVITE_COOKIE,
};
use crate::middleware::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Extension, Router,
};
use chrono::{Duration, Utc};
use sea_orm::Set;
use uuid::Uuid;

pub fn admin_invite_routes() -> Router<InviteState> {
    Router::new()
        .route(
            "/api/admin/invite-codes",
            get(list_invites).post(create_invite),
        )
        .route("/api/admin/invite-codes/{id}", delete(revoke_invite))
}

pub fn public_invite_routes() -> Router<InviteState> {
    Router::new()
        .route("/invite/{code}", get(serve_invite_landing))
        .route("/api/invites/current", get(current_invite))
        .route("/api/invites/confirm", post(confirm_invite))
        .route("/api/auth/logout/invite", post(clear_pending_invite))
}

/// Auth-tier rate-limited invite endpoints. Currently just the password-mode
/// acceptance handler. Registered separately so the auth_rate_limiter wraps
/// only the routes that need it.
pub fn auth_invite_routes(
    auth_rate_limiter: basic_axum_rate_limit::RateLimiter<
        crate::middleware::rate_limit::AppRateLimitCallbacks,
    >,
) -> Router<InviteState> {
    use axum::middleware::from_fn_with_state;
    use basic_axum_rate_limit::rate_limit_middleware;
    Router::new()
        .route(
            "/api/auth/invite/accept-password",
            post(accept_invite_password),
        )
        .layer(from_fn_with_state(auth_rate_limiter, rate_limit_middleware))
}

async fn create_invite(
    State(state): State<InviteState>,
    Extension(current): Extension<crate::admin::UserAuth>,
    Json(req): Json<CreateInviteRequest>,
) -> AppResult<(StatusCode, Json<InviteResponse>)> {
    let invites_enabled = state
        .settings
        .get_commenter_invites_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !invites_enabled {
        return Err(AppError::Forbidden(
            "Invite creation is currently disabled by an administrator".to_string(),
        ));
    }

    let lifetime_hours = req
        .expires_in_hours
        .unwrap_or(DEFAULT_INVITE_LIFETIME_HOURS);
    if lifetime_hours <= 0 {
        return Err(AppError::ValidationError(
            "expires_in_hours must be positive".to_string(),
        ));
    }
    if lifetime_hours > MAX_INVITE_LIFETIME_DAYS * 24 {
        return Err(AppError::ValidationError(format!(
            "expires_in_hours cannot exceed {} (max {} days)",
            MAX_INVITE_LIFETIME_DAYS * 24,
            MAX_INVITE_LIFETIME_DAYS
        )));
    }

    let plaintext = generate_invite_code();
    let expires_at = Utc::now() + Duration::hours(lifetime_hours);

    let active = invite_code::ActiveModel {
        id: Set(Uuid::new_v4()),
        code: Set(hash_invite_code(&plaintext)),
        email_hint: Set(req.email_hint),
        created_by: Set(current.id),
        expires_at: Set(expires_at.into()),
        used_at: Set(None),
        used_by_user_id: Set(None),
        revoked_at: Set(None),
        ..Default::default()
    };
    let inserted = queries::insert_invite(&state.db, active).await?;
    Ok((
        StatusCode::CREATED,
        Json(InviteResponse::issued(inserted, plaintext)),
    ))
}

async fn list_invites(
    State(state): State<InviteState>,
    _user: AuthenticatedUser,
) -> AppResult<Json<Vec<InviteResponse>>> {
    let rows = queries::list_invites_desc(&state.db).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

async fn revoke_invite(
    State(state): State<InviteState>,
    _user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<InviteResponse>> {
    let row = queries::find_invite(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Invite not found".to_string()))?;

    // If already used or revoked, revocation is a no-op.
    if row.used_at.is_some() {
        return Ok(Json(row.into()));
    }
    if row.revoked_at.is_some() {
        return Ok(Json(row.into()));
    }

    let mut active: invite_code::ActiveModel = row.into();
    active.revoked_at = Set(Some(Utc::now().into()));
    let updated = queries::update_invite(&state.db, active).await?;
    Ok(Json(updated.into()))
}

/// `GET /invite/{code}`. Entry point from the email/chat link an admin shares.
/// Serves the social SPA so React Router can render the trusted-device +
/// cookie-consent gate. The cookie is intentionally NOT set here; the SPA
/// posts to `/api/invites/confirm` only after explicit user consent.
async fn serve_invite_landing() -> AppResult<Response> {
    let html = tokio::fs::read_to_string("social-assets/index.html")
        .await
        .map_err(AppError::FileSystem)?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
        .into_response())
}

async fn current_invite(
    State(state): State<InviteState>,
    cookies: axum_extra::extract::CookieJar,
) -> AppResult<Json<Option<CurrentInviteResponse>>> {
    let Some(cookie) = cookies.get(PENDING_INVITE_COOKIE) else {
        return Ok(Json(None));
    };
    let code = cookie.value().to_string();

    let row = match queries::validate_invite_code(&state.db, &code).await? {
        Some(r) => r,
        None => return Ok(Json(None)),
    };

    // Echo the plaintext code from the cookie, not the row's `code` column
    // (which holds the at-rest hash). The splash needs the plaintext to
    // submit to /api/auth/invite/accept-password in password mode.
    Ok(Json(Some(CurrentInviteResponse {
        code,
        email_hint: row.email_hint,
        expires_at: row.expires_at.with_timezone(&Utc).to_rfc3339(),
    })))
}

async fn confirm_invite(
    State(state): State<InviteState>,
    cookies: axum_extra::extract::CookieJar,
    Json(req): Json<ConfirmInviteRequest>,
) -> AppResult<(
    axum_extra::extract::CookieJar,
    Json<Option<CurrentInviteResponse>>,
)> {
    // Kill switch: refuse to set the pending_invite cookie when commenter
    // invites are off. Returning `None` matches the polling endpoint's
    // contract; operators see the gate state in the admin UI.
    let invites_enabled = state
        .settings
        .get_commenter_invites_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !invites_enabled {
        return Ok((cookies, Json(None)));
    }

    let row = match queries::validate_invite_code(&state.db, &req.code).await? {
        Some(r) => r,
        None => return Ok((cookies, Json(None))),
    };

    let secure = is_cookie_secure();
    let cookie = build_pending_invite_cookie(&req.code, row.expires_at.with_timezone(&Utc), secure);
    let cookies = cookies.add(cookie);

    Ok((
        cookies,
        Json(Some(CurrentInviteResponse {
            code: req.code,
            email_hint: row.email_hint,
            expires_at: row.expires_at.with_timezone(&Utc).to_rfc3339(),
        })),
    ))
}

async fn clear_pending_invite(
    cookies: axum_extra::extract::CookieJar,
) -> (StatusCode, axum_extra::extract::CookieJar) {
    let removed = remove_pending_invite_cookie(cookies);
    (StatusCode::NO_CONTENT, removed)
}

/// `POST /api/auth/invite/accept-password`. Used when OIDC is disabled.
/// Dispatches between Flow C.1 (bind a pre-provisioned admin/poster row
/// whose invite carries an email_hint matching the form's email) and Flow
/// C.2 (mint a brand-new commenter when no row matches). Establishes a
/// session on success.
async fn accept_invite_password(
    State(state): State<InviteState>,
    auth_session: axum_login::AuthSession<crate::admin::UserAuthBackend>,
    Json(req): Json<AcceptInvitePasswordRequest>,
) -> AppResult<Json<AcceptInvitePasswordResponse>> {
    if state.oidc_enabled {
        return Err(AppError::AuthError(
            "Password-mode invite acceptance is disabled while OIDC is enabled.".to_string(),
        ));
    }

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

    if let Err(errors) =
        crate::admin::password::PasswordValidator::validate(&req.new_password, &req.email)
    {
        return Err(AppError::ValidationError(errors.join("; ")));
    }

    let invite = queries::validate_invite_code(&state.db, &req.code)
        .await?
        .ok_or_else(|| {
            AppError::AuthError("Invite is invalid, expired, revoked, or already used".to_string())
        })?;

    // Dispatch on email_hint: when set, the invite was issued for a
    // pre-provisioned row (Flow C.1). Otherwise it's a commenter invite (C.2).
    let updated = if invite.email_hint.is_some() {
        state
            .auth_backend
            .accept_invite_password_bind(&invite, &req.email, &req.new_password)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?
    } else {
        state
            .auth_backend
            .accept_invite_password_create_commenter(&invite, &req.email, &req.new_password)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?
    };

    let user_auth = state
        .auth_backend
        .user_auth_for_invite_accept(updated.clone());
    auth_session
        .login(&user_auth)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    Ok(Json(AcceptInvitePasswordResponse {
        id: updated.id,
        email: updated.email,
        role: updated.role,
        message: "Account activated. You're signed in.".to_string(),
    }))
}
