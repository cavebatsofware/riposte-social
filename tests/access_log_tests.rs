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

use riposte_social::entities::{access_code, access_log, user};
use common::{
    build_test_server, build_test_server_with, create_verified_admin, get_csrf_token, login_as,
    test_email, TestServices, TEST_PASSWORD,
};

use axum::http::StatusCode;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, PaginatorTrait, Set};
use uuid::Uuid;

// ==================== Helpers ====================

/// Insert an access log entry directly into the database.
async fn insert_access_log(
    db: &DatabaseConnection,
    access_code: &str,
    action: &str,
    success: bool,
    ip_address: &str,
) -> access_log::Model {
    let model = access_log::ActiveModel {
        id: Set(Uuid::new_v4()),
        access_code: Set(access_code.to_string()),
        ip_address: Set(Some(ip_address.to_string())),
        user_agent: Set(Some("test-agent".to_string())),
        tokens: Set(Some(10.0)),
        last_access_time: Set(None),
        last_delta_access: Set(None),
        action: Set(action.to_string()),
        success: Set(success),
        admin_user_id: Set(None),
        admin_user_email: Set(None),
        created_at: Set(Utc::now().into()),
    };
    model.insert(db).await.unwrap()
}

/// Insert an access code with usage data for dashboard metrics testing.
async fn insert_access_code_with_usage(
    db: &DatabaseConnection,
    code: &str,
    name: &str,
    created_by: Uuid,
    usage_count: i32,
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
        usage_count: Set(usage_count),
        last_used_at: Set(Some(Utc::now().into())),
    };
    model.insert(db).await.unwrap()
}

// ==================== List access logs ====================

#[sqlx::test(migrations = false)]
async fn test_list_logs_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/access-logs").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_list_logs_viewer_returns_403(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("al-viewer");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: user::ActiveModel = admin.into();
    active.role = Set("viewer".to_string());
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-logs").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Insufficient permissions");
}

#[sqlx::test(migrations = false)]
async fn test_list_logs_returns_empty_paginated(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("al-empty");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-logs").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["data"].is_array());
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
    assert_eq!(json["total"].as_u64().unwrap(), 0);
}

#[sqlx::test(migrations = false)]
async fn test_list_logs_returns_inserted_logs(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("al-inserted");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    insert_access_log(&db, "code-1", "GET:/access/code-1", true, "203.0.113.10").await;
    insert_access_log(&db, "code-2", "GET:/access/code-2", false, "198.51.100.42").await;

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/access-logs").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let data = json["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(json["total"].as_u64().unwrap(), 2);

    // Verify response structure on first entry
    let first = &data[0];
    assert!(first["id"].is_string());
    assert!(first["access_code"].is_string());
    assert!(first["action"].is_string());
    assert!(first["success"].is_boolean());
    assert!(first["created_at"].is_string());
    assert!(first["ip_address"].is_string());
}

// ==================== Clear access logs ====================

#[sqlx::test(migrations = false)]
async fn test_clear_logs_without_csrf_returns_403(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.delete("/api/admin/access-logs").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_clear_logs_removes_all(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("al-clear");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    insert_access_log(&db, "code-1", "GET", true, "203.0.113.1").await;
    insert_access_log(&db, "code-2", "GET", true, "203.0.113.2").await;
    insert_access_log(&db, "code-3", "GET", false, "198.51.100.3").await;

    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete("/api/admin/access-logs")
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

    // Verify all logs deleted from DB
    let count = access_log::Entity::find().count(&db).await.unwrap();
    assert_eq!(count, 0);
}

// ==================== Dashboard metrics ====================

#[sqlx::test(migrations = false)]
async fn test_dashboard_metrics_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/dashboard/metrics").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_dashboard_metrics_returns_structure(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("al-dash");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Insert successful access logs (only success=true is counted in hourly metrics)
    insert_access_log(&db, "test-code", "GET:/access/test-code", true, "203.0.113.50").await;
    insert_access_log(&db, "test-code", "GET:/access/test-code", true, "198.51.100.60").await;

    // Insert an access code with recent usage
    insert_access_code_with_usage(&db, "test-code", "Test Code", admin.id, 5).await;

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/dashboard/metrics").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();

    // Verify hourly_access_rates is a 24-element array
    let hourly = json["hourly_access_rates"].as_array().unwrap();
    assert_eq!(hourly.len(), 24);
    let first_hour = &hourly[0];
    assert!(first_hour["hour"].is_string());
    assert!(first_hour["count"].is_number());

    // Verify recent_access_codes contains our code
    let recent = json["recent_access_codes"].as_array().unwrap();
    assert!(!recent.is_empty());
    let first_code = &recent[0];
    assert!(first_code["id"].is_string());
    assert!(first_code["name"].is_string());
    assert!(first_code["count"].is_number());
}

// ==================== Access log middleware write path ====================

#[sqlx::test(migrations = false)]
async fn test_access_log_middleware_writes_when_enabled(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server_with(
        pool,
        TestServices {
            enable_logging: Some(true),
            log_successful_attempts: Some(true),
            ..Default::default()
        },
    )
    .await;
    let email = test_email("al-mw-write");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    // Count existing logs before our request
    let before_count = access_log::Entity::find().count(&db).await.unwrap();

    // Make a request that should be logged (admin route, not health/asset/favicon)
    server.get("/api/admin/access-codes").await;

    // Verify a log entry was created
    let after_count = access_log::Entity::find().count(&db).await.unwrap();
    assert!(
        after_count > before_count,
        "Access log middleware should write entries when logging is enabled (before: {}, after: {})",
        before_count,
        after_count,
    );
}

#[sqlx::test(migrations = false)]
async fn test_access_log_middleware_skipped_when_disabled(pool: sqlx::PgPool) {
    // Default build_test_server has logging disabled
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("al-mw-skip");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let before_count = access_log::Entity::find().count(&db).await.unwrap();

    server.get("/api/admin/access-codes").await;

    let after_count = access_log::Entity::find().count(&db).await.unwrap();
    assert_eq!(
        before_count, after_count,
        "Access log middleware should not write entries when logging is disabled"
    );
}
