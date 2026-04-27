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
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

// Role values stored in `role` column. `viewer` is retired.
// The `role` is the single source of truth for tier; `user_type` was retired
// in m20260425 because it was redundant with `role`.
pub const ROLE_ADMINISTRATOR: &str = "administrator";
pub const ROLE_POSTER: &str = "poster";
pub const ROLE_COMMENTER: &str = "commenter";

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    pub password_hash: String,
    pub email_verified: bool,
    pub verification_token: Option<String>,
    pub verification_token_expires_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    // TOTP MFA fields
    pub totp_secret: Option<String>,
    pub totp_enabled: Option<bool>,
    pub totp_enabled_at: Option<DateTimeWithTimeZone>,
    // MFA lockout fields
    pub mfa_failed_attempts: Option<i32>,
    pub mfa_locked_until: Option<DateTimeWithTimeZone>,
    // User management fields
    pub active: bool,
    pub deactivated_at: Option<DateTimeWithTimeZone>,
    pub force_password_change: bool,
    pub password_reset_token: Option<String>,
    pub password_reset_token_expires_at: Option<DateTimeWithTimeZone>,
    // Role-based access control
    pub role: String,
    // Unified user model — added in Phase 1 of the MVP plan.
    pub oidc_sub: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub last_login_at: Option<DateTimeWithTimeZone>,
    // FK to invite_code.id. Constraint added in Phase 2 when invite_code exists.
    pub invite_code_id: Option<Uuid>,
}

impl Model {
    pub fn is_administrator(&self) -> bool {
        self.role == ROLE_ADMINISTRATOR
    }

    pub fn is_poster(&self) -> bool {
        self.role == ROLE_POSTER
    }

    pub fn is_commenter(&self) -> bool {
        self.role == ROLE_COMMENTER
    }

    pub fn is_oidc_linked(&self) -> bool {
        self.oidc_sub.is_some()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
