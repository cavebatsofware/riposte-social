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
//! Invite-code creation, validation, and acceptance helpers.
//!
//! Commenter onboarding goes through this module. Admins create codes; visitors
//! land at `/invite/{code}` which sets a `pending_invite` cookie and serves the
//! social SPA. The SPA polls `/api/invites/current` to render a welcome splash;
//! clicking through into OIDC binds the code to the new commenter's user row.

use crate::entities::{invite_code, InviteCode};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Extension, Router,
};
use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder,
    Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Cookie carrying a pending invite from the splash page through OIDC sign-in.
/// The cookie's Max-Age matches `invite_code.expires_at`, so the splash returns
/// up to the admin-set expiry. SameSite=Lax keeps it on top-level navigations
/// (including the OIDC redirect chain), while Secure is set in production via
/// the same env-driven path used by tower-sessions.
pub const PENDING_INVITE_COOKIE: &str = "pending_invite";

/// Maximum lifetime an admin can set on an invite. Codes that linger longer
/// than this are a phishing footgun; the admin can always recreate.
pub const MAX_INVITE_LIFETIME_DAYS: i64 = 30;

/// Default lifetime when an admin doesn't specify `expires_in_hours`.
pub const DEFAULT_INVITE_LIFETIME_HOURS: i64 = 7 * 24;

#[derive(Clone)]
pub struct InviteState {
    pub db: DatabaseConnection,
    pub auth_backend: crate::admin::UserAuthBackend,
    /// Mirror of OidcConfig::enabled so the password-mode acceptance endpoint
    /// can short-circuit when SSO is the active auth mode.
    pub oidc_enabled: bool,
    pub settings: crate::settings::SettingsService,
}

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
/// only the routes that need it (not the SPA landing or splash polling).
pub fn auth_invite_routes(
    auth_rate_limiter: basic_axum_rate_limit::RateLimiter<crate::security_callbacks::AppRateLimitCallbacks>,
) -> Router<InviteState> {
    use axum::middleware::from_fn_with_state;
    use basic_axum_rate_limit::rate_limit_middleware;
    Router::new()
        .route("/api/auth/invite/accept-password", post(accept_invite_password))
        .layer(from_fn_with_state(auth_rate_limiter, rate_limit_middleware))
}

// ==================== Token generation ====================

