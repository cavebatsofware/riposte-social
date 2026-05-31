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
use crate::admin::auth::credentials::{
    compute_session_hash as compute_hash_helper, generate_verification_token, hash_password,
    verify_password, DUMMY_HASH,
};
use crate::admin::auth::oidc::{ensure_activated, ensure_email_match, ensure_role_match};
use crate::crypto::{decrypt_totp_secret, encrypt_token, encrypt_totp_secret};
use crate::entities::{user, User};
use anyhow::Result;
use axum_login::{AuthUser, AuthnBackend, UserId};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::{env, fmt};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAuth {
    pub id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub totp_enabled: bool,
    pub mfa_verified: bool,
    pub active: bool,
    pub force_password_change: bool,
    pub role: String,
    pub(crate) session_hash: Vec<u8>,
}

impl AuthUser for UserAuth {
    type Id = Uuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        &self.session_hash
    }
}

/// Compute a BLAKE2b digest of security-relevant fields. Re-exported for
/// the routes/handlers module which builds new `UserAuth` instances after
/// mutations that affect session validity (password change, email change,
/// TOTP secret).
pub(crate) fn compute_session_hash(
    password_hash: &str,
    email: &str,
    totp_secret: Option<&str>,
) -> Vec<u8> {
    compute_hash_helper(password_hash, email, totp_secret)
}

#[derive(Clone)]
pub struct UserAuthBackend {
    db: DatabaseConnection,
    allowed_domain: String,
}

/// IdP-attested claims for an OIDC sign-in. Bundles the fields that
/// flow through every OIDC helper. `idp_tier` is resolved by
/// `oidc::resolve_idp_tier` from the role claim; it is used for
/// drift validation against the DB row.
pub struct OidcAuthClaims<'a> {
    pub sub: &'a str,
    pub email: &'a str,
    pub email_verified: bool,
    pub idp_tier: &'a str,
    pub display_name: Option<&'a str>,
}

impl UserAuthBackend {
    pub fn new(db: DatabaseConnection) -> Self {
        let allowed_domain =
            env::var("SITE_DOMAIN").expect("SITE_DOMAIN environment variable must be set");

        Self { db, allowed_domain }
    }

    /// Create a new local password-mode user with an explicit role. Returns
    /// the inserted row and the plaintext verification token to email to the
    /// user. The site-domain check applies to admin-tier accounts
    /// (administrator/poster); commenters can be from any domain since they
    /// are invite-onboarded.
    ///
    /// `activated` controls whether the row can immediately establish a
    /// session. Set true for the bootstrap admin (who owns the password they
    /// just typed at /api/auth/register) and for any direct test creation.
    /// Set false for admin-pre-provisioned rows that must go through the
    /// invite acceptance flow before they're usable.
    pub async fn create_user(
        &self,
        email: &str,
        password: &str,
        role: &str,
        activated: bool,
    ) -> Result<(user::Model, String)> {
        let is_admin_tier = role == user::ROLE_ADMINISTRATOR || role == user::ROLE_POSTER;
        if is_admin_tier && !email.ends_with(&format!("@{}", self.allowed_domain)) {
            anyhow::bail!("Email must be from {} domain", self.allowed_domain);
        }

        let existing = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await?;

        if existing.is_some() {
            anyhow::bail!("User with this email already exists");
        }

        let password_hash = hash_password(password)?;

        let verification_token = generate_verification_token();
        let encrypted_token = encrypt_token(&verification_token)?;
        let verification_expires = Utc::now() + chrono::Duration::hours(24);

        let handle = crate::profile::mint_unique_handle(&self.db, email).await?;

        let new_user = user::ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(email.to_string()),
            password_hash: Set(password_hash),
            email_verified: Set(false),
            verification_token: Set(Some(encrypted_token)),
            verification_token_expires_at: Set(Some(verification_expires.into())),
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
            role: Set(role.to_string()),
            oidc_sub: Set(None),
            display_name: Set(None),
            avatar_url: Set(None),
            last_login_at: Set(None),
            invite_code_id: Set(None),
            activated_at: Set(if activated {
                Some(Utc::now().into())
            } else {
                None
            }),
            handle: Set(handle),
            bio: Set(None),
            pronouns: Set(None),
            avatar_s3_key: Set(None),
            avatar_icon_data: Set(None),
            locale: Set(None),
        };

