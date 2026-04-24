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

use riposte_social::entities::{access_code, admin_user};
use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

// ==================== Helpers ====================

/// Insert an access code directly into the database (bypassing S3).
async fn insert_access_code(
    db: &DatabaseConnection,
    code: &str,
    name: &str,
    created_by: Uuid,
) -> access_code::Model {
    let model = access_code::ActiveModel {
        id: Set(Uuid::new_v4()),
        code: Set(code.to_string()),
        name: Set(name.to_string()),
        description: Set(None),
        download_filename: Set(None),
        expires_at: Set(None),
        created_at: Set(Utc::now().into()),
        created_by: Set(created_by),
        usage_count: Set(0),
        last_used_at: Set(None),
    };
    model.insert(db).await.unwrap()
}

// ==================== List access codes ====================

#[sqlx::test(migrations = false)]
async fn test_list_access_codes_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_list_access_codes_non_admin_returns_403(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("ac-viewer");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: admin_user::ActiveModel = admin.into();
    active.role = Set("viewer".to_string());
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Insufficient permissions");
}

#[sqlx::test(migrations = false)]
async fn test_list_access_codes_returns_empty_array(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("ac-list-empty");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = false)]
async fn test_list_access_codes_returns_existing_codes(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("ac-list-data");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Insert codes directly into DB
    insert_access_code(&db, "test-code-1", "Code One", admin.id).await;
    insert_access_code(&db, "test-code-2", "Code Two", admin.id).await;

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // Verify response structure
    let first = &arr[0];
    assert!(first["id"].is_string());
    assert!(first["code"].is_string());
    assert!(first["name"].is_string());
    assert!(first["created_at"].is_string());
    assert!(first["is_expired"].is_boolean());
    assert!(first["usage_count"].is_number());
}

#[sqlx::test(migrations = false)]
async fn test_list_access_codes_ordered_by_created_at_desc(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("ac-list-order");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Insert in order: first, second
    insert_access_code(&db, "first-created", "First", admin.id).await;
    // Small delay to ensure different timestamps
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    insert_access_code(&db, "second-created", "Second", admin.id).await;

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-codes").await;
    let json: serde_json::Value = response.json();
    let arr = json.as_array().unwrap();

    // Most recent first (DESC order)
    assert_eq!(arr[0]["code"].as_str().unwrap(), "second-created");
    assert_eq!(arr[1]["code"].as_str().unwrap(), "first-created");
}

// ==================== Delete access code ====================

#[sqlx::test(migrations = false)]
async fn test_delete_access_code_unauthenticated_returns_csrf_or_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    // DELETE without auth or CSRF — hits CSRF middleware first
    let response = server
        .delete(&format!(
            "/api/admin/access-codes/{}",
            Uuid::new_v4()
        ))
        .await;

    // CSRF rejects before auth check
    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_delete_access_code_removes_from_database(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("ac-delete");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let code = insert_access_code(&db, "to-delete", "Delete Me", admin.id).await;

    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/admin/access-codes/{}", code.id))
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

    // Verify it's gone from DB
    let found = access_code::Entity::find_by_id(code.id)
        .one(&db)
        .await
        .unwrap();
    assert!(found.is_none(), "Access code should be deleted from DB");
}

#[sqlx::test(migrations = false)]
async fn test_delete_nonexistent_access_code_returns_error(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("ac-delete-404");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let fake_id = Uuid::new_v4();
    let response = server
        .delete(&format!("/api/admin/access-codes/{}", fake_id))
        .add_header("x-csrf-token", &csrf)
        .await;

    // Handler returns ValidationError("Access code not found") → 400
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Access code not found");
}

#[sqlx::test(migrations = false)]
async fn test_delete_access_code_non_admin_returns_403(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("ac-delete-viewer");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let code = insert_access_code(&db, "viewer-cant-delete", "No Delete", admin.id).await;

    // Change role to viewer
    let mut active: admin_user::ActiveModel = admin.into();
    active.role = Set("viewer".to_string());
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/admin/access-codes/{}", code.id))
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Insufficient permissions");

    // Verify code still exists
    let found = access_code::Entity::find_by_id(code.id)
        .one(&db)
        .await
        .unwrap();
    assert!(found.is_some(), "Access code should not be deleted");
}

// ==================== Create access code (validation) ====================

#[sqlx::test(migrations = false)]
async fn test_create_access_code_without_csrf_returns_403(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.post("/api/admin/access-codes").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(response.text().contains("CSRF"));
}

#[sqlx::test(migrations = false)]
async fn test_create_access_code_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    // Get CSRF token (establishes session) but don't login
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/access-codes")
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}
