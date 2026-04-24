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

use riposte_social::entities::admin_user;
use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use sea_orm::{ActiveModelTrait, Set};
use uuid::Uuid;

// ==================== List admin users ====================

#[sqlx::test(migrations = false)]
async fn test_list_users_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/users").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_list_users_viewer_returns_403(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("au-list-viewer");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: admin_user::ActiveModel = admin.into();
    active.role = Set("viewer".to_string());
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/users").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Insufficient permissions");
}

#[sqlx::test(migrations = false)]
async fn test_list_users_returns_paginated_response(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email1 = test_email("au-list-1");
    let email2 = test_email("au-list-2");
    create_verified_admin(&backend, &email1, TEST_PASSWORD).await;
    create_verified_admin(&backend, &email2, TEST_PASSWORD).await;
    login_as(&server, &email1, TEST_PASSWORD).await;

    let response = server.get("/api/admin/users").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["data"].is_array());
    assert!(json["total"].is_number());
    assert!(json["page"].is_number());
    assert!(json["per_page"].is_number());
    assert!(json["total_pages"].is_number());
    assert_eq!(json["total"].as_u64().unwrap(), 2);
}

#[sqlx::test(migrations = false)]
async fn test_list_users_pagination_params(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email1 = test_email("au-page-1");
    let email2 = test_email("au-page-2");
    let email3 = test_email("au-page-3");
    create_verified_admin(&backend, &email1, TEST_PASSWORD).await;
    create_verified_admin(&backend, &email2, TEST_PASSWORD).await;
    create_verified_admin(&backend, &email3, TEST_PASSWORD).await;
    login_as(&server, &email1, TEST_PASSWORD).await;

    let response = server
        .get("/api/admin/users?page=1&per_page=2")
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let data = json["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(json["total"].as_u64().unwrap(), 3);
    assert_eq!(json["total_pages"].as_u64().unwrap(), 2);
}

// ==================== Get admin user ====================

#[sqlx::test(migrations = false)]
async fn test_get_user_returns_response(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("au-get");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server
        .get(&format!("/api/admin/users/{}", admin.id))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["id"].is_string());
    assert_eq!(json["email"].as_str().unwrap(), email);
    assert!(json["email_verified"].is_boolean());
    assert!(json["totp_enabled"].is_boolean());
    assert!(json["mfa_locked"].is_boolean());
    assert!(json["mfa_failed_attempts"].is_number());
    assert!(json["active"].is_boolean());
    assert!(json["force_password_change"].is_boolean());
    assert!(json["role"].is_string());
    assert!(json["created_at"].is_string());
    assert!(json["updated_at"].is_string());
}

#[sqlx::test(migrations = false)]
async fn test_get_user_not_found_returns_401(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("au-get-404");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let fake_id = Uuid::new_v4();
    let response = server
        .get(&format!("/api/admin/users/{}", fake_id))
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "User not found");
}

// ==================== Update admin user ====================

#[sqlx::test(migrations = false)]
async fn test_update_user_self_edit_returns_401(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("au-self-edit");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", admin.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"role": "viewer"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value = response.json();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Cannot edit yourself"));
}

#[sqlx::test(migrations = false)]
async fn test_update_user_change_role_to_viewer(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor_email = test_email("au-role-actor");
    let target_email = test_email("au-role-target");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"role": "viewer"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["role"].as_str().unwrap(), "viewer");
}

#[sqlx::test(migrations = false)]
async fn test_update_user_invalid_role_returns_400(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor_email = test_email("au-badrole-actor");
    let target_email = test_email("au-badrole-target");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"role": "superadmin"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = response.json();
    assert!(json["error"].as_str().unwrap().contains("Invalid role"));
}

#[sqlx::test(migrations = false)]
async fn test_update_user_set_email_verified(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor_email = test_email("au-verify-actor");
    let target_email = test_email("au-verify-target");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    // Target is already verified from create_verified_admin, so set to false then back to true
    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"email_verified": false}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["email_verified"].as_bool().unwrap(), false);
}

#[sqlx::test(migrations = false)]
async fn test_update_user_deactivate(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor_email = test_email("au-deact-actor");
    let target_email = test_email("au-deact-target");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"active": false}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["active"].as_bool().unwrap(), false);
    assert!(json["deactivated_at"].is_string());
}

#[sqlx::test(migrations = false)]
async fn test_update_user_reset_mfa_lockout(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let actor_email = test_email("au-mfa-rst-actor");
    let target_email = test_email("au-mfa-rst-target");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;

    // Set MFA failures on target
    let target_id = target.id;
    let mut active: admin_user::ActiveModel = target.into();
    active.mfa_failed_attempts = Set(Some(5));
    active.update(&db).await.unwrap();

    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target_id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"reset_mfa_lockout": true}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["mfa_failed_attempts"].as_i64().unwrap(), 0);
}

#[sqlx::test(migrations = false)]
async fn test_update_user_disable_mfa(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor_email = test_email("au-dis-mfa-actor");
    let target_email = test_email("au-dis-mfa-target");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;

    // Enable TOTP on target via backend
    backend
        .update_totp(
            target.id,
            Some("JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP".to_string()),
            true,
        )
        .await
        .unwrap();

    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"disable_mfa": true}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["totp_enabled"].as_bool().unwrap(), false);
}

// ==================== Resend verification ====================

#[sqlx::test(migrations = false)]
async fn test_resend_verification_self_returns_401(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("au-resend-self");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post(&format!(
            "/api/admin/users/{}/resend-verification",
            admin.id
        ))
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    let json: serde_json::Value = response.json();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Cannot resend verification email to yourself"));
}