        let result = new_user.insert(&self.db).await?;

        Ok((result, verification_token))
    }

    /// Convenience wrapper around `create_user` for the bootstrap admin
    /// registration flow. Always creates an `administrator`.
    pub async fn create_admin(&self, email: &str, password: &str) -> Result<(user::Model, String)> {
        // Bootstrap admin owns the password they just typed; activate immediately.
        self.create_user(email, password, user::ROLE_ADMINISTRATOR, true)
            .await
    }

    pub async fn get_admin_by_id(&self, id: Uuid) -> Result<Option<user::Model>> {
        let admin = User::find_by_id(id).one(&self.db).await?;
        Ok(admin)
    }

    pub async fn update_totp(
        &self,
        user_id: Uuid,
        totp_secret: Option<String>,
        totp_enabled: bool,
    ) -> Result<user::Model> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        // Encrypt the TOTP secret before storing if provided
        let encrypted_secret = match totp_secret {
            Some(secret) => Some(encrypt_totp_secret(&secret)?),
            None => None,
        };

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.totp_secret = Set(encrypted_secret);
        admin_active.totp_enabled = Set(Some(totp_enabled));
        admin_active.totp_enabled_at = Set(if totp_enabled {
            Some(Utc::now().into())
        } else {
            None
        });
        // Clear lockout fields when disabling MFA to avoid stale state on re-enrollment
        if !totp_enabled {
            admin_active.mfa_failed_attempts = Set(Some(0));
            admin_active.mfa_locked_until = Set(None);
        }
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;
        Ok(updated)
    }

    /// Get the decrypted TOTP secret for a user
    pub async fn get_totp_secret(&self, user_id: Uuid) -> Result<Option<String>> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        match admin.totp_secret {
            Some(encrypted) => {
                let decrypted = decrypt_totp_secret(&encrypted)?;
                Ok(Some(decrypted))
            }
            None => Ok(None),
        }
    }

    /// Check if the user is currently locked out from MFA attempts
    pub async fn is_mfa_locked(&self, user_id: Uuid) -> Result<bool> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        if let Some(locked_until) = admin.mfa_locked_until {
            if locked_until.with_timezone(&Utc) > Utc::now() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Record a failed MFA attempt and lock account if threshold exceeded
    /// Returns (new_attempt_count, is_now_locked)
    pub async fn record_mfa_failure(&self, user_id: Uuid) -> Result<(i32, bool)> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let current_attempts = admin.mfa_failed_attempts.unwrap_or(0);
        let new_attempts = current_attempts + 1;

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.mfa_failed_attempts = Set(Some(new_attempts));

        // Lock account after 3 failed attempts for 24 hours
        const MAX_ATTEMPTS: i32 = 3;
        const LOCKOUT_HOURS: i64 = 24;

        let is_locked = new_attempts >= MAX_ATTEMPTS;
        if is_locked {
            let lockout_until = Utc::now() + chrono::Duration::hours(LOCKOUT_HOURS);
            admin_active.mfa_locked_until = Set(Some(lockout_until.into()));
        }

        admin_active.updated_at = Set(Utc::now().into());
        admin_active.update(&self.db).await?;

        Ok((new_attempts, is_locked))
    }

    /// Reset MFA failure count after successful verification
    pub async fn reset_mfa_failures(&self, user_id: Uuid) -> Result<()> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.mfa_failed_attempts = Set(Some(0));
        admin_active.mfa_locked_until = Set(None);
        admin_active.updated_at = Set(Utc::now().into());
        admin_active.update(&self.db).await?;

        Ok(())
    }

    /// Deactivate a user account
    /// Cannot deactivate self or the last active admin
    pub async fn deactivate_user(
        &self,
        user_id: Uuid,
        current_user_id: Uuid,
    ) -> Result<user::Model> {
        // Prevent self-deactivation
        if user_id == current_user_id {
            anyhow::bail!("Cannot deactivate your own account");
        }

        // Check if this is the last active admin
        let active_count = User::find()
            .filter(user::Column::Active.eq(true))
            .count(&self.db)
            .await?;

        if active_count <= 1 {
            anyhow::bail!("Cannot deactivate the last active administrator");
        }

        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        if !admin.active {
            anyhow::bail!("User is already deactivated");
        }

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.active = Set(false);
        admin_active.deactivated_at = Set(Some(Utc::now().into()));
        // Clear sensitive data on deactivation
        admin_active.totp_secret = Set(None);
        admin_active.totp_enabled = Set(Some(false));
        admin_active.totp_enabled_at = Set(None);
        admin_active.verification_token = Set(None);
        admin_active.verification_token_expires_at = Set(None);
        admin_active.password_reset_token = Set(None);
        admin_active.password_reset_token_expires_at = Set(None);
        admin_active.mfa_failed_attempts = Set(Some(0));
        admin_active.mfa_locked_until = Set(None);
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;
        Ok(updated)
    }

    /// Reactivate a user account
    /// Sets email_verified to false so user must re-verify
    pub async fn reactivate_user(&self, user_id: Uuid) -> Result<(user::Model, String)> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        if admin.active {
            anyhow::bail!("User is already active");
        }

        // Generate new verification token
        let verification_token = generate_verification_token();
        let encrypted_token = encrypt_token(&verification_token)?;
        let verification_expires = Utc::now() + chrono::Duration::hours(24);

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.active = Set(true);
        admin_active.deactivated_at = Set(None);
        admin_active.email_verified = Set(false);
        admin_active.verification_token = Set(Some(encrypted_token));
        admin_active.verification_token_expires_at = Set(Some(verification_expires.into()));
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;
        Ok((updated, verification_token))
    }

    /// Change a user's password
    /// If force_change is true, the user will be forced to change password on next login
    pub async fn change_password(
        &self,
        user_id: Uuid,
        new_password: &str,
        force_change: bool,
    ) -> Result<user::Model> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        let password_hash = hash_password(new_password)?;

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.password_hash = Set(password_hash);
        admin_active.force_password_change = Set(force_change);
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;
        Ok(updated)
    }

    /// Create a password reset token for a user
    /// Returns the plaintext token (to be sent via email) if successful
    /// Returns None if user not found (for enumeration protection)
    pub async fn create_password_reset_token(&self, email: &str) -> Result<Option<String>> {
        let admin = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await?;

        let admin = match admin {
            Some(a) => a,
            None => return Ok(None), // User not found, but don't reveal this
        };

        // Check cooldown: reject if token not yet expired
        if let Some(expires_at) = admin.password_reset_token_expires_at {
            if Utc::now() < expires_at.with_timezone(&Utc) {
                anyhow::bail!("Password reset already requested. Please wait 1 hour for the current request to expire.");
            }
        }

        // Generate token
        let reset_token = generate_verification_token();
        let encrypted_token = encrypt_token(&reset_token)?;
        let token_expires = Utc::now() + chrono::Duration::hours(1);

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.password_reset_token = Set(Some(encrypted_token));
        admin_active.password_reset_token_expires_at = Set(Some(token_expires.into()));
        admin_active.updated_at = Set(Utc::now().into());
        admin_active.update(&self.db).await?;

        Ok(Some(reset_token))
    }

    /// Validate a password reset token
    /// Returns the user if token is valid and not expired
    pub async fn validate_reset_token(&self, token: &str) -> Result<Option<user::Model>> {
        use crate::crypto::decrypt_token;

        // Find all users with non-null reset tokens
        let admins = User::find()
            .filter(user::Column::PasswordResetToken.is_not_null())
            .all(&self.db)
            .await?;

        for admin in admins {
            if let Some(ref encrypted_token) = admin.password_reset_token {
                // Try to decrypt and compare
                if let Ok(decrypted) = decrypt_token(encrypted_token) {
                    if decrypted == token {
                        // Check expiry
                        if let Some(expires_at) = admin.password_reset_token_expires_at {
                            if Utc::now() < expires_at.with_timezone(&Utc) {
                                return Ok(Some(admin));
                            }
                        }
                        // Token found but expired
                        return Ok(None);
                    }
                }
            }
        }

        Ok(None)
    }

    /// Complete password reset using a valid token. The token is kept until
    /// its original expiry so the cooldown enforced by
    /// `create_password_reset_token` continues to apply for the next request.
    ///
    /// Atomic consume: the candidate row is located via
    /// `validate_reset_token` (decrypt-and-match scan), then re-checked
    /// under a `SELECT ... FOR UPDATE` inside a transaction. A racer that
    /// presents the same plaintext token waits for our txn to commit and
    /// then finds `password_reset_token IS NULL`, so the second reset
    /// surfaces as "invalid or expired" instead of overwriting the first.
    pub async fn reset_password_with_token(
        &self,
        token: &str,
        new_password: &str,
    ) -> Result<user::Model> {
        let candidate = self
            .validate_reset_token(token)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invalid or expired password reset token"))?;

        let password_hash = hash_password(new_password)?;
        let now = Utc::now();

        let txn = self.db.begin().await?;
        let locked = User::find_by_id(candidate.id)
            .filter(user::Column::PasswordResetToken.is_not_null())
            .filter(user::Column::PasswordResetTokenExpiresAt.gt(now))
            .lock_exclusive()
            .one(&txn)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invalid or expired password reset token"))?;

        let mut active: user::ActiveModel = locked.into();
        active.password_hash = Set(password_hash);
        active.force_password_change = Set(false);
        active.password_reset_token = Set(None);
        active.updated_at = Set(now.into());

        let updated = active.update(&txn).await?;
        txn.commit().await?;
        Ok(updated)
    }

    /// Get a user by email (for password reset flow)
    pub async fn get_admin_by_email(&self, email: &str) -> Result<Option<user::Model>> {
        let admin = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await?;
        Ok(admin)
    }

    /// Generate a new verification token for a user and return the plaintext token
    /// Used for resending verification emails
    pub async fn regenerate_verification_token(
        &self,
        user_id: Uuid,
    ) -> Result<(user::Model, String)> {
        let admin = User::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;

        if !admin.active {
            anyhow::bail!("Cannot send verification email to deactivated user");
        }

        if admin.email_verified {
            anyhow::bail!("User email is already verified");
        }

        // Generate new verification token
        let verification_token = generate_verification_token();
        let encrypted_token = encrypt_token(&verification_token)?;
        let verification_expires = Utc::now() + chrono::Duration::hours(24);

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.verification_token = Set(Some(encrypted_token));
        admin_active.verification_token_expires_at = Set(Some(verification_expires.into()));
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;
        Ok((updated, verification_token))
    }

    pub async fn verify_email(&self, token: &str) -> Result<user::Model> {
        use crate::crypto::decrypt_token;

        // Tokens are stored encrypted, so we must decrypt and compare
        let admins = User::find()
            .filter(user::Column::VerificationToken.is_not_null())
            .all(&self.db)
            .await?;

        let admin = admins
            .into_iter()
            .find(|a| {
                a.verification_token
                    .as_deref()
                    .and_then(|encrypted| decrypt_token(encrypted).ok())
                    .is_some_and(|decrypted| decrypted == token)
            })
            .ok_or_else(|| anyhow::anyhow!("Invalid verification token"))?;

        // Check if token is expired
        if let Some(expires_at) = admin.verification_token_expires_at {
            if Utc::now() > expires_at.with_timezone(&Utc) {
                anyhow::bail!("Verification token has expired");
            }
        } else {
            anyhow::bail!("No verification token expiration set");
        }

        // Mark as verified
        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.email_verified = Set(true);
        admin_active.verification_token = Set(None);
        admin_active.verification_token_expires_at = Set(None);
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;

        Ok(updated)
    }
}

