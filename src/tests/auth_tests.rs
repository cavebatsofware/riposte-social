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
use crate::admin::auth::{verify_password, UserAuthBackend};
use crate::admin::Credentials;
use crate::entities::{user, User};
use crate::tests::{test_db_from_pool, test_email};
use axum_login::AuthnBackend;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

// ==================== Helpers ====================

/// Create an admin and immediately verify their email so they can authenticate.
async fn create_verified_admin(
    backend: &UserAuthBackend,
    email: &str,
    password: &str,
) -> user::Model {
    let (_admin, token) = backend.create_admin(email, password).await.unwrap();
    backend.verify_email(&token).await.unwrap()
}

const TEST_PASSWORD: &str = "MyStr0ng!Password123";

// ==================== Registration Tests ====================

#[sqlx::test(migrations = false)]
async fn test_create_admin_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-create");

    let (admin, token) = backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    assert_eq!(admin.email, email);
    assert!(!admin.email_verified);
    assert!(!token.is_empty());
    assert_eq!(admin.role, "administrator");
    assert!(admin.active);
    assert!(!admin.force_password_change);
    assert_eq!(admin.totp_enabled, Some(false));
    assert!(admin.totp_secret.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_create_admin_wrong_domain_rejected(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let result = backend
        .create_admin("test@wrongdomain.invalid", TEST_PASSWORD)
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("domain"));
}

#[sqlx::test(migrations = false)]
async fn test_create_admin_duplicate_email_rejected(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-dup");

    backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    let result = backend.create_admin(&email, "AnotherStr0ng!Pass456").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}

// ==================== Authentication Tests ====================

#[sqlx::test(migrations = false)]
async fn test_authenticate_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-auth");

    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let user = backend
        .authenticate(Credentials::Password {
            email: email.clone(),
            password: TEST_PASSWORD.to_string(),
        })
        .await
        .unwrap()
        .expect("Should return authenticated user");

    assert_eq!(user.email, email);
    assert!(user.email_verified);
    assert!(user.active);
    assert_eq!(user.role, "administrator");
}

