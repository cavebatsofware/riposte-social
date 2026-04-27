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

use axum::http::StatusCode;
use common::{build_test_server, get_csrf_token};

// ==================== Tests ====================

#[sqlx::test(migrations = false)]
async fn test_get_request_exempt_from_csrf(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/auth/config").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_post_without_token_returns_403(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .post("/api/auth/login")
        .json(&serde_json::json!({"email": "x@x.com", "password": "pass"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(response.text().contains("CSRF"));
}

#[sqlx::test(migrations = false)]
async fn test_post_with_invalid_token_returns_403(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .post("/api/auth/login")
        .add_header("x-csrf-token", "bogus-token")
        .json(&serde_json::json!({"email": "x@x.com", "password": "pass"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_post_with_valid_token_passes_csrf(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let token = get_csrf_token(&server).await;

    // POST with valid CSRF token — will fail auth (401) but should NOT be 403/CSRF
    let response = server
        .post("/api/auth/login")
        .add_header("x-csrf-token", &token)
        .json(&serde_json::json!({"email": "x@x.com", "password": "wrong"}))
        .await;

    assert_ne!(
        response.status_code(),
        StatusCode::FORBIDDEN,
        "CSRF should pass; expected non-403 (got {})",
        response.status_code()
    );
}

#[sqlx::test(migrations = false)]
async fn test_put_without_token_returns_403(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .put("/api/admin/access-codes/00000000-0000-0000-0000-000000000000")
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(response.text().contains("CSRF"));
}

#[sqlx::test(migrations = false)]
async fn test_delete_without_token_returns_403(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .delete("/api/admin/access-codes/00000000-0000-0000-0000-000000000000")
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(response.text().contains("CSRF"));
}

#[sqlx::test(migrations = false)]
async fn test_cross_session_token_rejected(pool: sqlx::PgPool) {
    // Server A — fetch a CSRF token bound to its session
    let (server_a, _backend_a, _db_a) = build_test_server(pool.clone()).await;
    let token_a = get_csrf_token(&server_a).await;

    // Server B — separate cookie jar = separate session
    let (server_b, _backend_b, _db_b) = build_test_server(pool).await;
    // Establish server B's own session
    get_csrf_token(&server_b).await;

    // Use server A's token on server B — should fail
    let response = server_b
        .post("/api/auth/login")
        .add_header("x-csrf-token", &token_a)
        .json(&serde_json::json!({"email": "x@x.com", "password": "pass"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_contact_route_csrf_enforced(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .post("/api/contact")
        .json(&serde_json::json!({"name": "Test", "email": "t@t.com", "message": "hi"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(response.text().contains("CSRF"));
}

#[sqlx::test(migrations = false)]
async fn test_subscribe_route_csrf_enforced(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .post("/api/subscribe")
        .json(&serde_json::json!({"email": "t@t.com"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(response.text().contains("CSRF"));
}
