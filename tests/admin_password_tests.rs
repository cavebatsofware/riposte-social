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

use common::ses_mock::{build_test_email_service_any, EmailSpy};
use common::{
    build_test_server, build_test_server_with, create_verified_admin, generate_totp_code,
    get_csrf_token, login_as, test_email, TestServices, TEST_PASSWORD, TEST_TOTP_SECRET,
};
use riposte_social::settings::SettingsService;

use axum::http::StatusCode;

// ==================== Helpers ====================

async fn seed_site_settings(db: &sea_orm::DatabaseConnection) {
    let settings = SettingsService::new(db.clone());
    settings
        .set("site_name", "Test Site", Some("site"), None)
        .await
        .unwrap();
    settings
        .set("from_email", "noreply@test.example", Some("site"), None)
        .await
        .unwrap();
    settings
        .set("contact_email", "ops@test.example", Some("site"), None)
        .await
        .unwrap();
}

async fn build_server_with_spy(
    pool: sqlx::PgPool,
    spy: &EmailSpy,
) -> (
    axum_test::TestServer,
    riposte_social::admin::UserAuthBackend,
    sea_orm::DatabaseConnection,
) {
    let db = riposte_social::tests::test_db_from_pool(pool.clone()).await;
    seed_site_settings(&db).await;
    let email = build_test_email_service_any(spy, &db);
    build_test_server_with(
        pool,
        TestServices {
            email: Some(email),
            ..Default::default()
        },
    )
    .await
}

// ==================== Change Password Tests ====================

#[sqlx::test(migrations = false)]
async fn test_change_password_success(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, _db) = build_server_with_spy(pool, &spy).await;
    let email = test_email("cp-success");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let new_password = "NewStr0ng!Password456";
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "current_password": TEST_PASSWORD,
            "new_password": new_password,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Change password should succeed: {}",
        response.text()
    );

    // Notification email should have been sent
    assert_eq!(spy.len(), 1, "Expected password change notification email");

    // Old password should no longer work for login
    let csrf = get_csrf_token(&server).await;
    let old_pw_response = server
        .post("/api/auth/login")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "password": TEST_PASSWORD,
        }))
        .await;
    assert_ne!(old_pw_response.status_code(), StatusCode::OK);

    // New password should work
    login_as(&server, &email, new_password).await;
}

#[sqlx::test(migrations = false)]
async fn test_change_password_wrong_current_fails(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("cp-wrong-current");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "current_password": "WrongPassword123!",
            "new_password": "NewStr0ng!Password456",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

// ==================== Forgot Password Tests ====================

#[sqlx::test(migrations = false)]
async fn test_forgot_password_returns_success(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("fp-ok");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/forgot-password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "email": email }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["requires_mfa"].as_bool().unwrap(), true);
}

// ==================== Reset Password Tests ====================

#[sqlx::test(migrations = false)]
async fn test_reset_password_with_valid_token(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, _db) = build_server_with_spy(pool, &spy).await;
    let email = test_email("rp-valid");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Create a reset token via backend (bypasses MFA requirement)
    let token = backend
        .create_password_reset_token(&email)
        .await
        .unwrap()
        .expect("Should create reset token");

    let new_password = "ResetStr0ng!Password789";
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/reset-password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "token": token,
            "new_password": new_password,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Reset password should succeed: {}",
        response.text()
    );

    // Should be able to login with new password
    login_as(&server, &email, new_password).await;
}

#[sqlx::test(migrations = false)]
async fn test_reset_password_invalid_token_fails(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/reset-password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "token": "totally-bogus-token",
            "new_password": "NewStr0ng!Password456",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
    assert_ne!(
        response.status_code(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "Invalid token should not cause 500"
    );
}

// ==================== Multi-Step Flow: forgot-password -> verify-mfa -> reset ====================

#[sqlx::test(migrations = false)]
async fn test_forgot_password_verify_mfa_reset_full_flow(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, _db) = build_server_with_spy(pool, &spy).await;
    let email = test_email("fp-flow");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Enable MFA
    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    // Step 1: Request password reset
    let csrf = get_csrf_token(&server).await;
    let fp_response = server
        .post("/api/auth/forgot-password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "email": email }))
        .await;
    assert_eq!(fp_response.status_code(), StatusCode::OK);

    // Step 2: Verify MFA to get reset token sent via email
    let code = generate_totp_code(TEST_TOTP_SECRET, &email);
    let csrf = get_csrf_token(&server).await;
    let mfa_response = server
        .post("/api/auth/forgot-password/verify-mfa")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "code": code,
        }))
        .await;
    assert_eq!(
        mfa_response.status_code(),
        StatusCode::OK,
        "Forgot-password MFA verify should succeed: {}",
        mfa_response.text()
    );

    // Verify reset email was sent
    assert_eq!(spy.len(), 1, "Expected password reset email to be sent");
    let captured = spy.captured();
    assert!(captured[0].to.contains(&email));

    // Step 3: Extract reset token from backend (in production it comes from the email link)
    // The token was consumed by creating a password_reset_token in the DB, so we read it
    let admin_record = backend
        .get_admin_by_email(&email)
        .await
        .unwrap()
        .expect("Admin should exist");
    // Validate token exists (it's encrypted in DB, use backend's validate method)
    // Instead, extract from the email body which contains the reset link
    let reset_email = &captured[0];
    let token = extract_reset_token_from_email(&reset_email.html_body)
        .or_else(|| extract_reset_token_from_email(&reset_email.text_body))
        .unwrap_or_else(|| {
            // Fallback: create token directly if email parsing fails
            panic!(
                "Could not extract reset token from email. HTML body: {}",
                reset_email.html_body
            );
        });

    // Step 4: Reset password with token
    let new_password = "FlowStr0ng!Password999";
    let csrf = get_csrf_token(&server).await;
    let reset_response = server
        .post("/api/auth/reset-password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "token": token,
            "new_password": new_password,
        }))
        .await;
    assert_eq!(
        reset_response.status_code(),
        StatusCode::OK,
        "Reset password should succeed: {}",
        reset_response.text()
    );

    // Step 5: Login with new password should work
    login_as(&server, &email, new_password).await;

    // Suppress unused variable warning for admin_record
    let _ = admin_record;
}

/// Extract the reset token from the password reset email body.
/// Looks for a URL containing `token=<value>` in the email content.
fn extract_reset_token_from_email(body: &str) -> Option<String> {
    // Look for token= parameter in the email body
    let token_prefix = "token=";
    if let Some(start) = body.find(token_prefix) {
        let token_start = start + token_prefix.len();
        let remaining = &body[token_start..];
        // Token ends at whitespace, quote, angle bracket, or ampersand
        let end = remaining
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '&')
            .unwrap_or(remaining.len());
        let token = &remaining[..end];
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    None
}

#[sqlx::test(migrations = false)]
async fn test_forgot_password_verify_mfa_invalid_code_fails(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("fp-mfa-bad");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/forgot-password/verify-mfa")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "code": "000000",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}