impl UserAuthBackend {
    /// Local password authentication. Looks up the user by email, runs argon2
    /// verification, and enforces the OIDC-linkage hard auth-mode boundary:
    /// if `oidc_sub IS NOT NULL`, the password path is closed regardless of
    /// what hash they provide. The dummy-hash branch keeps timing parity with
    /// the non-existent-user case.
    pub async fn authenticate_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<Option<UserAuth>, AuthError> {
        let user_row = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await
            .map_err(AuthError::from)?;

        // Decide which hash to verify against. OIDC-linked users get the dummy
        // hash so verify_password runs (timing parity) but fails identically to
        // a non-existent user.
        let (hash_to_check, password_path_open) = match &user_row {
            Some(u) if u.oidc_sub.is_some() => (DUMMY_HASH.as_str(), false),
            Some(u) => (u.password_hash.as_str(), true),
            None => (DUMMY_HASH.as_str(), false),
        };

        let valid = verify_password(password, hash_to_check).map_err(AuthError::from)?;

        if !password_path_open || !valid {
            return Ok(None);
        }

        // Safe to unwrap: password_path_open is only true when user_row is Some.
        let user_row = user_row.unwrap();

        if !user_row.active {
            return Err(AuthError(anyhow::anyhow!("Account has been deactivated")));
        }

        // Inert rows (admin/poster pre-provisioned but not yet invite-bound)
        // cannot establish a session even if a password somehow validates.
        ensure_activated(&user_row).map_err(AuthError::from)?;

        if !user_row.email_verified {
            return Err(AuthError(anyhow::anyhow!(
                "Email not verified. Please check your email for verification link."
            )));
        }

        Ok(Some(self.user_auth_from_model(
            user_row, /* oidc_login */ false,
        )))
    }

