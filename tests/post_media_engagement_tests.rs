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
//! Backend integration tests for per-media-item engagement:
//! - Reactions (POST/DELETE) are idempotent and validate the kind allowlist.
//! - Comments support create/list/edit/soft-delete with the same author-or-
//!   admin gate as post comments.
//! - Visibility is inherited from the parent post: when the caller can't
//!   read the post, every media-engagement endpoint returns 404 without
//!   disclosing existence.
//! - Media id ↔ post id mismatches return 404 with the same surface so
//!   the URL prefix can't be probed for ownership.
//! - The lazy-loaded engagement endpoint returns counts shaped for the
//!   lightbox.

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{post, post_media, user, User};
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

async fn insert_post_row(
    db: &DatabaseConnection,
    author_id: Uuid,
    visibility: &str,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set("body".to_string()),
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

async fn insert_media_row(
    db: &DatabaseConnection,
    post_id: Uuid,
    ordinal: i32,
) -> post_media::Model {
    let media_id = Uuid::new_v4();
    post_media::ActiveModel {
        id: Set(media_id),
        post_id: Set(post_id),
        s3_key: Set(format!("posts/{}/{}", post_id, media_id)),
        mime_type: Set("image/jpeg".to_string()),
        width: Set(None),
        height: Set(None),
        ordinal: Set(ordinal),
        caption: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

// ==================== Reactions ====================

#[sqlx::test(migrations = false)]
async fn test_media_reaction_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-react-idem");
    let user = make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-react-idem-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/reactions", p.id, m.id);

    let r1 = server
        .post(&url)
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": "like" }))
        .await;
    assert_eq!(r1.status_code(), StatusCode::OK);
    let s1: serde_json::Value = r1.json();
    assert_eq!(s1["reaction_counts"]["like"].as_i64().unwrap(), 1);

    // Second add should be a no-op (count stays at 1) and return 200.
    let r2 = server
        .post(&url)
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": "like" }))
        .await;
    assert_eq!(r2.status_code(), StatusCode::OK);
    let s2: serde_json::Value = r2.json();
    assert_eq!(s2["reaction_counts"]["like"].as_i64().unwrap(), 1);
    assert_eq!(
        s2["viewer_reaction_kinds"].as_array().unwrap().len(),
        1,
        "viewer should have exactly one reaction after duplicate add",
    );

    let _ = user;
}

#[sqlx::test(migrations = false)]
async fn test_media_reaction_delete_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-react-del-idem");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-react-del-idem-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/reactions/like", p.id, m.id);
    // Delete with no prior reaction should succeed and return zero counts.
    let r = server.delete(&url).add_header("x-csrf-token", &csrf).await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let s: serde_json::Value = r.json();
    assert!(s["reaction_counts"]
        .as_object()
        .map(|m| m.is_empty())
        .unwrap_or(true));
}

#[sqlx::test(migrations = false)]
async fn test_media_reaction_fb6_kinds_accepted(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-react-fb6");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-react-fb6-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/reactions", p.id, m.id);
    for kind in ["like", "love", "haha", "wow", "sad", "angry"] {
        let r = server
            .post(&url)
            .add_header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "kind": kind }))
            .await;
        assert_eq!(
            r.status_code(),
            StatusCode::OK,
            "kind {} should be accepted",
            kind
        );
    }
}

#[sqlx::test(migrations = false)]
async fn test_media_reaction_invalid_kind_rejected(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-react-bad-kind");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-react-bad-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .post(&format!("/api/posts/{}/media/{}/reactions", p.id, m.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": "spaghetti" }))
        .await;
    assert_eq!(r.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_media_reaction_visibility_blocked_for_under_tier(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-react-vis");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-react-vis-author"),
        TEST_PASSWORD,
    )
    .await;
    // Posters-only post: a commenter shouldn't be able to react to its media.
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_POSTERS).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .post(&format!("/api/posts/{}/media/{}/reactions", p.id, m.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": "like" }))
        .await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_media_reaction_post_id_mismatch_returns_404(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-react-mismatch");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-react-mismatch-author"),
        TEST_PASSWORD,
    )
    .await;
    let p1 = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let p2 = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m_on_p1 = insert_media_row(&db, p1.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    // Use p2's id with a media that belongs to p1: should be 404.
    let r = server
        .post(&format!(
            "/api/posts/{}/media/{}/reactions",
            p2.id, m_on_p1.id
        ))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": "like" }))
        .await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
}

// ==================== Comments ====================

#[sqlx::test(migrations = false)]
async fn test_media_comment_create_and_list(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-comm-create");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-comm-create-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/comments", p.id, m.id);

    let create = server
        .post(&url)
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "nice photo" }))
        .await;
    assert_eq!(create.status_code(), StatusCode::CREATED);
    let created: serde_json::Value = create.json();
    assert_eq!(created["body"].as_str().unwrap(), "nice photo");
    assert_eq!(
        Uuid::parse_str(created["media_id"].as_str().unwrap()).unwrap(),
        m.id
    );

    let list = server.get(&url).await;
    assert_eq!(list.status_code(), StatusCode::OK);
    let listed: serde_json::Value = list.json();
    let comments = listed["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["body"].as_str().unwrap(), "nice photo");
}

