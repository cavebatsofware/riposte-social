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
//! Backend integration tests for the admin moderation endpoints:
//! - `GET /api/admin/moderation/comments` (list, includes-deleted toggle,
//!   pagination)
//! - `POST /api/admin/moderation/comments/bulk-delete` (idempotent
//!   soft-delete batch)
//! - Role gating: non-admins blocked

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{comment, post, user, Comment, User};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

async fn set_role(db: &DatabaseConnection, user_id: Uuid, role: &str) {
    let row = User::find_by_id(user_id).one(db).await.unwrap().unwrap();
    let mut active: user::ActiveModel = row.into();
    active.role = Set(role.to_string());
    active.update(db).await.unwrap();
}

async fn make_user(
    backend: &UserAuthBackend,
    db: &DatabaseConnection,
    email: &str,
    role: &str,
) -> user::Model {
    let row = create_verified_admin(backend, email, TEST_PASSWORD).await;
    if role != user::ROLE_ADMINISTRATOR {
        set_role(db, row.id, role).await;
    }
    User::find_by_id(row.id).one(db).await.unwrap().unwrap()
}

async fn insert_post(db: &DatabaseConnection, author_id: Uuid, body: &str) -> post::Model {
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(body.to_string()),
        visibility: Set(post::VISIBILITY_PUBLIC.to_string()),
        published_at: Set(chrono::Utc::now().into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_comment(
    db: &DatabaseConnection,
    post_id: Uuid,
    user_id: Uuid,
    body: &str,
) -> comment::Model {
    comment::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_id: Set(post_id),
        user_id: Set(user_id),
        body: Set(body.to_string()),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

// ==================== Listing ====================

#[sqlx::test(migrations = false)]
async fn test_moderation_list_excludes_deleted_by_default(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("mod-list-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "post body").await;
    let live = insert_comment(&db, p.id, admin.id, "live comment").await;
    let dead = insert_comment(&db, p.id, admin.id, "dead comment").await;

    let mut active: comment::ActiveModel = dead.clone().into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get("/api/admin/moderation/comments").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let ids: Vec<&str> = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    let live_id = live.id.to_string();
    let dead_id = dead.id.to_string();
    assert!(ids.contains(&live_id.as_str()));
    assert!(!ids.contains(&dead_id.as_str()));
}

#[sqlx::test(migrations = false)]
async fn test_moderation_list_include_deleted(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("mod-incl-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p").await;
    let dead = insert_comment(&db, p.id, admin.id, "tombstone").await;

    let mut active: comment::ActiveModel = dead.clone().into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server
        .get("/api/admin/moderation/comments?include_deleted=true")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let dead_id = dead.id.to_string();
    let found = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"].as_str().unwrap() == dead_id);
    assert!(found, "deleted row missing from include_deleted=true list");
}

#[sqlx::test(migrations = false)]
async fn test_moderation_list_includes_post_excerpt_and_author(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("mod-ctx-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let body = "This is a fairly long post body that should get truncated for the moderation view";
    let p = insert_post(&db, admin.id, body).await;
    let author_email = test_email("mod-ctx-author");
    let author = make_user(&backend, &db, &author_email, user::ROLE_COMMENTER).await;
    insert_comment(&db, p.id, author.id, "comment text").await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get("/api/admin/moderation/comments").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let row = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["body"].as_str().unwrap() == "comment text")
        .expect("inserted comment should appear in list");
    assert!(row["post_excerpt"]
        .as_str()
        .unwrap()
        .contains("fairly long"));
    assert_eq!(row["author_email"].as_str().unwrap(), author_email.as_str());
}

#[sqlx::test(migrations = false)]
async fn test_moderation_list_non_admin_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let other_email = test_email("mod-no-c");
    make_user(&backend, &db, &other_email, user::ROLE_COMMENTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/moderation/comments").await;
    let status = response.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401/403 got {}",
        status
    );
}

#[sqlx::test(migrations = false)]
async fn test_moderation_list_anonymous_unauthorized(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let response = server.get("/api/admin/moderation/comments").await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

// ==================== Bulk delete ====================

#[sqlx::test(migrations = false)]
async fn test_moderation_bulk_delete_soft_deletes(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("mod-bd-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p").await;
    let c1 = insert_comment(&db, p.id, admin.id, "spam 1").await;
    let c2 = insert_comment(&db, p.id, admin.id, "spam 2").await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/moderation/comments/bulk-delete")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "comment_ids": [c1.id, c2.id],
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["deleted"].as_u64().unwrap(), 2);

    let r1 = Comment::find_by_id(c1.id).one(&db).await.unwrap().unwrap();
    let r2 = Comment::find_by_id(c2.id).one(&db).await.unwrap().unwrap();
    assert!(r1.deleted_at.is_some());
    assert!(r2.deleted_at.is_some());
}

#[sqlx::test(migrations = false)]
async fn test_moderation_bulk_delete_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("mod-idem-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p").await;
    let c = insert_comment(&db, p.id, admin.id, "spam").await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;

    let first = server
        .post("/api/admin/moderation/comments/bulk-delete")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "comment_ids": [c.id] }))
        .await;
    assert_eq!(first.status_code(), StatusCode::OK);
    assert_eq!(
        first.json::<serde_json::Value>()["deleted"]
            .as_u64()
            .unwrap(),
        1
    );

    let csrf2 = get_csrf_token(&server).await;
    let second = server
        .post("/api/admin/moderation/comments/bulk-delete")
        .add_header("x-csrf-token", &csrf2)
        .json(&serde_json::json!({ "comment_ids": [c.id] }))
        .await;
    assert_eq!(second.status_code(), StatusCode::OK);
    assert_eq!(
        second.json::<serde_json::Value>()["deleted"]
            .as_u64()
            .unwrap(),
        0,
        "re-running bulk-delete on already-deleted rows must return 0"
    );
}

#[sqlx::test(migrations = false)]
async fn test_moderation_bulk_delete_empty_rejected(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let admin_email = test_email("mod-empty-a");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    login_as(&server, &admin_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/moderation/comments/bulk-delete")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "comment_ids": [] }))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_moderation_bulk_delete_non_admin_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("mod-bd-no-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p").await;
    let c = insert_comment(&db, p.id, admin.id, "x").await;

    let other_email = test_email("mod-bd-no-c");
    make_user(&backend, &db, &other_email, user::ROLE_COMMENTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/moderation/comments/bulk-delete")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "comment_ids": [c.id] }))
        .await;
    let status = response.status_code();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN);

    let row = Comment::find_by_id(c.id).one(&db).await.unwrap().unwrap();
    assert!(
        row.deleted_at.is_none(),
        "non-admin must not be able to soft-delete via bulk endpoint"
    );
}
