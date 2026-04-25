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
use crate::crypto::{decrypt_totp_secret, encrypt_token, encrypt_totp_secret};
use crate::entities::{user, User};
use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use rand::RngExt;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::{env, fmt};
use uuid::Uuid;

// Precomputed dummy hash for timing attack mitigation
// This ensures password verification takes the same time regardless of whether the user exists
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(b"dummy_password_for_timing_attack_mitigation", &salt)
        .expect("Failed to generate dummy hash")
        .to_string()
});

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
    pub user_type: String,
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

/// Compute a BLAKE2b digest of security-relevant fields.
/// Changes to password, email, or TOTP secret will invalidate existing sessions.
/// This is roughly equal or better than SHA256 and faster, but more importantly,
/// already pulled in by argon2 which is the primary password hashing method
pub(crate) fn compute_session_hash(password_hash: &str, email: &str, totp_secret: Option<&str>) -> Vec<u8> {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(password_hash.as_bytes());
    hasher.update(email.as_bytes());
    hasher.update(totp_secret.unwrap_or("").as_bytes());
    hasher.finalize().to_vec()
}

#[derive(Clone)]
pub struct UserAuthBackend {
    db: DatabaseConnection,
    allowed_domain: String,
}

impl UserAuthBackend {
    pub fn new(db: DatabaseConnection) -> Self {
        let allowed_domain =
            env::var("SITE_DOMAIN").expect("SITE_DOMAIN environment variable must be set");

        Self { db, allowed_domain }
    }

    pub async fn create_admin(
        &self,
        email: &str,
        password: &str,
    ) -> Result<(user::Model, String)> {
        // Validate email domain
        if !email.ends_with(&format!("@{}", self.allowed_domain)) {
            anyhow::bail!("Email must be from {} domain", self.allowed_domain);
        }

        // Check if user already exists
        let existing = User::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.db)
            .await?;

        if existing.is_some() {
            anyhow::bail!("Admin user with this email already exists");
        }

        // Hash password
        let password_hash = hash_password(password)?;

        // Generate verification token
        let verification_token = generate_verification_token();
        let encrypted_token = encrypt_token(&verification_token)?;
        let verification_expires = Utc::now() + chrono::Duration::hours(24);

        let admin = user::ActiveModel {
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
            role: Set(user::ROLE_ADMINISTRATOR.to_string()),
            user_type: Set(user::USER_TYPE_ADMIN.to_string()),
            oidc_sub: Set(None),
            display_name: Set(None),
            avatar_url: Set(None),
            last_login_at: Set(None),
            invite_code_id: Set(None),
        };

        let result = admin.insert(&self.db).await?;

        Ok((result, verification_token))
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

    /// Complete password reset using a valid token
    /// Note: Token is kept until expiry to prevent new reset requests during cooldown
    pub async fn reset_password_with_token(
        &self,
        token: &str,
        new_password: &str,
    ) -> Result<user::Model> {
        let admin = self
            .validate_reset_token(token)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Invalid or expired password reset token"))?;

        let password_hash = hash_password(new_password)?;

        let mut admin_active: user::ActiveModel = admin.into();
        admin_active.password_hash = Set(password_hash);
        admin_active.force_password_change = Set(false);
        // Clear token after use (single-use). Cooldown still works via
        // password_reset_token_expires_at which remains set.
        admin_active.password_reset_token = Set(None);
        admin_active.updated_at = Set(Utc::now().into());

        let updated = admin_active.update(&self.db).await?;
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

        if !user_row.email_verified {
            return Err(AuthError(anyhow::anyhow!(
                "Email not verified. Please check your email for verification link."
            )));
        }

        Ok(Some(self.user_auth_from_model(user_row, /* oidc_login */ false)))
    }

    /// OIDC authentication. Upserts the user row from the verified OIDC claims,
    /// applies the role + user_type the caller resolved from the claims, and
    /// returns a `UserAuth` flagged as already MFA-verified (Keycloak handles
    /// MFA at the IdP).
    #[allow(clippy::too_many_arguments)]
    pub async fn authenticate_oidc(
        &self,
        sub: &str,
        email: &str,
        email_verified: bool,
        role: &str,
        user_type: &str,
        display_name: Option<&str>,
        invite_code: Option<&str>,
    ) -> Result<Option<UserAuth>, AuthError> {
        let user_row = self
            .upsert_oidc_user(
                sub,
                email,
                email_verified,
                role,
                user_type,
                display_name,
                invite_code,
            )
            .await
            .map_err(AuthError::from)?;

        if !user_row.active {
            return Err(AuthError(anyhow::anyhow!("Account has been deactivated")));
        }

        Ok(Some(self.user_auth_from_model(user_row, /* oidc_login */ true)))
    }

