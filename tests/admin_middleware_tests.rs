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
    test_email, TEST_PASSWORD, TEST_TOTP_SECRET,
};
use riposte_social::entities::user;

use axum::http::StatusCode;
use sea_orm::{ActiveModelTrait, Set};

// ==================== require_authenticated tests ====================

#[sqlx::test(migrations = false)]
async fn test_unauthenticated_request_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Not authenticated");
}

#[sqlx::test(migrations = false)]
async fn test_authenticated_admin_access_succeeds(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mw-basic");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_mfa_enabled_not_verified_returns_403(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mw-mfa-unverified");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "MFA verification required");
}

#[sqlx::test(migrations = false)]
async fn test_mfa_enabled_and_verified_returns_200(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mw-mfa-verified");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    backend
        .update_totp(admin.id, Some(TEST_TOTP_SECRET.to_string()), true)
        .await
        .unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    // Verify MFA via the real endpoint
    let code = generate_totp_code(TEST_TOTP_SECRET, &email);
    let csrf = get_csrf_token(&server).await;
    let mfa_response = server
        .post("/api/auth/mfa/verify")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": code }))
        .await;
    assert_eq!(
        mfa_response.status_code(),
        StatusCode::OK,
        "MFA verify should succeed: {}",
        mfa_response.text()
    );

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_force_password_change_blocks_normal_routes(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("mw-forcepw");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: user::ActiveModel = admin.into();
    active.force_password_change = Set(true);
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Password change required");
}

#[sqlx::test(migrations = false)]
async fn test_force_password_change_allows_change_password_endpoint(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("mw-forcepw-cp");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: user::ActiveModel = admin.into();
    active.force_password_change = Set(true);
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/me/password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "current_password": TEST_PASSWORD,
            "new_password": "NewStr0ng!Password456"
        }))
        .await;

    // Should NOT be blocked by force_password_change middleware
    assert_ne!(
        response.text(),
        "Password change required",
        "change-password should be allowed even with force_password_change"
    );
}

#[sqlx::test(migrations = false)]
async fn test_force_password_change_allows_logout_endpoint(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("mw-forcepw-lo");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: user::ActiveModel = admin.into();
    active.force_password_change = Set(true);
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/logout")
        .add_header("x-csrf-token", &csrf)
        .await;

    // Should NOT be blocked by force_password_change middleware
    assert_ne!(
        response.text(),
        "Password change required",
        "logout should be allowed even with force_password_change"
    );
}

// ==================== require_admin tests ====================

#[sqlx::test(migrations = false)]
async fn test_administrator_role_access_succeeds(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("mw-admin-role");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_non_admin_role_returns_403(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("mw-viewer-role");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Change role to "viewer" in DB before login
    let mut active: user::ActiveModel = admin.into();
    active.role = Set("viewer".to_string());
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Insufficient permissions");
}