    /// OIDC authentication. Three flows, dispatched by what's in the DB:
    /// - **B  normal login:** existing `oidc_sub` match. Fails closed on
    ///   role/email drift; refreshes `last_login_at`, `display_name`,
    ///   `email_verified` only.
    /// - **A.1  invite-bind existing row:** invite's `email_hint` must
    ///   match the IdP email. Stamps `oidc_sub`, rotates the placeholder
    ///   `password_hash`, sets `activated_at`.
    /// - **A.2  mint new commenter:** privileged tiers rejected
    ///   (must be pre-provisioned via A.1).
    pub async fn authenticate_oidc(
        &self,
        claims: OidcAuthClaims<'_>,
        invite_code: Option<&str>,
    ) -> Result<Option<UserAuth>, AuthError> {
        // Flow B: existing oidc_sub match → normal login.
        if let Some(row) = User::find()
            .filter(user::Column::OidcSub.eq(claims.sub))
            .one(&self.db)
            .await
            .map_err(AuthError::from)?
        {
            let user_row = self
                .oidc_normal_login(row, &claims)
                .await
                .map_err(AuthError::from)?;
            return Ok(Some(self.user_auth_from_model(user_row, true)));
        }

        // Flow A: invite required for any new bind or new row.
        let code = invite_code.ok_or_else(|| {
            AuthError(anyhow::anyhow!(
                "This site is invite-only; please use a valid invite link to sign in."
            ))
        })?;
        let invite_row = crate::invites::validate_invite_code(&self.db, code)
            .await
            .map_err(|e| AuthError(anyhow::anyhow!(e.to_string())))?
            .ok_or_else(|| {
                AuthError(anyhow::anyhow!(
                    "Invite is invalid, expired, revoked, or already used"
                ))
            })?;

        // Flow A.1: pre-provisioned row reachable via invite_code_id (the
        // admin stamped the invite onto the user row at creation time).
        // Looking up by invite_code_id rather than email guarantees only
        // *this specific* invite can bind *this specific* row, no other
        // invite, even one with a matching email_hint, can hijack it.
        if let Some(row) = User::find()
            .filter(user::Column::InviteCodeId.eq(invite_row.id))
            .one(&self.db)
            .await
            .map_err(AuthError::from)?
        {
            let user_row = self
                .oidc_bind_existing(row, &claims, &invite_row)
                .await
                .map_err(AuthError::from)?;
            return Ok(Some(self.user_auth_from_model(user_row, true)));
        }

        // Flow A.2: no row links to this invite. Two sub-cases:
        //  - Commenter invite (email_hint is None): mint a new commenter row.
        //  - Pre-provisioned-row invite (email_hint is Some): the row was
        //    deleted somehow. Don't downgrade silently to commenter.
        if invite_row.email_hint.is_some() {
            return Err(AuthError(anyhow::anyhow!(
                "Invite is for an account that no longer exists"
            )));
        }
        let user_row = self
            .oidc_create_commenter(&claims, &invite_row)
            .await
            .map_err(AuthError::from)?;
        Ok(Some(self.user_auth_from_model(user_row, true)))
    }

