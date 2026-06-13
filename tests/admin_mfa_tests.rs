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
mod common;

use common::{
    build_test_server, create_verified_admin, generate_totp_code, get_csrf_token, login_as,
    login_as_with_mfa, test_email, TEST_PASSWORD, TEST_TOTP_SECRET,
};

use axum::http::StatusCode;

// ==================== MFA Setup Tests ====================

#[sqlx::test(migrations = false)]
async fn test_mfa_setup_returns_secret_and_qr(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-setup");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/setup")
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "MFA setup should succeed: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    assert!(json["secret"].is_string(), "Should return secret");
    assert!(!json["secret"].as_str().unwrap().is_empty());
    assert!(json["qr_code"].is_string(), "Should return QR code");
    assert!(!json["qr_code"].as_str().unwrap().is_empty());
    assert!(json["otpauth_url"].is_string(), "Should return otpauth URL");
    assert!(json["otpauth_url"].as_str().unwrap().contains("otpauth://"));
}

#[sqlx::test(migrations = false)]
async fn test_mfa_setup_unauthenticated_returns_error(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/setup")
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

// ==================== MFA Confirm Setup Tests ====================

#[sqlx::test(migrations = false)]
async fn test_mfa_confirm_setup_success(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-confirm-ok");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    // Get a setup secret from the endpoint
    let csrf = get_csrf_token(&server).await;
    let setup_response = server
        .post("/api/me/mfa/setup")
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(setup_response.status_code(), StatusCode::OK);
    let setup_json: serde_json::Value = setup_response.json();
    let secret = setup_json["secret"].as_str().unwrap();

    // Generate valid code from the returned secret
    let code = generate_totp_code(secret, &email);

    // Confirm setup with valid code
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/confirm-setup")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "secret": secret,
            "code": code,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "MFA confirm should succeed: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    assert!(json["totp_enabled"].as_bool().unwrap());

    // After enabling TOTP, the session hash changes (session invalidation).
    // Verify that re-login + MFA verify grants access to protected routes.
    login_as_with_mfa(&server, &email, TEST_PASSWORD, secret).await;
    let response = server.get("/api/admin/access-codes").await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_mfa_confirm_setup_invalid_code_fails(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-confirm-bad");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    // Get a setup secret
    let csrf = get_csrf_token(&server).await;
    let setup_response = server
        .post("/api/me/mfa/setup")
        .add_header("x-csrf-token", &csrf)
        .await;
    let setup_json: serde_json::Value = setup_response.json();
    let secret = setup_json["secret"].as_str().unwrap();

    // Try to confirm with an invalid code
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/confirm-setup")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "secret": secret,
            "code": "000000",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

// ==================== MFA Disable Tests ====================

#[sqlx::test(migrations = false)]
async fn test_mfa_disable_with_correct_password(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-disable-ok");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Enable MFA via backend
    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    // Login and verify MFA
    login_as_with_mfa(&server, &email, TEST_PASSWORD, TEST_TOTP_SECRET).await;

    // Disable MFA with correct password
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/disable")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "MFA disable should succeed: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    assert!(!json["totp_enabled"].as_bool().unwrap());
}

#[sqlx::test(migrations = false)]
async fn test_mfa_disable_wrong_password_fails(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-disable-bad");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    login_as_with_mfa(&server, &email, TEST_PASSWORD, TEST_TOTP_SECRET).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/disable")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "password": "WrongPassword123!",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_mfa_disable_without_mfa_verification_fails(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-disable-noverify");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    // Login but do NOT verify MFA
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/mfa/disable")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

// ==================== MFA Verify Tests ====================

#[sqlx::test(migrations = false)]
async fn test_mfa_verify_already_verified_returns_error(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-already-verified");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    login_as_with_mfa(&server, &email, TEST_PASSWORD, TEST_TOTP_SECRET).await;

    // Second verify should fail
    let code = generate_totp_code(TEST_TOTP_SECRET, &email);
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/mfa/verify")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": code }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["error"].as_str().unwrap().contains("already verified"));
}

// ==================== Lockout -> Unlock -> Retry Cycle ====================

#[sqlx::test(migrations = false)]
async fn test_mfa_lockout_unlock_retry_cycle(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let target_email = test_email("mfa-cycle-target");
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;

    backend
        .update_totp(target.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    // Login as target
    login_as(&server, &target_email, TEST_PASSWORD).await;

    // Send 3 bad MFA codes to trigger lockout
    for _ in 0..3 {
        let csrf = get_csrf_token(&server).await;
        server
            .post("/api/auth/mfa/verify")
            .add_header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "code": "000000" }))
            .await;
    }

    // Next attempt should indicate lockout
    let csrf = get_csrf_token(&server).await;
    let locked_response = server
        .post("/api/auth/mfa/verify")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": "000000" }))
        .await;
    assert_ne!(locked_response.status_code(), StatusCode::OK);

    // Admin resets lockout via backend (simulates admin action)
    backend.reset_mfa_failures(target.id).await.unwrap();

    // Re-login and verify MFA should now succeed
    login_as(&server, &target_email, TEST_PASSWORD).await;
    let code = generate_totp_code(TEST_TOTP_SECRET, &target_email);
    let csrf = get_csrf_token(&server).await;
    let retry_response = server
        .post("/api/auth/mfa/verify")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": code }))
        .await;

    assert_eq!(
        retry_response.status_code(),
        StatusCode::OK,
        "MFA verify should succeed after lockout reset: {}",
        retry_response.text()
    );
}

#[sqlx::test(migrations = false)]
async fn test_mfa_lockout_rejects_correct_code(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mfa-locked-correct");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    // Trigger lockout with 3 bad codes
    for _ in 0..3 {
        let csrf = get_csrf_token(&server).await;
        server
            .post("/api/auth/mfa/verify")
            .add_header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "code": "000000" }))
            .await;
    }

    // A correct code must still be rejected while the lockout is active
    let code = generate_totp_code(TEST_TOTP_SECRET, &email);
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/mfa/verify")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": code }))
        .await;

    assert_ne!(
        response.status_code(),
        StatusCode::OK,
        "Correct TOTP code must be rejected while the account is locked"
    );
    let json: serde_json::Value = response.json();
    assert!(json["error"].as_str().unwrap().contains("locked"));
}
