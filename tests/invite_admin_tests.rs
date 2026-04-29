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
//! Tests for the admin invite-codes CRUD endpoints:
//! - `POST /api/admin/invite-codes`
//! - `GET /api/admin/invite-codes`
//! - `DELETE /api/admin/invite-codes/{id}`

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email,
    TEST_PASSWORD,
};
use riposte_social::entities::{invite_code, InviteCode};

use axum::http::StatusCode;
use sea_orm::EntityTrait;

#[sqlx::test(migrations = false)]
async fn test_create_invite_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/invite-codes")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"email_hint": "x@example.com"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_create_invite_returns_plaintext_once(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let actor = test_email("ic-create-actor");
    create_verified_admin(&backend, &actor, TEST_PASSWORD).await;
    login_as(&server, &actor, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/invite-codes")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "email_hint": "friend@example.com",
            "expires_in_hours": 48,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::CREATED,
        "expected 201, got {} body: {}",
        response.status_code(),
        response.text()
    );

    let json: serde_json::Value = response.json();
    let plaintext = json["code"].as_str().expect("code in response");
    assert_eq!(plaintext.len(), 32, "plaintext should be 32 alphanumeric");
    assert_eq!(json["email_hint"].as_str().unwrap(), "friend@example.com");
    assert_eq!(json["status"].as_str().unwrap(), "active");

    // The DB stores the hash, not the plaintext.
    let invite_id: uuid::Uuid = serde_json::from_value(json["id"].clone()).unwrap();
    let row = InviteCode::find_by_id(invite_id)
        .one(&db)
        .await
        .unwrap()
        .expect("invite row");
    assert_ne!(row.code, plaintext, "DB column must hold hash, not plaintext");
    assert!(
        row.code.len() > 32,
        "Blake2b hex digest should be longer than the plaintext"
    );
}

#[sqlx::test(migrations = false)]
async fn test_create_invite_rejects_invalid_lifetime(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor = test_email("ic-bad-lifetime-actor");
    create_verified_admin(&backend, &actor, TEST_PASSWORD).await;
    login_as(&server, &actor, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/invite-codes")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"expires_in_hours": 0}))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

    let response = server
        .post("/api/admin/invite-codes")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"expires_in_hours": 99999}))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_list_invites_returns_status(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let actor = test_email("ic-list-actor");
    let admin = create_verified_admin(&backend, &actor, TEST_PASSWORD).await;
    login_as(&server, &actor, TEST_PASSWORD).await;

    // Issue two invites: one active, one revoked.
    let (active_invite, _) = riposte_social::invites::issue_invite_for_user(
        &db,
        admin.id,
        Some("active@example.com".to_string()),
    )
    .await
    .unwrap();
    let (revoked_invite, _) = riposte_social::invites::issue_invite_for_user(
        &db,
        admin.id,
        Some("revoked@example.com".to_string()),
    )
    .await
    .unwrap();

    // Revoke the second.
    let csrf = get_csrf_token(&server).await;
    let revoke_resp = server
        .delete(&format!("/api/admin/invite-codes/{}", revoked_invite.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(revoke_resp.status_code(), StatusCode::OK);

    let response = server.get("/api/admin/invite-codes").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let list: Vec<serde_json::Value> = response.json();
    assert!(list.len() >= 2);

    let active = list
        .iter()
        .find(|v| v["id"] == serde_json::json!(active_invite.id))
        .expect("active invite in list");
    assert_eq!(active["status"].as_str().unwrap(), "active");
    // List responses MUST NOT surface the plaintext code.
    assert!(
        active.get("code").is_none() || active["code"].is_null(),
        "list response must not include plaintext code"
    );

    let revoked = list
        .iter()
        .find(|v| v["id"] == serde_json::json!(revoked_invite.id))
        .expect("revoked invite in list");
    assert_eq!(revoked["status"].as_str().unwrap(), "revoked");
}

#[sqlx::test(migrations = false)]
async fn test_revoke_invite_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let actor = test_email("ic-revoke-actor");
    let admin = create_verified_admin(&backend, &actor, TEST_PASSWORD).await;
    login_as(&server, &actor, TEST_PASSWORD).await;

    let (invite, _) = riposte_social::invites::issue_invite_for_user(&db, admin.id, None)
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;

    // First revoke succeeds and stamps revoked_at.
    let resp1 = server
        .delete(&format!("/api/admin/invite-codes/{}", invite.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(resp1.status_code(), StatusCode::OK);
    let json1: serde_json::Value = resp1.json();
    assert_eq!(json1["status"].as_str().unwrap(), "revoked");

    // Second revoke is a no-op (still 200).
    let resp2 = server
        .delete(&format!("/api/admin/invite-codes/{}", invite.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(resp2.status_code(), StatusCode::OK);

    // DB row still has the original revoked_at; status is still revoked.
    let row = InviteCode::find_by_id(invite.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.revoked_at.is_some());
}

#[sqlx::test(migrations = false)]
async fn test_revoke_invite_nonexistent_returns_error(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let actor = test_email("ic-revoke-404-actor");
    create_verified_admin(&backend, &actor, TEST_PASSWORD).await;
    login_as(&server, &actor, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!(
            "/api/admin/invite-codes/{}",
            uuid::Uuid::new_v4()
        ))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_ne!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_revoked_invite_cannot_validate(pool: sqlx::PgPool) {
    let (_server, backend, db) =
        build_test_server(pool).await;
    let admin =
        create_verified_admin(&backend, &test_email("ic-revoke-validate"), TEST_PASSWORD)
            .await;

    let (invite, plaintext) =
        riposte_social::invites::issue_invite_for_user(&db, admin.id, None)
            .await
            .unwrap();

    // Mark as revoked directly in DB (skipping the HTTP layer for this slice).
    use sea_orm::{ActiveModelTrait, Set};
    let mut active: invite_code::ActiveModel = invite.clone().into();
    active.revoked_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    // validate_invite_code now returns None.
    let result = riposte_social::invites::validate_invite_code(&db, &plaintext)
        .await
        .unwrap();
    assert!(result.is_none(), "revoked invite must not validate");
}