    // ==================== Flow B: normal login ====================

    async fn oidc_normal_login(
        &self,
        row: user::Model,
        claims: &OidcAuthClaims<'_>,
    ) -> Result<user::Model> {
        ensure_activated(&row)?;
        ensure_role_match(claims.idp_tier, &row.role)?;
        ensure_email_match(claims.email, &row.email)?;
        if !row.active {
            anyhow::bail!("Account has been deactivated");
        }

        let mut active: user::ActiveModel = row.into();
        active.email_verified = Set(claims.email_verified);
        if let Some(name) = claims.display_name {
            active.display_name = Set(Some(name.to_string()));
        }
        active.last_login_at = Set(Some(Utc::now().into()));
        // updated_at is auto-managed by ActiveModelBehavior::before_save.
        Ok(active.update(&self.db).await?)
    }

    // ==================== Flow A.1: bind pre-provisioned row ====================

    async fn oidc_bind_existing(
        &self,
        row: user::Model,
        claims: &OidcAuthClaims<'_>,
        invite: &crate::entities::invite_code::Model,
    ) -> Result<user::Model> {
        if row.oidc_sub.is_some() {
            anyhow::bail!("Account already linked to a different SSO identity");
        }
        if !row.active {
            anyhow::bail!("Account has been deactivated");
        }
        ensure_role_match(claims.idp_tier, &row.role)?;

        // email_hint enforcement: the invite was created with intent to bind a
        // specific email. Without a matching hint, this invite cannot bind to
        // an existing row, it would let a stolen commenter invite hijack a
        // pre-provisioned admin/poster account whose email happens to match.
        let hint = invite
            .email_hint
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Invite is not for this account"))?;
        if hint != claims.email {
            anyhow::bail!("Invite is not for this account");
        }

        let row_id = row.id;
        let mut active: user::ActiveModel = row.into();
        active.oidc_sub = Set(Some(claims.sub.to_string()));
        // Rotate password_hash to a fresh non-usable value so any placeholder
        // hash assigned at user-creation time is closed.
        active.password_hash = Set(format!("oidc_user_{}", Uuid::new_v4()));
        active.email_verified = Set(claims.email_verified);
        if let Some(name) = claims.display_name {
            active.display_name = Set(Some(name.to_string()));
        }
        active.last_login_at = Set(Some(Utc::now().into()));
        // invite_code_id is already set on the row (admin stamped it at user
        // creation time). updated_at is auto-managed by ActiveModelBehavior.
        active.activated_at = Set(Some(Utc::now().into()));
        let updated = active.update(&self.db).await?;

        if let Err(e) = crate::invites::mark_used(&self.db, invite.id, row_id).await {
            tracing::warn!(
                "Failed to mark invite {} used by bound user {}: {}",
                invite.id,
                row_id,
                e
            );
        }

        tracing::info!(
            "OIDC bind for pre-provisioned user: {} role={}",
            updated.email,
            updated.role
        );
        Ok(updated)
    }