/// 32-char alphanumeric invite code with ~190 bits of entropy, well past the
/// guessing threshold even with parallel attackers, and short enough to fit
/// neatly in a URL or chat message.
fn generate_invite_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Blake2b digest of the plaintext code, hex-encoded. The DB stores this
/// digest in the `code` column instead of the plaintext, so a database read
/// alone yields no usable invite codes (Blake2b on a 190-bit alphanumeric
/// is computationally infeasible to invert). Lookup remains O(1) via the
/// existing unique index because the digest is deterministic.
fn hash_invite_code(plaintext: &str) -> String {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

// ==================== Domain helpers ====================

/// Returns the live invite matching the supplied plaintext `code`, or `None`
/// if it doesn't exist, has been revoked, has been used, or has expired.
/// Callers don't get to distinguish these cases at the API surface; knowing
/// *why* a code is invalid leaks information about prior issuance.
///
/// The DB stores `hash_invite_code(plaintext)` in the `code` column rather
/// than the plaintext itself, so this function hashes the input and looks
/// the row up by indexed equality (O(1) via the unique index on `code`).
pub async fn validate_invite_code(
    db: &DatabaseConnection,
    code: &str,
) -> AppResult<Option<invite_code::Model>> {
    let hashed = hash_invite_code(code);
    let row = InviteCode::find()
        .filter(invite_code::Column::Code.eq(hashed))
        .one(db)
        .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    if row.revoked_at.is_some() {
        return Ok(None);
    }
    if row.used_at.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    if row.expires_at.with_timezone(&Utc) <= now {
        return Ok(None);
    }

    Ok(Some(row))
}

/// Marks an invite as accepted by the given user. Idempotent in the sense that
/// re-running for an already-used invite is a no-op (we don't overwrite the
/// existing `used_by_user_id`); but the caller should always validate first.
pub async fn mark_used(
    db: &DatabaseConnection,
    invite_id: Uuid,
    user_id: Uuid,
) -> AppResult<()> {
    let row = InviteCode::find_by_id(invite_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::AuthError("Invite not found".to_string()))?;

    if row.used_at.is_some() {
        return Ok(());
    }

    let mut active: invite_code::ActiveModel = row.into();
    active.used_at = Set(Some(Utc::now().into()));
    active.used_by_user_id = Set(Some(user_id));
    active.update(db).await?;
    Ok(())
}

/// Create an invite_code with the default lifetime, used both by the admin
/// `POST /api/admin/invite-codes` route and by the auto-issue path inside
/// `POST /api/admin/users`. Centralizing the construction keeps the code-
/// generation, hashing, expiry math, and column defaults in one place.
/// Accepts any `ConnectionTrait` so it can run inside a transaction (when
/// paired with a user-row insert that references the new invite's id).
///
/// Returns the inserted row AND the plaintext code. The plaintext exists
/// only in this call's return value: the DB stores only the hash, so the
/// admin must capture and deliver the plaintext at issuance time. There is
/// no way to recover it later.
pub async fn issue_invite_for_user<C>(
    db: &C,
    created_by: Uuid,
    email_hint: Option<String>,
) -> AppResult<(invite_code::Model, String)>
where
    C: sea_orm::ConnectionTrait,
{
    let plaintext = generate_invite_code();
    let expires_at = Utc::now() + Duration::hours(DEFAULT_INVITE_LIFETIME_HOURS);
    let active = invite_code::ActiveModel {
        id: Set(Uuid::new_v4()),
        code: Set(hash_invite_code(&plaintext)),
        email_hint: Set(email_hint),
        created_by: Set(created_by),
        expires_at: Set(expires_at.into()),
        used_at: Set(None),
        used_by_user_id: Set(None),
        revoked_at: Set(None),
        ..Default::default()
    };
    let inserted = active.insert(db).await?;
    Ok((inserted, plaintext))
}

// ==================== Admin endpoints ====================

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    /// Optional name/email hint for the recipient. Surfaces in the admin list
    /// and in the splash to help the visitor confirm the invite is for them.
    pub email_hint: Option<String>,
    /// Lifetime in hours. Capped at MAX_INVITE_LIFETIME_DAYS; defaults to one week.
    pub expires_in_hours: Option<i64>,
}

/// Admin-facing view of an invite. The plaintext `code` is `Some` only on
/// the creation response (where the admin/auto-issuer needs to deliver it
/// out-of-band). Listing and revoke responses leave it `None` because the
/// DB only stores the hash and there is no way to recover the plaintext
/// after issuance.
#[derive(Serialize)]
pub struct InviteResponse {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub email_hint: Option<String>,
    pub created_by: Uuid,
    pub expires_at: String,
    pub created_at: String,
    pub used_at: Option<String>,
    pub used_by_user_id: Option<Uuid>,
    pub revoked_at: Option<String>,
    pub status: &'static str,
}

impl InviteResponse {
    /// Build a response that exposes the plaintext code. Used at issuance time.
    pub fn issued(m: invite_code::Model, plaintext: String) -> Self {
        let mut r = Self::from(m);
        r.code = Some(plaintext);
        r
    }
}

