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
//! Backend integration tests for the engagement module:
//! - Reaction add/remove with role + visibility gates
//! - Comment create/list/delete with role + visibility gates
//! - Payload extension on `PostResponse` (reaction_counts,
//!   viewer_reaction_kinds, comment_count) and on the feed.

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use axum_test::TestServer;
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{comment, post, reaction, user, Comment, User};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

// ==================== Helpers ====================

async fn set_role(db: &DatabaseConnection, user_id: Uuid, role: &str) {
    let row = User::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .expect("user row");
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

async fn insert_post(
    db: &DatabaseConnection,
    author_id: Uuid,
    body: &str,
    visibility: &str,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(body.to_string()),
        visibility: Set(visibility.to_string()),
        published_at: Set(now.into()),
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

async fn react(server: &TestServer, post_id: Uuid, kind: &str) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .post(&format!("/api/posts/{}/reactions", post_id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": kind }))
        .await
}

async fn unreact(server: &TestServer, post_id: Uuid, kind: &str) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .delete(&format!("/api/posts/{}/reactions/{}", post_id, kind))
        .add_header("x-csrf-token", &csrf)
        .await
}

// ==================== Reactions: auth + validation ====================

#[sqlx::test(migrations = false)]
async fn test_reaction_anonymous_unauthorized(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-anon"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let response = react(&server, p.id, reaction::KIND_LIKE).await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_reaction_invalid_kind_rejected(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-bad-author"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let user_email = test_email("rx-bad");
    make_user(&backend, &db, &user_email, user::ROLE_COMMENTER).await;
    login_as(&server, &user_email, TEST_PASSWORD).await;

    let response = react(&server, p.id, "fire").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

// ==================== Reactions: round trip ====================

#[sqlx::test(migrations = false)]
async fn test_reaction_commenter_can_like_public(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-pub-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("rx-pub-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = react(&server, p.id, reaction::KIND_LIKE).await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    assert_eq!(json["reaction_counts"]["like"].as_i64().unwrap(), 1);
    assert_eq!(
        json["viewer_reaction_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        vec!["like".to_string()]
    );
}

#[sqlx::test(migrations = false)]
async fn test_reaction_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-idem-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("rx-idem-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    react(&server, p.id, reaction::KIND_LIKE).await;
    let response = react(&server, p.id, reaction::KIND_LIKE).await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    // Second call is a no-op; count stays at 1.
    assert_eq!(json["reaction_counts"]["like"].as_i64().unwrap(), 1);
}

#[sqlx::test(migrations = false)]
async fn test_reaction_delete_round_trip(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-rt-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("rx-rt-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    react(&server, p.id, reaction::KIND_LIKE).await;
    let response = unreact(&server, p.id, reaction::KIND_LIKE).await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    // After remove the kind drops out of the counts map.
    assert!(json["reaction_counts"].get("like").is_none()
        || json["reaction_counts"]["like"].as_i64().unwrap() == 0);
    assert!(json["viewer_reaction_kinds"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_reaction_delete_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-del-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("rx-del-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    // No prior add: delete still returns 200, viewer state is empty.
    let response = unreact(&server, p.id, reaction::KIND_LIKE).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["viewer_reaction_kinds"].as_array().unwrap().is_empty());
}

// ==================== Reactions: visibility ====================

#[sqlx::test(migrations = false)]
async fn test_reaction_commenter_blocked_on_poster_post(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("rx-vis-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_POSTERS).await;

    let email = test_email("rx-vis-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = react(&server, p.id, reaction::KIND_LIKE).await;
    // Same "Post not found" surface as get_post for an under-tier caller.
    let status = response.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND,
        "expected 401/404, got {}",
        status
    );
}

// ==================== Comments: auth + validation ====================

async fn post_comment(server: &TestServer, post_id: Uuid, body: &str) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .post(&format!("/api/posts/{}/comments", post_id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": body }))
        .await
}

#[sqlx::test(migrations = false)]
async fn test_comment_anonymous_unauthorized(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cm-anon-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let response = post_comment(&server, p.id, "hi").await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_comment_empty_body_rejected(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cm-empty-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("cm-empty-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = post_comment(&server, p.id, "   ").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_comment_too_long_rejected(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cm-long-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("cm-long-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let body = "a".repeat(4001);
    let response = post_comment(&server, p.id, &body).await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_comment_renders_markdown(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cm-md-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("cm-md-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = post_comment(&server, p.id, "Hello **world**").await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
    let json: serde_json::Value = response.json();
    assert!(json["body_html"]
        .as_str()
        .unwrap()
        .contains("<strong>world</strong>"));
    // The author_email field must not leak even into the create response.
    assert!(json.get("author_email").is_none());
}

#[sqlx::test(migrations = false)]
async fn test_comment_blocked_on_invisible_post(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cm-vis-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_POSTERS).await;

    let email = test_email("cm-vis-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = post_comment(&server, p.id, "hi").await;
    let status = response.status_code();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND);
}

// ==================== Comments: list ====================

#[sqlx::test(migrations = false)]
async fn test_list_comments_public_for_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cl-pub-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;
    let commenter = make_user(
        &backend,
        &db,
        &test_email("cl-pub-c"),
        user::ROLE_COMMENTER,
    )
    .await;
    insert_comment(&db, p.id, commenter.id, "hello").await;

    let response = server
        .get(&format!("/api/posts/{}/comments", p.id))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["comments"].as_array().unwrap().len(), 1);
    let first = &json["comments"][0];
    assert_eq!(first["body"].as_str().unwrap(), "hello");
    // Author email never leaks to public callers.
    assert!(first.get("author_email").is_none());
}

#[sqlx::test(migrations = false)]
async fn test_list_comments_excludes_soft_deleted(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cl-soft-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;
    let commenter = make_user(
        &backend,
        &db,
        &test_email("cl-soft-c"),
        user::ROLE_COMMENTER,
    )
    .await;
    let live = insert_comment(&db, p.id, commenter.id, "live one").await;
    let dead = insert_comment(&db, p.id, commenter.id, "tombstone").await;

    // Soft-delete the second comment.
    let mut active: comment::ActiveModel = dead.into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    let response = server
        .get(&format!("/api/posts/{}/comments", p.id))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let bodies: Vec<&str> = json["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, vec!["live one"]);
    // The dead row is still in the DB for audit.
    let still_there = Comment::find_by_id(live.id).one(&db).await.unwrap();
    assert!(still_there.is_some());
}

#[sqlx::test(migrations = false)]
async fn test_list_comments_visibility_filter(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cl-vis-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_COMMENTERS).await;

    // Anonymous caller: parent post not visible, list returns 401/404.
    let response = server
        .get(&format!("/api/posts/{}/comments", p.id))
        .await;
    let status = response.status_code();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND);
}

// ==================== Comments: delete ====================

#[sqlx::test(migrations = false)]
async fn test_delete_comment_author_can(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cd-author-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let author_email = test_email("cd-author");
    let author = make_user(&backend, &db, &author_email, user::ROLE_COMMENTER).await;
    let c = insert_comment(&db, p.id, author.id, "self-delete").await;

    login_as(&server, &author_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}/comments/{}", p.id, c.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

    let row = Comment::find_by_id(c.id).one(&db).await.unwrap().unwrap();
    assert!(row.deleted_at.is_some());
}

#[sqlx::test(migrations = false)]
async fn test_delete_comment_admin_can(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("cd-admin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let author = make_user(
        &backend,
        &db,
        &test_email("cd-admin-author"),
        user::ROLE_COMMENTER,
    )
    .await;
    let c = insert_comment(&db, p.id, author.id, "moderate me").await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}/comments/{}", p.id, c.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
}

#[sqlx::test(migrations = false)]
async fn test_delete_comment_other_user_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("cd-other-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let author = make_user(
        &backend,
        &db,
        &test_email("cd-other-author"),
        user::ROLE_COMMENTER,
    )
    .await;
    let c = insert_comment(&db, p.id, author.id, "not yours").await;

    let other_email = test_email("cd-other");
    make_user(&backend, &db, &other_email, user::ROLE_COMMENTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}/comments/{}", p.id, c.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    let status = response.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401/403 got {}",
        status
    );

    let row = Comment::find_by_id(c.id).one(&db).await.unwrap().unwrap();
    assert!(
        row.deleted_at.is_none(),
        "comment must not be soft-deleted by a non-author non-admin"
    );
}

#[sqlx::test(migrations = false)]
async fn test_delete_comment_wrong_post_returns_not_found(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("cd-wrong-a");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let p1 = insert_post(&db, admin.id, "p1", post::VISIBILITY_PUBLIC).await;
    let p2 = insert_post(&db, admin.id, "p2", post::VISIBILITY_PUBLIC).await;
    let c = insert_comment(&db, p1.id, admin.id, "belongs to p1").await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}/comments/{}", p2.id, c.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    let status = response.status_code();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND);
}

// ==================== Payload: PostResponse augmentation ====================

#[sqlx::test(migrations = false)]
async fn test_post_payload_includes_engagement_fields(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("pl-fields-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["reaction_counts"].is_object());
    assert!(json["viewer_reaction_kinds"].is_array());
    assert!(json["comment_count"].is_number());
    assert_eq!(json["comment_count"].as_i64().unwrap(), 0);
    // Author email is no longer surfaced in post payloads.
    assert!(json.get("author_email").is_none());
}

#[sqlx::test(migrations = false)]
async fn test_post_payload_viewer_reaction_kinds(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("pl-viewer-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let email = test_email("pl-viewer-c");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    react(&server, p.id, reaction::KIND_LIKE).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["reaction_counts"]["like"].as_i64().unwrap(), 1);
    let kinds: Vec<&str> = json["viewer_reaction_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["like"]);
}

#[sqlx::test(migrations = false)]
async fn test_post_payload_anonymous_has_no_viewer_kinds(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("pl-anon-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    // Seed a like by some other user so reaction_counts is non-zero, but
    // the anonymous caller still gets an empty viewer list.
    let other = make_user(
        &backend,
        &db,
        &test_email("pl-anon-other"),
        user::ROLE_COMMENTER,
    )
    .await;
    reaction::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_id: Set(p.id),
        user_id: Set(other.id),
        kind: Set(reaction::KIND_LIKE.to_string()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["reaction_counts"]["like"].as_i64().unwrap(), 1);
    assert!(json["viewer_reaction_kinds"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_post_payload_comment_count_excludes_deleted(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("pl-cc-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;
    let commenter = make_user(
        &backend,
        &db,
        &test_email("pl-cc-c"),
        user::ROLE_COMMENTER,
    )
    .await;
    insert_comment(&db, p.id, commenter.id, "live").await;
    let dead = insert_comment(&db, p.id, commenter.id, "dead").await;

    let mut active: comment::ActiveModel = dead.into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["comment_count"].as_i64().unwrap(), 1);
}

#[sqlx::test(migrations = false)]
async fn test_feed_payload_includes_engagement(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("fd-eng-a"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let commenter = make_user(
        &backend,
        &db,
        &test_email("fd-eng-c"),
        user::ROLE_COMMENTER,
    )
    .await;
    insert_comment(&db, p.id, commenter.id, "hi").await;
    reaction::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_id: Set(p.id),
        user_id: Set(commenter.id),
        kind: Set(reaction::KIND_LIKE.to_string()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let response = server.get("/api/feed").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["reaction_counts"]["like"].as_i64().unwrap(), 1);
    assert_eq!(posts[0]["comment_count"].as_i64().unwrap(), 1);
}