    // ==================== Flow C: password-mode invite acceptance ====================

    /// Flow C.1: bind a pre-provisioned admin/poster row by setting the
    /// user-chosen password and activating the row. The row is reached via
    /// the invite's `id` (the admin pre-stamped this on the user row at
    /// creation time), and the form-submitted email must match both the
    /// invite's email_hint and the row's email.
    pub async fn accept_invite_password_bind(
        &self,
        invite: &crate::entities::invite_code::Model,
        email: &str,
        new_password: &str,
    ) -> Result<user::Model> {
        let row = User::find()
            .filter(user::Column::InviteCodeId.eq(invite.id))
            .one(&self.db)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "This invite code is no longer valid. A newer invite may have been issued."
                )
            })?;

        if row.oidc_sub.is_some() {
            anyhow::bail!("Account is linked to SSO and cannot be activated with a password");
        }
        if row.activated_at.is_some() {
            anyhow::bail!("Account is already activated. Please sign in instead.");
        }
        if !row.active {
            anyhow::bail!("Account has been deactivated");
        }

        let hint = invite
            .email_hint
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Invite is not for this account"))?;
        if hint != email || hint != row.email {
            anyhow::bail!("Invite is not for this account");
        }

        let row_id = row.id;
        let password_hash = hash_password(new_password)?;
        let mut active: user::ActiveModel = row.into();
        active.password_hash = Set(password_hash);
        active.email_verified = Set(true); // invite is delivered to a known address
        active.force_password_change = Set(false);
        active.last_login_at = Set(Some(Utc::now().into()));
        // invite_code_id is already set; updated_at is auto-managed.
        active.activated_at = Set(Some(Utc::now().into()));
        let updated = active.update(&self.db).await?;

        if let Err(e) = crate::invites::mark_used(&self.db, invite.id, row_id).await {
            tracing::warn!(
                "Failed to mark invite {} used by password-bound user {}: {}",
                invite.id,
                row_id,
                e
            );
        }

        tracing::info!(
            "Password-mode invite bind for pre-provisioned user: {} role={}",
            updated.email,
            updated.role
        );
        Ok(updated)
    }

    /// Flow C.2: mint a brand-new commenter row from a password-mode invite
    /// acceptance form. The form's email becomes the row's email; no row
    /// pre-existed because the invite was issued without a specific row in mind
    /// (or with email_hint pointing at the invitee).
    pub async fn accept_invite_password_create_commenter(
        &self,
        invite: &crate::entities::invite_code::Model,
        email: &str,
        new_password: &str,
    ) -> Result<user::Model> {
        let existing = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await?;
        if existing.is_some() {
            anyhow::bail!("An account with this email already exists. Please sign in instead.");
        }

        let new_user_id = Uuid::new_v4();
        let password_hash = hash_password(new_password)?;
        let handle = crate::profile::mint_unique_handle(&self.db, email).await?;
        let new_user = user::ActiveModel {
            id: Set(new_user_id),
            email: Set(email.to_string()),
            password_hash: Set(password_hash),
            email_verified: Set(true),
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
            role: Set(user::ROLE_COMMENTER.to_string()),
            oidc_sub: Set(None),
            display_name: Set(None),
            avatar_url: Set(None),
            last_login_at: Set(Some(Utc::now().into())),
            invite_code_id: Set(Some(invite.id)),
            activated_at: Set(Some(Utc::now().into())),
            handle: Set(handle),
            bio: Set(None),
            pronouns: Set(None),
            avatar_s3_key: Set(None),
            avatar_icon_data: Set(None),
            locale: Set(None),
        };
        let result = new_user.insert(&self.db).await?;

        if let Err(e) = crate::invites::mark_used(&self.db, invite.id, new_user_id).await {
            tracing::warn!(
                "Failed to mark invite {} used by new password commenter {}: {}",
                invite.id,
                new_user_id,
                e
            );
        }

        tracing::info!(
            "Created new password-mode commenter via invite: {}",
            new_user_id
        );
        Ok(result)
    }

    /// Build a `UserAuth` session principal for a password-mode user that has
    /// just completed invite acceptance. Exposed for the invite-accept handler
    /// in `admin/routes.rs` so it can establish a session without going back
    /// through the full password authentication path.
    pub fn user_auth_for_invite_accept(&self, model: user::Model) -> UserAuth {
        // Password-mode invite-accept is fresh authentication: MFA is not
        // configured yet (totp_enabled=false), so user_auth_from_model returns
        // mfa_verified=true automatically.
        self.user_auth_from_model(model, /* oidc_login */ false)
    }

    // ==================== Flow A.2: mint new commenter ====================

    async fn oidc_create_commenter(
        &self,
        claims: &OidcAuthClaims<'_>,
        invite: &crate::entities::invite_code::Model,
    ) -> Result<user::Model> {
        // Privileged accounts must be pre-provisioned by an admin (Flow A.1).
        // A first-time visitor with an admin or poster claim from the IdP is
        // either a misconfiguration or an attempted privilege escalation.
        if claims.idp_tier != user::ROLE_COMMENTER {
            anyhow::bail!(
                "Privileged accounts must be pre-provisioned by an administrator before signing in."
            );
        }

        let new_user_id = Uuid::new_v4();
        let random_hash = format!("oidc_user_{}", Uuid::new_v4());
        let handle = crate::profile::mint_unique_handle(&self.db, claims.email).await?;
        let new_user = user::ActiveModel {
            id: Set(new_user_id),
            email: Set(claims.email.to_string()),
            password_hash: Set(random_hash),
            email_verified: Set(claims.email_verified),
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
            role: Set(user::ROLE_COMMENTER.to_string()),
            oidc_sub: Set(Some(claims.sub.to_string())),
            display_name: Set(claims.display_name.map(|s| s.to_string())),
            avatar_url: Set(None),
            last_login_at: Set(Some(Utc::now().into())),
            invite_code_id: Set(Some(invite.id)),
            activated_at: Set(Some(Utc::now().into())),
            handle: Set(handle),
            bio: Set(None),
            pronouns: Set(None),
            avatar_s3_key: Set(None),
            avatar_icon_data: Set(None),
            locale: Set(None),
        };
        let result = new_user.insert(&self.db).await?;

        if let Err(e) = crate::invites::mark_used(&self.db, invite.id, new_user_id).await {
            tracing::warn!(
                "Failed to mark invite {} used by new commenter {}: {}",
                invite.id,
                new_user_id,
                e
            );
        }

        tracing::info!("Created new OIDC commenter via invite: {}", new_user_id);
        Ok(result)
    }

    /// Build a `UserAuth` session principal from a user row. `oidc_login=true`
    /// marks the principal as MFA-verified up front because Keycloak owns MFA
    /// at the IdP for SSO logins.
    fn user_auth_from_model(&self, model: user::Model, oidc_login: bool) -> UserAuth {
        let totp_enabled = model.totp_enabled.unwrap_or(false);
        let session_hash = compute_session_hash(
            &model.password_hash,
            &model.email,
            model.totp_secret.as_deref(),
        );
        UserAuth {
            id: model.id,
            email: model.email,
            email_verified: model.email_verified,
            totp_enabled,
            // OIDC logins are MFA-verified at the IdP; password logins start
            // unverified if local TOTP is enabled.
            mfa_verified: oidc_login || !totp_enabled,
            active: model.active,
            force_password_change: model.force_password_change,
            role: model.role,
            session_hash,
        }
    }
}

