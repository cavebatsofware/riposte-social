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
use sea_orm::Set;
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
    // Unified user model
    pub oidc_sub: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub last_login_at: Option<DateTimeWithTimeZone>,
    // FK to invite_code.id.
    pub invite_code_id: Option<Uuid>,
    /// Set when the row's invite has been accepted (or, for the bootstrap
    /// admin, at creation time). NULL means the row is inert; no login of
    /// any kind can establish a session until activation.
    pub activated_at: Option<DateTimeWithTimeZone>,
    /// Public handle, like a GitHub username. NOT NULL, unique. The
    /// migration backfills it from the email local-part. App layer enforces
    /// length 3-30 and the [a-z0-9_-] charset.
    pub handle: String,
    /// Self-described bio, capped at 500 chars at the app layer.
    pub bio: Option<String>,
    /// Pronouns string (free-form), capped at 30 chars at the app layer.
    pub pronouns: Option<String>,
    /// Relative S3 key for an uploaded avatar, e.g. `avatars/{user_id}/{uuid}.webp`.
    /// Distinct from `avatar_url` (which is reserved for an external URL the
    /// IdP might supply later); the API renders an `avatar_url` field that
    /// prefers the S3-served route over the external URL.
    pub avatar_s3_key: Option<String>,
    /// User's saved UI locale. NULL means no explicit choice
    /// yet; the social-frontend's i18next browser-language detector
    /// picks one from `navigator.language`. App-layer validation in
    /// `crate::profile::locale::SUPPORTED_LOCALES` is the source of truth
    /// for which codes are accepted.
    pub locale: Option<String>,
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

    pub fn is_activated(&self) -> bool {
        self.activated_at.is_some()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// Auto-manage `created_at` (set once on insert) and `updated_at` (set on
    /// every insert and update). Callers no longer need to stamp these by
    /// hand; explicit `Set(...)` values still work but are redundant.
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now: DateTimeWithTimeZone = chrono::Utc::now().into();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