impl From<invite_code::Model> for InviteResponse {
    fn from(m: invite_code::Model) -> Self {
        let now = Utc::now();
        let status = if m.revoked_at.is_some() {
            "revoked"
        } else if m.used_at.is_some() {
            "used"
        } else if m.expires_at.with_timezone(&Utc) <= now {
            "expired"
        } else {
            "active"
        };
        Self {
            id: m.id,
            code: None,
            email_hint: m.email_hint,
            created_by: m.created_by,
            expires_at: m.expires_at.with_timezone(&Utc).to_rfc3339(),
            created_at: m.created_at.with_timezone(&Utc).to_rfc3339(),
            used_at: m.used_at.map(|t| t.with_timezone(&Utc).to_rfc3339()),
            used_by_user_id: m.used_by_user_id,
            revoked_at: m.revoked_at.map(|t| t.with_timezone(&Utc).to_rfc3339()),
            status,
        }
    }
}

async fn create_invite(
    State(state): State<InviteState>,
    Extension(current): Extension<crate::admin::UserAuth>,
    Json(req): Json<CreateInviteRequest>,
) -> AppResult<(StatusCode, Json<InviteResponse>)> {
    // Fail closed: settings read errors surface as 500 rather than
    // silently allowing issuance during a transient DB problem.
    let invites_enabled = state
        .settings
        .get_commenter_invites_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !invites_enabled {
        return Err(AppError::AuthError(
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
    let inserted = active.insert(&state.db).await?;
    Ok((
        StatusCode::CREATED,
        Json(InviteResponse::issued(inserted, plaintext)),
    ))
}

async fn list_invites(
    State(state): State<InviteState>,
    _user: AuthenticatedUser,
) -> AppResult<Json<Vec<InviteResponse>>> {
    let rows = InviteCode::find()
        .order_by(invite_code::Column::CreatedAt, Order::Desc)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

async fn revoke_invite(
    State(state): State<InviteState>,
    _user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<InviteResponse>> {
    let row = InviteCode::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Invite not found".to_string()))?;

    // If already used, revocation is a no-op; the commenter is already onboarded.
    if row.used_at.is_some() {
        return Ok(Json(row.into()));
    }
    if row.revoked_at.is_some() {
        return Ok(Json(row.into()));
    }

    let mut active: invite_code::ActiveModel = row.into();
    active.revoked_at = Set(Some(Utc::now().into()));
    let updated = active.update(&state.db).await?;
    Ok(Json(updated.into()))
}

// ==================== Public endpoints ====================

/// `GET /invite/{code}`. Entry point from the email/chat link an admin shares.
/// Serves the social SPA so React Router can render the trusted-device +
/// cookie-consent gate. The cookie is intentionally NOT set here; the SPA
/// posts to `/api/invites/confirm` only after explicit user consent, so a
/// visitor on a shared device can decline to leave any state behind.
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

/// Cookie `Secure` flag follows the same DEV_MODE convention as tower-sessions.
fn is_cookie_secure() -> bool {
    std::env::var("DEV_MODE")
        .map(|v| v != "true")
        .unwrap_or(true)
}


#[derive(Serialize)]
pub struct CurrentInviteResponse {
    pub code: String,
    pub email_hint: Option<String>,
    pub expires_at: String,
}

/// Reads the `pending_invite` cookie, validates the code is still live, and
/// returns invite details for the splash modal, or `null` if no live invite.
async fn current_invite(
    State(state): State<InviteState>,
    cookies: axum_extra::extract::CookieJar,
) -> AppResult<Json<Option<CurrentInviteResponse>>> {
    let Some(cookie) = cookies.get(PENDING_INVITE_COOKIE) else {
        return Ok(Json(None));
    };
    let code = cookie.value().to_string();

    let row = match validate_invite_code(&state.db, &code).await? {
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

#[derive(Deserialize)]
pub struct ConfirmInviteRequest {
    pub code: String,
}

/// `POST /api/invites/confirm`. Called by the social SPA after the visitor
/// confirms (a) they're on a trusted/private device and (b) they consent to
/// the necessary cookies. Validates the supplied plaintext code, sets the
/// `pending_invite` cookie with Max-Age tied to the invite's expiry, and
/// returns the same shape as `/api/invites/current`. Returns `null` (200)
/// when the code is invalid, matching the polling endpoint's contract.
async fn confirm_invite(
    State(state): State<InviteState>,
    cookies: axum_extra::extract::CookieJar,
    Json(req): Json<ConfirmInviteRequest>,
) -> AppResult<(axum_extra::extract::CookieJar, Json<Option<CurrentInviteResponse>>)> {
    // Kill switch: refuse to set the pending_invite cookie when commenter
    // invites are off. Returning the same `None` shape as a stale/revoked
    // invite means the SPA renders its "no live invite" state without
    // distinguishing the kill-switch case (operators see the gate state
    // in the admin UI; commenters don't need to). Fail closed on settings
    // read errors — a 500 here forces the SPA to retry rather than
    // silently letting a stale code persist.
    let invites_enabled = state
        .settings
        .get_commenter_invites_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !invites_enabled {
        return Ok((cookies, Json(None)));
    }

    let row = match validate_invite_code(&state.db, &req.code).await? {
        Some(r) => r,
        None => return Ok((cookies, Json(None))),
    };

    let secure = is_cookie_secure();
    let cookie = build_pending_invite_cookie(
        &req.code,
        row.expires_at.with_timezone(&Utc),
        secure,
    );
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

/// Explicit cookie clear for the SPA's "decline" action.
async fn clear_pending_invite(
    cookies: axum_extra::extract::CookieJar,
) -> (StatusCode, axum_extra::extract::CookieJar) {
    let removed = remove_pending_invite_cookie(cookies);
    (StatusCode::NO_CONTENT, removed)
}

/// Build the `Set-Cookie` for a fresh pending invite. Max-Age matches the
/// invite's expiry exactly so the splash stops auto-surfacing once the code
/// can't be accepted anyway.
pub fn build_pending_invite_cookie(
    code: &str,
    expires_at: chrono::DateTime<Utc>,
    secure: bool,
) -> axum_extra::extract::cookie::Cookie<'static> {
    let max_age = (expires_at - Utc::now()).max(Duration::seconds(0));
    let mut c = axum_extra::extract::cookie::Cookie::new(
        PENDING_INVITE_COOKIE,
        code.to_owned(),
    );
    c.set_path("/");
    c.set_http_only(true);
    c.set_same_site(axum_extra::extract::cookie::SameSite::Lax);
    c.set_secure(secure);
    c.set_max_age(time::Duration::seconds(max_age.num_seconds()));
    c
}

pub fn remove_pending_invite_cookie(
    cookies: axum_extra::extract::CookieJar,
) -> axum_extra::extract::CookieJar {
    let mut removal = axum_extra::extract::cookie::Cookie::from(PENDING_INVITE_COOKIE);
    removal.set_path("/");
    cookies.remove(removal)
}

// ==================== Password-mode invite acceptance (Flow C) ====================

#[derive(Deserialize)]
pub struct AcceptInvitePasswordRequest {
    pub code: String,
    pub email: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct AcceptInvitePasswordResponse {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub message: String,
}

/// `POST /api/auth/invite/accept-password`. Password-mode invite acceptance.
/// Used when OIDC is disabled. Dispatches between Flow C.1 (bind a pre-
/// provisioned admin/poster row whose invite carries an email_hint matching
/// the form's email) and Flow C.2 (mint a brand-new commenter when no row
/// matches). Establishes a session on success.
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

    // Kill switch: when `commenter_invites_enabled` is off, refuse the
    // acceptance even if the code is otherwise valid. Existing
    // already-activated users are unaffected — they log in via the
    // normal password path which doesn't touch this handler. Fail closed
    // on settings read errors so a transient DB hiccup never permits a
    // gated acceptance.
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

    let invite = validate_invite_code(&state.db, &req.code).await?.ok_or_else(|| {
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

    let user_auth = state.auth_backend.user_auth_for_invite_accept(updated.clone());
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

