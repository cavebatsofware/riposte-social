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
use crate::admin::AdminAuthBackend;
use crate::email::EmailService;
use crate::entities::{admin_user, AdminUser};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, DatabaseConnection, EntityTrait, Order, PaginatorTrait, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AdminUserState {
    pub db: DatabaseConnection,
    pub auth_backend: AdminAuthBackend,
    pub email_service: Arc<EmailService>,
}

pub fn admin_user_routes() -> Router<AdminUserState> {
    Router::new()
        .route("/api/admin/users", get(list_admin_users))
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
    created_at: String,
    updated_at: String,
}

impl From<admin_user::Model> for AdminUserResponse {
    fn from(model: admin_user::Model) -> Self {
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

    let paginator = AdminUser::find()
        .order_by(admin_user::Column::CreatedAt, Order::Desc)
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
    let admin = AdminUser::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    Ok(Json(admin.into()))
}

async fn update_admin_user(
    State(state): State<AdminUserState>,
    Extension(current_user): Extension<crate::admin::AdminUserAuth>,
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
    let admin = AdminUser::find_by_id(user_id)
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
    let admin = AdminUser::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    let mut admin_active: admin_user::ActiveModel = admin.into();

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
        let valid_roles = ["administrator", "viewer"];
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
    Extension(current_user): Extension<crate::admin::AdminUserAuth>,
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