    /// Find an existing user by `oidc_sub` (or by email as a fallback for
    /// pre-OIDC accounts being upgraded to SSO) and update it with the OIDC
    /// claims; otherwise create a new user row. Always sets `oidc_sub`, which
    /// closes the password path for this user from this point on.
    #[allow(clippy::too_many_arguments)]
    async fn upsert_oidc_user(
        &self,
        sub: &str,
        email: &str,
        email_verified: bool,
        role: &str,
        user_type: &str,
        display_name: Option<&str>,
        _invite_code: Option<&str>, // Phase 2 wires this through.
    ) -> Result<user::Model> {
        // Prefer match by oidc_sub. Fall back to email for first-time SSO
        // upgrade of a previously password-only admin/poster account.
        let by_sub = User::find()
            .filter(user::Column::OidcSub.eq(sub))
            .one(&self.db)
            .await?;
        let existing = if by_sub.is_some() {
            by_sub
        } else {
            User::find()
                .filter(user::Column::Email.eq(email))
                .one(&self.db)
                .await?
        };

        if let Some(existing_row) = existing {
            let mut active: user::ActiveModel = existing_row.into();
            // Lock the row to OIDC. This is the moment the password path closes.
            active.oidc_sub = Set(Some(sub.to_string()));
            // Defense-in-depth: rotate password_hash to a fresh non-usable value
            // on every OIDC login so leaks of an older hash can't be replayed.
            active.password_hash = Set(format!("oidc_user_{}", Uuid::new_v4()));
            active.email = Set(email.to_string());
            active.email_verified = Set(email_verified);
            active.role = Set(role.to_string());
            active.user_type = Set(user_type.to_string());
            if let Some(name) = display_name {
                active.display_name = Set(Some(name.to_string()));
            }
            active.last_login_at = Set(Some(Utc::now().into()));
            active.updated_at = Set(Utc::now().into());
            return Ok(active.update(&self.db).await?);
        }

        let random_hash = format!("oidc_user_{}", Uuid::new_v4());
        let new_user = user::ActiveModel {
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
            role: Set(role.to_string()),
            user_type: Set(user_type.to_string()),
            oidc_sub: Set(Some(sub.to_string())),
            display_name: Set(display_name.map(|s| s.to_string())),
            avatar_url: Set(None),
            last_login_at: Set(Some(Utc::now().into())),
            invite_code_id: Set(None),
        };
        let result = new_user.insert(&self.db).await?;
        tracing::info!(
            "Created new OIDC user: {} role={} user_type={}",
            email,
            role,
            user_type
        );
        Ok(result)
    }

    /// Build a `UserAuth` session principal from a user row. `oidc_login=true`
    /// marks the principal as MFA-verified up front because Keycloak owns MFA
    /// at the IdP for SSO logins.
    fn user_auth_from_model(&self, model: user::Model, oidc_login: bool) -> UserAuth {
        let totp_enabled = model.totp_enabled.unwrap_or(false);
        let session_hash =
            compute_session_hash(&model.password_hash, &model.email, model.totp_secret.as_deref());
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
            user_type: model.user_type,
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
            match creds {
                Credentials::Password { email, password } => {
                    backend.authenticate_password(&email, &password).await
                }
                Credentials::Oidc {
                    sub,
                    email,
                    email_verified,
                    role,
                    user_type,
                    display_name,
                    invite_code,
                } => {
                    backend
                        .authenticate_oidc(
                            &sub,
                            &email,
                            email_verified,
                            &role,
                            &user_type,
                            display_name.as_deref(),
                            invite_code.as_deref(),
                        )
                        .await
                }
            }
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
    Oidc {
        sub: String,
        email: String,
        email_verified: bool,
        role: String,
        user_type: String,
        display_name: Option<String>,
        invite_code: Option<String>,
    },
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?
        .to_string();
    Ok(password_hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(password_hash).map_err(|e| anyhow::anyhow!("Invalid hash: {}", e))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

fn generate_verification_token() -> String {
    let token_bytes: [u8; 32] = rand::rng().random();
    hex::encode(token_bytes)
}
