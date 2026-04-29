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
use crate::admin::pagination::{Paginated, PaginationParams};
use crate::admin::password::PasswordValidator;
use crate::admin::UserAuthBackend;
use crate::email::EmailService;
use crate::entities::{user, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AdminUserState {
    pub db: DatabaseConnection,
    pub auth_backend: UserAuthBackend,
    pub email_service: Arc<EmailService>,
}

pub fn admin_user_routes() -> Router<AdminUserState> {
    Router::new()
        .route(
            "/api/admin/users",
            get(list_admin_users).post(create_admin_user),
        )
        .route(
            "/api/admin/users/{id}",
            get(get_admin_user).put(update_admin_user),
        )
        .route(
            "/api/admin/users/{id}/resend-verification",
            post(resend_verification_email),
        )
}

#[derive(Serialize)]
pub struct AdminUserResponse {
    id: Uuid,
    email: String,
    email_verified: bool,
    totp_enabled: bool,
    mfa_locked: bool,
    mfa_failed_attempts: i32,
    active: bool,
    deactivated_at: Option<String>,
    force_password_change: bool,
    role: String,
    display_name: Option<String>,
    oidc_linked: bool,
    last_login_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<user::Model> for AdminUserResponse {
    fn from(model: user::Model) -> Self {
        let now = Utc::now();
        let mfa_locked = model
            .mfa_locked_until
            .map(|t| t.with_timezone(&Utc) > now)
            .unwrap_or(false);

        Self {
            id: model.id,
            email: model.email,
            email_verified: model.email_verified,
            totp_enabled: model.totp_enabled.unwrap_or(false),
            mfa_locked,
            mfa_failed_attempts: model.mfa_failed_attempts.unwrap_or(0),
            active: model.active,
            deactivated_at: model
                .deactivated_at
                .map(|t| t.with_timezone(&Utc).to_rfc3339()),
            force_password_change: model.force_password_change,
            role: model.role,
            display_name: model.display_name,
            oidc_linked: model.oidc_sub.is_some(),
            last_login_at: model
                .last_login_at
                .map(|t| t.with_timezone(&Utc).to_rfc3339()),
            created_at: model.created_at.with_timezone(&Utc).to_rfc3339(),
            updated_at: model.updated_at.with_timezone(&Utc).to_rfc3339(),
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateAdminUserRequest {
    email_verified: Option<bool>,
    reset_mfa_lockout: Option<bool>,
    disable_mfa: Option<bool>,
    active: Option<bool>,
    new_password: Option<String>,
    role: Option<String>,
}

async fn list_admin_users(
    State(state): State<AdminUserState>,
    _user: AuthenticatedUser,
    Query(params): Query<PaginationParams>,
) -> AppResult<Json<Paginated<AdminUserResponse>>> {
    let validated = params.validate();

    let paginator = User::find()
        .order_by(user::Column::CreatedAt, Order::Desc)
        .paginate(&state.db, validated.per_page);

    let total = paginator.num_items().await?;
    let total_pages = paginator.num_pages().await?;
    let users = paginator.fetch_page(validated.page - 1).await?;
    let user_responses: Vec<AdminUserResponse> = users.into_iter().map(Into::into).collect();

    Ok(Json(Paginated::new(
        user_responses,
        total,
        validated.page,
        validated.per_page,
        total_pages,
    )))
}

async fn get_admin_user(
    State(state): State<AdminUserState>,
    _user: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<AdminUserResponse>> {
    let admin = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    Ok(Json(admin.into()))
}

async fn update_admin_user(
    State(state): State<AdminUserState>,
    Extension(current_user): Extension<crate::admin::UserAuth>,
    Path(user_id): Path<Uuid>,
    Json(req): Json<UpdateAdminUserRequest>,
) -> AppResult<Json<AdminUserResponse>> {
    // Prevent self-editing
    if current_user.id == user_id {
        return Err(AppError::AuthError(
            "Cannot edit yourself. Use the Profile page instead.".to_string(),
        ));
    }

    // Fetch target user
    let admin = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    let target_email = admin.email.clone();

    // Handle deactivation/reactivation via auth backend (has protection checks)
    if let Some(active) = req.active {
        if active && !admin.active {
            // Reactivate user
            let (_, verification_token) = state
                .auth_backend
                .reactivate_user(user_id)
                .await
                .map_err(|e| AppError::AuthError(e.to_string()))?;

            // Send verification email for reactivated user
            if let Err(e) = state
                .email_service
                .send_verification_email(&target_email, &verification_token)
                .await
            {
                tracing::warn!("Failed to send verification email: {}", e);
            }
        } else if !active && admin.active {
            // Deactivate user
            state
                .auth_backend
                .deactivate_user(user_id, current_user.id)
                .await
                .map_err(|e| AppError::AuthError(e.to_string()))?;
        }
    }

    // Handle password change (admin setting password for user)
    if let Some(ref new_password) = req.new_password {
        // Validate password
        if let Err(errors) = PasswordValidator::validate(new_password, &target_email) {
            return Err(AppError::ValidationError(errors.join("; ")));
        }

        // Change password with force_change = true (user must change on next login)
        state
            .auth_backend
            .change_password(user_id, new_password, true)
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?;

        // Send notification email
        if let Err(e) = state
            .email_service
            .send_password_changed_notification(&target_email, true)
            .await
        {
            tracing::warn!("Failed to send password change notification: {}", e);
        }
    }

    // Re-fetch user for other updates
    let admin = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    let mut admin_active: user::ActiveModel = admin.into();

    if let Some(verified) = req.email_verified {
        admin_active.email_verified = Set(verified);
    }

    if req.reset_mfa_lockout == Some(true) {
        admin_active.mfa_failed_attempts = Set(Some(0));
        admin_active.mfa_locked_until = Set(None);
    }

    if req.disable_mfa == Some(true) {
        admin_active.totp_secret = Set(None);
        admin_active.totp_enabled = Set(Some(false));
        admin_active.totp_enabled_at = Set(None);
        admin_active.mfa_failed_attempts = Set(Some(0));
        admin_active.mfa_locked_until = Set(None);
    }

    if let Some(ref role) = req.role {
        let valid_roles = [
            user::ROLE_ADMINISTRATOR,
            user::ROLE_POSTER,
            user::ROLE_COMMENTER,
        ];
        if !valid_roles.contains(&role.as_str()) {
            return Err(AppError::ValidationError(format!(
                "Invalid role '{}'. Valid roles: {:?}",
                role, valid_roles
            )));
        }
        admin_active.role = Set(role.clone());
    }

    admin_active.updated_at = Set(Utc::now().into());
    let updated = admin_active.update(&state.db).await?;

    Ok(Json(updated.into()))
}

async fn resend_verification_email(
    State(state): State<AdminUserState>,
    Extension(current_user): Extension<crate::admin::UserAuth>,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<AdminUserResponse>> {
    // Prevent self-action
    if current_user.id == user_id {
        return Err(AppError::AuthError(
            "Cannot resend verification email to yourself".to_string(),
        ));
    }

    // Generate new verification token
    let (updated, verification_token) = state
        .auth_backend
        .regenerate_verification_token(user_id)
        .await
        .map_err(|e| AppError::AuthError(e.to_string()))?;

    // Send verification email
    state
        .email_service
        .send_verification_email(&updated.email, &verification_token)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to send email: {}", e)))?;

    tracing::info!(
        "Admin {} resent verification email to user {}",
        current_user.email,
        updated.email
    );

    Ok(Json(updated.into()))
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    email: String,
    /// One of: `administrator`, `poster`, `commenter`.
    role: String,
}

#[derive(Serialize)]
pub struct CreateUserResponse {
    user: AdminUserResponse,
    invite: crate::invites::InviteResponse,
}

/// POST /api/admin/users: admin-only endpoint to pre-provision a new account.
/// The row is created in an inert state (`activated_at = NULL`, placeholder
/// `password_hash`) and an invite_code is auto-issued with `email_hint = email`.
/// The admin shares the returned invite link with the recipient, who completes
/// activation via `/invite/{code}` (OIDC sign-in or password-set form).
///
/// The admin never types the user's password; this avoids the operational
/// hazard of an admin briefly knowing another user's credential.
async fn create_admin_user(
    State(state): State<AdminUserState>,
    Extension(current_user): Extension<crate::admin::UserAuth>,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<CreateUserResponse>)> {
    let valid_roles = [
        user::ROLE_ADMINISTRATOR,
        user::ROLE_POSTER,
        user::ROLE_COMMENTER,
    ];
    if !valid_roles.contains(&req.role.as_str()) {
        return Err(AppError::ValidationError(format!(
            "Invalid role '{}'. Valid roles: {:?}",
            req.role, valid_roles
        )));
    }

    let existing = User::find()
        .filter(user::Column::Email.eq(&req.email))
        .one(&state.db)
        .await?;
    if existing.is_some() {
        return Err(AppError::ValidationError(
            "User with this email already exists".to_string(),
        ));
    }

    // Issue the invite first, then create the user with `invite_code_id`
    // pre-set. Wrap in a transaction so we never end up with a user row
    // pointing at an invite that didn't make it (or vice versa). The user
    // row references `invite_code_id` directly so bind-path lookups can use
    // it as the lookup key, so only the issued invite can activate this row.
    let txn = state.db.begin().await?;

    let (invite, invite_plaintext) =
        crate::invites::issue_invite_for_user(&txn, current_user.id, Some(req.email.clone()))
            .await
            .map_err(|e| AppError::AuthError(e.to_string()))?;

    // Placeholder hash. The row is inert until invite acceptance, at which
    // point either Flow A.1 (OIDC bind) rotates this to a non-usable OIDC
    // marker, or Flow C.1 (password bind) replaces this with the user's
    // chosen argon2 hash. We seed with a real argon2 hash of a discarded
    // random secret (rather than a non-PHC string) so `verify_password` on
    // login probes returns `Ok(false)` indistinguishably from the dummy-hash
    // path for non-existent users, instead of erroring on hash parse and
    // leaking inert-row existence via response-body discrimination. The
    // activated_at gate is the actual security boundary.
    let placeholder_hash = crate::admin::auth::placeholder_password_hash()
        .map_err(|e| AppError::AuthError(format!("Failed to create placeholder: {}", e)))?;
    let new_user = user::ActiveModel {
        id: Set(Uuid::new_v4()),
        email: Set(req.email.clone()),
        password_hash: Set(placeholder_hash),
        email_verified: Set(false),
        verification_token: Set(None),
        verification_token_expires_at: Set(None),
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
        role: Set(req.role.clone()),
        oidc_sub: Set(None),
        display_name: Set(None),
        avatar_url: Set(None),
        last_login_at: Set(None),
        invite_code_id: Set(Some(invite.id)),
        activated_at: Set(None),
        ..Default::default()
    };
    let created = new_user.insert(&txn).await?;

    txn.commit().await?;

    // Best-effort email delivery. If SES is misconfigured or the recipient's
    // address is invalid we still return the invite link in the response so
    // the admin can share it manually.
    if let Err(e) = state
        .email_service
        .send_invite_email(
            &created.email,
            &invite_plaintext,
            &created.role,
            &current_user.email,
        )
        .await
    {
        tracing::warn!(
            "Failed to email invite for {} (admin can copy link manually): {}",
            created.email,
            e
        );
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateUserResponse {
            user: created.into(),
            invite: crate::invites::InviteResponse::issued(invite, invite_plaintext),
        }),
    ))
}