#[sqlx::test(migrations = false)]
async fn test_media_comment_soft_delete_hides_from_list(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-comm-del");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-comm-del-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/comments", p.id, m.id);

    let create = server
        .post(&url)
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "doomed" }))
        .await;
    let created: serde_json::Value = create.json();
    let cid = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let del = server
        .delete(&format!("{}/{}", url, cid))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(del.status_code(), StatusCode::NO_CONTENT);

    let list: serde_json::Value = server.get(&url).await.json();
    assert_eq!(list["comments"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = false)]
async fn test_media_comment_edit_sets_edited_at(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-comm-edit");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin = create_verified_admin(
        &backend,
        &test_email("media-comm-edit-author"),
        TEST_PASSWORD,
    )
    .await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/comments", p.id, m.id);
    let created: serde_json::Value = server
        .post(&url)
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "v1" }))
        .await
        .json();
    let cid = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    assert!(created["edited_at"].is_null());

    let updated: serde_json::Value = server
        .patch(&format!("{}/{}", url, cid))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "v2 edited" }))
        .await
        .json();
    assert_eq!(updated["body"].as_str().unwrap(), "v2 edited");
    assert!(
        updated["edited_at"].is_string(),
        "edited_at should be populated after edit"
    );
}

#[sqlx::test(migrations = false)]
async fn test_media_comment_other_user_cannot_delete(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let author_email = test_email("media-comm-author");
    let admin = create_verified_admin(&backend, &test_email("media-comm-pa"), TEST_PASSWORD).await;
    make_user(&backend, &db, &author_email, user::ROLE_COMMENTER).await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;

    // Author posts the comment.
    login_as(&server, &author_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let url = format!("/api/posts/{}/media/{}/comments", p.id, m.id);
    let created: serde_json::Value = server
        .post(&url)
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "mine" }))
        .await
        .json();
    let cid = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    // Different commenter logs in and tries to delete. The ownership
    // gate is pushed into the comment WHERE clause so a non-author lookup
    // returns no row and the handler responds with a 404
    let other_email = test_email("media-comm-other");
    make_user(&backend, &db, &other_email, user::ROLE_COMMENTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;
    let csrf2 = get_csrf_token(&server).await;
    let r = server
        .delete(&format!("{}/{}", url, cid))
        .add_header("x-csrf-token", &csrf2)
        .await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
}

// ==================== Engagement summary endpoint ====================

#[sqlx::test(migrations = false)]
async fn test_media_engagement_endpoint_returns_counts(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("media-eng-summary");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    let admin =
        create_verified_admin(&backend, &test_email("media-eng-summary-a"), TEST_PASSWORD).await;
    let p = insert_post_row(&db, admin.id, post::VISIBILITY_PUBLIC).await;
    let m = insert_media_row(&db, p.id, 0).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    server
        .post(&format!("/api/posts/{}/media/{}/reactions", p.id, m.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "kind": "love" }))
        .await;
    server
        .post(&format!("/api/posts/{}/media/{}/comments", p.id, m.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "👍" }))
        .await;

    let r = server
        .get(&format!("/api/posts/{}/media/{}/engagement", p.id, m.id))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let json: serde_json::Value = r.json();
    assert_eq!(json["reaction_counts"]["love"].as_i64().unwrap(), 1);
    assert_eq!(json["comment_count"].as_i64().unwrap(), 1);
    assert_eq!(
        json["viewer_reaction_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["love"]
    );
}