#[derive(Debug)]
pub struct AuthError(anyhow::Error);

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AuthError {}

impl From<anyhow::Error> for AuthError {
    fn from(err: anyhow::Error) -> Self {
        AuthError(err)
    }
}

impl From<sea_orm::DbErr> for AuthError {
    fn from(err: sea_orm::DbErr) -> Self {
        AuthError(err.into())
    }
}

impl AuthnBackend for UserAuthBackend {
    type User = UserAuth;
    type Credentials = Credentials;
    type Error = AuthError;

    fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> impl std::future::Future<Output = Result<Option<Self::User>, Self::Error>> + Send {
        let backend = self.clone();
        async move {
            let result = match creds {
                Credentials::Password { email, password } => {
                    backend.authenticate_password(&email, &password).await
                }
                Credentials::Oidc {
                    sub,
                    email,
                    email_verified,
                    idp_tier,
                    display_name,
                    invite_code,
                } => {
                    let claims = OidcAuthClaims {
                        sub: &sub,
                        email: &email,
                        email_verified,
                        idp_tier: &idp_tier,
                        display_name: display_name.as_deref(),
                    };
                    backend
                        .authenticate_oidc(claims, invite_code.as_deref())
                        .await
                }
            };
            // Count successful logins by tier so /metrics can show
            // commenter vs poster vs admin login volume.
            if let Ok(Some(ref user)) = result {
                crate::metrics::LOGINS_TOTAL
                    .with_label_values(&[user.role.as_str()])
                    .inc();
            }
            result
        }
    }