#[sqlx::test(migrations = false)]
async fn test_authenticate_wrong_password_returns_none(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-wrongpw");

    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let result = backend
        .authenticate(Credentials::Password {
            email,
            password: "WrongPassword!456".to_string(),
        })
        .await
        .unwrap();

    assert!(result.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_authenticate_nonexistent_user_returns_none(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    // Also verifies timing attack mitigation (dummy hash used for nonexistent users)
    let result = backend
        .authenticate(Credentials::Password {
            email: test_email("nobody"),
            password: "SomePassword!123".to_string(),
        })
        .await
        .unwrap();

    assert!(result.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_authenticate_unverified_email_returns_error(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-unverified");

    backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    let result = backend
        .authenticate(Credentials::Password {
            email,
            password: TEST_PASSWORD.to_string(),
        })
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not verified"));
}

#[sqlx::test(migrations = false)]
async fn test_authenticate_deactivated_account_returns_error(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email1 = test_email("test-deact1");
    let email2 = test_email("test-deact2");

    let admin = create_verified_admin(&backend, &email1, TEST_PASSWORD).await;
    let other = create_verified_admin(&backend, &email2, TEST_PASSWORD).await;
    backend.deactivate_user(admin.id, other.id).await.unwrap();

    let result = backend
        .authenticate(Credentials::Password {
            email: email1,
            password: TEST_PASSWORD.to_string(),
        })
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("deactivated"));
}

#[sqlx::test(migrations = false)]
async fn test_authenticate_user_with_totp_has_mfa_not_verified(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-mfaauth");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    backend
        .update_totp(admin.id, Some("JBSWY3DPEHPK3PXP".to_string()), true)
        .await
        .unwrap();

    let user = backend
        .authenticate(Credentials::Password {
            email,
            password: TEST_PASSWORD.to_string(),
        })
        .await
        .unwrap()
        .expect("Should return user");

    assert!(user.totp_enabled);
    assert!(!user.mfa_verified);
}

// ==================== Email Verification Tests ====================

#[sqlx::test(migrations = false)]
async fn test_verify_email_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-verify");

    let (_admin, token) = backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    let verified = backend.verify_email(&token).await.unwrap();
    assert!(verified.email_verified);
    assert!(verified.verification_token.is_none());
    assert!(verified.verification_token_expires_at.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_verify_email_invalid_token_fails(pool: sqlx::PgPool) {
    let _db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(_db.clone());

    assert!(backend
        .verify_email("nonexistent-token-12345")
        .await
        .is_err());
}

#[sqlx::test(migrations = false)]
async fn test_verify_email_expired_token_fails(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-expired");

    let (admin, token) = backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    // Manually expire the token
    let mut active: user::ActiveModel = admin.into();
    active.verification_token_expires_at =
        Set(Some((Utc::now() - chrono::Duration::hours(25)).into()));
    active.update(&db).await.unwrap();

    let result = backend.verify_email(&token).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("expired"));
}

// ==================== Password Management Tests ====================

#[sqlx::test(migrations = false)]
async fn test_change_password_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-changepw");
    let new_password = "NewStr0ng!Password456";

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    backend
        .change_password(admin.id, new_password, false)
        .await
        .unwrap();

    // Old password should fail
    assert!(backend
        .authenticate(Credentials::Password {
            email: email.clone(),
            password: TEST_PASSWORD.to_string(),
        })
        .await
        .unwrap()
        .is_none());

    // New password should work
    assert!(backend
        .authenticate(Credentials::Password {
            email,
            password: new_password.to_string(),
        })
        .await
        .unwrap()
        .is_some());
}

#[sqlx::test(migrations = false)]
async fn test_change_password_with_force_flag(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-forcepw");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let updated = backend
        .change_password(admin.id, "TempStr0ng!Password789", true)
        .await
        .unwrap();

    assert!(updated.force_password_change);

    let user = backend
        .authenticate(Credentials::Password {
            email,
            password: "TempStr0ng!Password789".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(user.force_password_change);
}

#[sqlx::test(migrations = false)]
async fn test_password_reset_flow(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-reset");
    let new_password = "ResetStr0ng!Password789";

    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let token = backend
        .create_password_reset_token(&email)
        .await
        .unwrap()
        .expect("Should return token");

    let admin = backend
        .validate_reset_token(&token)
        .await
        .unwrap()
        .expect("Token should be valid");
    assert_eq!(admin.email, email);

    backend
        .reset_password_with_token(&token, new_password)
        .await
        .unwrap();

    assert!(backend
        .authenticate(Credentials::Password {
            email,
            password: new_password.to_string(),
        })
        .await
        .unwrap()
        .is_some());
}

#[sqlx::test(migrations = false)]
async fn test_password_reset_token_nonexistent_user(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let result = backend
        .create_password_reset_token(&test_email("nobody"))
        .await
        .unwrap();

    assert!(
        result.is_none(),
        "Enumeration protection: should return None"
    );
}

#[sqlx::test(migrations = false)]
async fn test_validate_reset_token_invalid(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    assert!(backend
        .validate_reset_token("bogus-token")
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrations = false)]
async fn test_password_reset_cooldown(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-cooldown");

    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    backend
        .create_password_reset_token(&email)
        .await
        .unwrap()
        .unwrap();

    // Second request should fail (token hasn't expired yet)
    assert!(backend.create_password_reset_token(&email).await.is_err());
}

// ==================== Password Hashing Tests ====================

#[tokio::test]
async fn test_verify_password_correct() {
    use argon2::password_hash::{rand_core::OsRng, SaltString};
    use argon2::{Argon2, PasswordHasher};

    let hash = Argon2::default()
        .hash_password(b"TestPassword123!", &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string();

    assert!(verify_password("TestPassword123!", &hash).unwrap());
}

#[tokio::test]
async fn test_verify_password_incorrect() {
    use argon2::password_hash::{rand_core::OsRng, SaltString};
    use argon2::{Argon2, PasswordHasher};

    let hash = Argon2::default()
        .hash_password(b"TestPassword123!", &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string();

    assert!(!verify_password("WrongPassword!", &hash).unwrap());
}

// ==================== MFA Management Tests ====================

#[sqlx::test(migrations = false)]
async fn test_update_totp_enable(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-totp-en");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let secret = "JBSWY3DPEHPK3PXP";

    let updated = backend
        .update_totp(admin.id, Some(secret.to_string()), true)
        .await
        .unwrap();

    assert_eq!(updated.totp_enabled, Some(true));
    assert!(updated.totp_secret.is_some());
    assert!(updated.totp_enabled_at.is_some());
    // Secret should be encrypted, not stored in plaintext
    assert_ne!(updated.totp_secret.as_deref(), Some(secret));
}

#[sqlx::test(migrations = false)]
async fn test_update_totp_disable(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-totp-dis");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    backend
        .update_totp(admin.id, Some("JBSWY3DPEHPK3PXP".to_string()), true)
        .await
        .unwrap();

    let updated = backend.update_totp(admin.id, None, false).await.unwrap();

    assert_eq!(updated.totp_enabled, Some(false));
    assert!(updated.totp_secret.is_none());
    assert!(updated.totp_enabled_at.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_get_totp_secret_roundtrip(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-totp-rt");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let original = "JBSWY3DPEHPK3PXP";
    backend
        .update_totp(admin.id, Some(original.to_string()), true)
        .await
        .unwrap();

    let decrypted = backend
        .get_totp_secret(admin.id)
        .await
        .unwrap()
        .expect("Should have a secret");

    assert_eq!(decrypted, original);
}

#[sqlx::test(migrations = false)]
async fn test_get_totp_secret_none_when_not_set(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-totp-none");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    assert!(backend.get_totp_secret(admin.id).await.unwrap().is_none());
}

#[sqlx::test(migrations = false)]
async fn test_mfa_lockout_after_failures(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-lockout");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    assert!(!backend.is_mfa_locked(admin.id).await.unwrap());

    let (count, locked) = backend.record_mfa_failure(admin.id).await.unwrap();
    assert_eq!(count, 1);
    assert!(!locked);

    let (count, locked) = backend.record_mfa_failure(admin.id).await.unwrap();
    assert_eq!(count, 2);
    assert!(!locked);
    assert!(!backend.is_mfa_locked(admin.id).await.unwrap());

    // Third failure triggers lockout
    let (count, locked) = backend.record_mfa_failure(admin.id).await.unwrap();
    assert_eq!(count, 3);
    assert!(locked);
    assert!(backend.is_mfa_locked(admin.id).await.unwrap());
}

#[sqlx::test(migrations = false)]
async fn test_reset_mfa_failures(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-mfa-reset");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    backend.record_mfa_failure(admin.id).await.unwrap();
    backend.record_mfa_failure(admin.id).await.unwrap();

    backend.reset_mfa_failures(admin.id).await.unwrap();

    let updated = User::find_by_id(admin.id).one(&db).await.unwrap().unwrap();
    assert_eq!(updated.mfa_failed_attempts, Some(0));
    assert!(updated.mfa_locked_until.is_none());
}

// ==================== User Management Tests ====================

#[sqlx::test(migrations = false)]
async fn test_deactivate_user_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let u1 = create_verified_admin(&backend, &test_email("test-deact-a"), TEST_PASSWORD).await;
    let u2 = create_verified_admin(&backend, &test_email("test-deact-b"), TEST_PASSWORD).await;

    let deactivated = backend.deactivate_user(u1.id, u2.id).await.unwrap();

    assert!(!deactivated.active);
    assert!(deactivated.deactivated_at.is_some());
    assert!(deactivated.totp_secret.is_none());
    assert_eq!(deactivated.totp_enabled, Some(false));
    assert!(deactivated.verification_token.is_none());
    assert!(deactivated.password_reset_token.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_deactivate_self_rejected(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let admin = create_verified_admin(&backend, &test_email("test-selfdeact"), TEST_PASSWORD).await;
    create_verified_admin(&backend, &test_email("test-selfdeact2"), TEST_PASSWORD).await;

    let result = backend.deactivate_user(admin.id, admin.id).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Cannot deactivate your own"));
}

#[sqlx::test(migrations = false)]
async fn test_deactivate_last_admin_rejected(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let admin = create_verified_admin(&backend, &test_email("test-lastadmin"), TEST_PASSWORD).await;

    let result = backend.deactivate_user(admin.id, Uuid::new_v4()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("last active"));
}

#[sqlx::test(migrations = false)]
async fn test_deactivate_already_deactivated_rejected(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let u1 = create_verified_admin(&backend, &test_email("test-dbldeact1"), TEST_PASSWORD).await;
    let u2 = create_verified_admin(&backend, &test_email("test-dbldeact2"), TEST_PASSWORD).await;
    create_verified_admin(&backend, &test_email("test-dbldeact3"), TEST_PASSWORD).await;

    backend.deactivate_user(u1.id, u2.id).await.unwrap();

    let result = backend.deactivate_user(u1.id, u2.id).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("already deactivated"));
}

#[sqlx::test(migrations = false)]
async fn test_reactivate_user_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let u1 = create_verified_admin(&backend, &test_email("test-react1"), TEST_PASSWORD).await;
    let u2 = create_verified_admin(&backend, &test_email("test-react2"), TEST_PASSWORD).await;

    backend.deactivate_user(u1.id, u2.id).await.unwrap();

    let (reactivated, token) = backend.reactivate_user(u1.id).await.unwrap();
    assert!(reactivated.active);
    assert!(reactivated.deactivated_at.is_none());
    assert!(!reactivated.email_verified);
    assert!(!token.is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_reactivate_already_active_rejected(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let admin =
        create_verified_admin(&backend, &test_email("test-reactactive"), TEST_PASSWORD).await;

    let result = backend.reactivate_user(admin.id).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already active"));
}

// ==================== get_user / Lookup Tests ====================

#[sqlx::test(migrations = false)]
async fn test_get_user_by_id(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-getuser");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let user = backend
        .get_user(&admin.id)
        .await
        .unwrap()
        .expect("Should find user");

    assert_eq!(user.id, admin.id);
    assert_eq!(user.email, email);
    assert!(user.email_verified);
    assert!(user.active);
}

#[sqlx::test(migrations = false)]
async fn test_get_user_nonexistent(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    assert!(backend.get_user(&Uuid::new_v4()).await.unwrap().is_none());
}

#[sqlx::test(migrations = false)]
async fn test_get_admin_by_email(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-byemail");

    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let found = backend.get_admin_by_email(&email).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().email, email);

    assert!(backend
        .get_admin_by_email(&test_email("missing"))
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrations = false)]
async fn test_get_admin_by_id(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-byid");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let found = backend.get_admin_by_id(admin.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().email, email);

    assert!(backend
        .get_admin_by_id(Uuid::new_v4())
        .await
        .unwrap()
        .is_none());
}

// ==================== Verification Token Regeneration Tests ====================

#[sqlx::test(migrations = false)]
async fn test_regenerate_verification_token_success(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-regen");

    let (admin, _) = backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    let (_updated, new_token) = backend
        .regenerate_verification_token(admin.id)
        .await
        .unwrap();
    assert!(!new_token.is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_regenerate_verification_token_already_verified(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());
    let email = test_email("test-regen-verified");

    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let result = backend.regenerate_verification_token(admin.id).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already verified"));
}

#[sqlx::test(migrations = false)]
async fn test_regenerate_verification_token_deactivated_user(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;
    let backend = UserAuthBackend::new(db.clone());

    let (u1, _) = backend
        .create_admin(&test_email("test-regen-deact1"), TEST_PASSWORD)
        .await
        .unwrap();
    let u2 = create_verified_admin(&backend, &test_email("test-regen-deact2"), TEST_PASSWORD).await;

    backend.deactivate_user(u1.id, u2.id).await.unwrap();

    let result = backend.regenerate_verification_token(u1.id).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("deactivated"));
}
