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

use riposte_social::settings::SettingsService;
use common::ses_mock::{build_test_email_service_any, EmailSpy};
use common::{
    build_test_server, build_test_server_with, create_verified_admin, get_csrf_token, login_as,
    test_email, TestServices, TEST_PASSWORD, TEST_TOTP_SECRET,
};

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

// ==================== Registration Tests ====================

#[sqlx::test(migrations = false)]
async fn test_register_success(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, _db) = build_server_with_spy(pool, &spy).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/register")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": test_email("reg-success"),
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Register should succeed: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    assert!(json["message"].as_str().unwrap().contains("Registration successful"));
    assert_eq!(json["email"].as_str().unwrap(), test_email("reg-success"));

    // Verification email should have been sent
    assert_eq!(spy.len(), 1, "Expected verification email to be sent");
    let captured = spy.captured();
    assert!(captured[0].to.contains(&test_email("reg-success")));
}

#[sqlx::test(migrations = false)]
async fn test_register_disabled_returns_error(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, db) = build_server_with_spy(pool, &spy).await;

    // Disable registration
    let settings = SettingsService::new(db.clone());
    settings
        .set("admin_registration_enabled", "false", Some("system"), None)
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/register")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": test_email("reg-disabled"),
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
    assert_eq!(spy.len(), 0, "No email should be sent when registration is disabled");
}

#[sqlx::test(migrations = false)]
async fn test_register_duplicate_email_returns_error(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, _db) = build_server_with_spy(pool, &spy).await;

    let email = test_email("reg-dup");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/register")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_register_weak_password_returns_error(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, _db) = build_server_with_spy(pool, &spy).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/register")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": test_email("reg-weak"),
            "password": "short",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
    assert_eq!(spy.len(), 0, "No email should be sent for weak password");
}

// ==================== Login Tests ====================

#[sqlx::test(migrations = false)]
async fn test_login_success_returns_user(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("login-ok");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/login")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["email"].as_str().unwrap(), email);
    assert!(json["id"].is_string());
    assert_eq!(json["email_verified"].as_bool().unwrap(), true);
    assert_eq!(json["totp_enabled"].as_bool().unwrap(), false);
    assert_eq!(json["mfa_required"].as_bool().unwrap(), false);
    assert_eq!(json["active"].as_bool().unwrap(), true);
    assert_eq!(json["force_password_change"].as_bool().unwrap(), false);
    assert_eq!(json["role"].as_str().unwrap(), "administrator");
    assert!(json["features"].is_object());
}

#[sqlx::test(migrations = false)]
async fn test_login_wrong_password_returns_error(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("login-bad-pw");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/login")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "password": "WrongPassword123!",
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
    assert_ne!(
        response.status_code(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "Wrong password should not cause 500"
    );
}

#[sqlx::test(migrations = false)]
async fn test_login_unverified_email_returns_error(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("login-unverified");
    backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/login")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_ne!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_login_mfa_user_returns_mfa_required(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("login-mfa");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/login")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email": email,
            "password": TEST_PASSWORD,
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["mfa_required"].as_bool().unwrap(), true);
}

// ==================== Logout Tests ====================

#[sqlx::test(migrations = false)]
async fn test_logout_clears_session(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("logout");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/logout")
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // After logout, /api/admin/me should fail
    let me_response = server.get("/api/admin/me").await;
    assert_eq!(me_response.status_code(), StatusCode::UNAUTHORIZED);
}

// ==================== Verify Email Tests ====================

#[sqlx::test(migrations = false)]
async fn test_verify_email_via_http(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("verify-http");
    let (_admin, token) = backend.create_admin(&email, TEST_PASSWORD).await.unwrap();

    let response = server
        .get(&format!("/api/admin/verify-email?token={}", token))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Verify email should succeed: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    assert!(json["message"].as_str().unwrap().contains("verified"));
    assert_eq!(json["email"].as_str().unwrap(), email);
}

// ==================== Me Endpoint Tests ====================

#[sqlx::test(migrations = false)]
async fn test_me_authenticated(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("me-authed");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/me").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["email"].as_str().unwrap(), email);
    assert_eq!(json["role"].as_str().unwrap(), "administrator");
    assert!(json["features"].is_object());
}

#[sqlx::test(migrations = false)]
async fn test_me_unauthenticated_returns_error(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/me").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}