    fn get_user(
        &self,
        user_id: &UserId<Self>,
    ) -> impl std::future::Future<Output = Result<Option<Self::User>, Self::Error>> + Send {
        let user_id = *user_id;
        let backend = self.clone();
        async move {
            let model = User::find_by_id(user_id)
                .one(&backend.db)
                .await
                .map_err(AuthError::from)?;
            // mfa_verified is always re-derived from totp_enabled here. The
            // tower-sessions MFA_VERIFIED_KEY (set after successful TOTP entry
            // or after OIDC login) is what the auth middleware actually gates
            // on, so this principal field is just a hint.
            Ok(model.map(|m| backend.user_auth_from_model(m, /* oidc_login */ false)))
        }
    }
}

/// Authentication credentials for the unified `UserAuthBackend`.
///
/// `Password` is the local email+password path. `Oidc` is constructed by the
/// OIDC callback after a successful Keycloak exchange and carries every field
/// needed to upsert the user row.
#[derive(Debug, Clone)]
pub enum Credentials {
    Password {
        email: String,
        password: String,
    },
    /// OIDC-issued credential. `idp_tier` is the IdP's effective tier resolved
    /// by `oidc::resolve_idp_tier`, used by the backend to validate against
    /// the DB row's role. The IdP's claim is never used to *assign* role:
    /// brand-new rows are always commenters (admins/posters must be pre-
    /// provisioned), and existing rows keep whatever role is in the DB. The
    /// tier is here so the backend can fail closed on drift.
    Oidc {
        sub: String,
        email: String,
        email_verified: bool,
        idp_tier: String,
        display_name: Option<String>,
        invite_code: Option<String>,
    },
}
