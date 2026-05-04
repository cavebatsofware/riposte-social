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
//! Backend integration tests for the posts module:
//! - CRUD endpoints (`POST /api/posts`, `GET/PATCH/DELETE /api/posts/{id}`).
//! - Feed visibility tier filtering (`GET /api/feed`).
//! - Media upload + serve with visibility check.

mod common;

use common::{
    build_test_server, build_test_server_with, create_verified_admin, get_csrf_token, login_as,
    s3_mock, test_email, TestServices, TEST_PASSWORD,
};

use axum::http::StatusCode;
use axum_test::multipart::{MultipartForm, Part};
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{post, user, Post, User};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

const PNG_BYTES: &[u8] =
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";

// ==================== Helpers ====================

/// Promote a verified user row to a non-administrator role (poster or
/// commenter) by direct DB update. The `create_verified_admin` helper sets
/// `role = administrator`; this rewrites it for tests that need a different
/// tier without going through the invite-acceptance flow.
async fn set_role(db: &sea_orm::DatabaseConnection, user_id: Uuid, role: &str) {
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
    db: &sea_orm::DatabaseConnection,
    email: &str,
    role: &str,
) -> user::Model {
    let row = create_verified_admin(backend, email, TEST_PASSWORD).await;
    if role != user::ROLE_ADMINISTRATOR {
        set_role(db, row.id, role).await;
    }
    User::find_by_id(row.id).one(db).await.unwrap().unwrap()
}

/// Insert a live post directly via SeaORM, bypassing the HTTP layer. Used
/// by tier tests so we can seed the DB without tracking author sessions.
async fn insert_post(
    db: &sea_orm::DatabaseConnection,
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

fn post_form(body: &str, visibility: &str) -> MultipartForm {
    MultipartForm::new()
        .add_text("body", body)
        .add_text("visibility", visibility)
}

// ==================== Create / role gating ====================

#[sqlx::test(migrations = false)]
async fn test_create_post_admin_can(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("post-admin");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("Hello **world**", "public"))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::CREATED,
        "body: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    assert_eq!(json["body"].as_str().unwrap(), "Hello **world**");
    assert_eq!(json["visibility"].as_str().unwrap(), "public");
    assert!(json["body_html"]
        .as_str()
        .unwrap()
        .contains("<strong>world</strong>"));
}

#[sqlx::test(migrations = false)]
async fn test_create_post_poster_can(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("post-poster");
    make_user(&backend, &db, &email, user::ROLE_POSTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("Poster post", "commenters"))
        .await;

    assert_eq!(response.status_code(), StatusCode::CREATED);
}

#[sqlx::test(migrations = false)]
async fn test_create_post_commenter_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("post-commenter");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("nope", "public"))
        .await;
    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_create_post_anonymous_unauthorized(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("anon", "public"))
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_create_post_invalid_visibility(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("post-bad-vis");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("hi", "everyone"))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_create_post_empty_body_rejected(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("post-empty-body");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("   ", "public"))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

// ==================== Get + visibility filter ====================

#[sqlx::test(migrations = false)]
async fn test_get_public_post_visible_to_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("po-author"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "anyone can see", post::VISIBILITY_PUBLIC).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_get_commenter_post_404_for_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("po-author2"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "commenter only", post::VISIBILITY_COMMENTERS).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    // Returns 401 Unauthorized via the AuthError mapping (we don't disclose
    // existence to anon callers; the message is "Post not found").
    let status = response.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND,
        "expected 401/404, got {}",
        status
    );
}

#[sqlx::test(migrations = false)]
async fn test_get_commenter_post_visible_to_commenter(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("po-a3"), TEST_PASSWORD).await;
    let p = insert_post(
        &db,
        admin.id,
        "commenter visible",
        post::VISIBILITY_COMMENTERS,
    )
    .await;

    let commenter_email = test_email("po-comm");
    make_user(&backend, &db, &commenter_email, user::ROLE_COMMENTER).await;
    login_as(&server, &commenter_email, TEST_PASSWORD).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_get_poster_post_blocked_for_commenter(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("po-a4"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "poster only", post::VISIBILITY_POSTERS).await;

    let commenter_email = test_email("po-comm2");
    make_user(&backend, &db, &commenter_email, user::ROLE_COMMENTER).await;
    login_as(&server, &commenter_email, TEST_PASSWORD).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    let status = response.status_code();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_get_poster_post_visible_to_poster(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("po-a5"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "poster visible", post::VISIBILITY_POSTERS).await;

    let poster_email = test_email("po-poster");
    make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    login_as(&server, &poster_email, TEST_PASSWORD).await;

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_get_post_404_for_missing(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let response = server.get(&format!("/api/posts/{}", Uuid::new_v4())).await;
    assert_ne!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_get_post_404_after_soft_delete(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("po-soft"), TEST_PASSWORD).await;
    let p = insert_post(&db, admin.id, "to be deleted", post::VISIBILITY_PUBLIC).await;

    // Soft delete directly.
    let mut active: post::ActiveModel = p.clone().into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    let response = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_ne!(response.status_code(), StatusCode::OK);
}

// ==================== Update / author check ====================

#[sqlx::test(migrations = false)]
async fn test_update_post_author_can(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("upd-author");
    let author = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let p = insert_post(&db, author.id, "v1", post::VISIBILITY_PUBLIC).await;

    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .patch(&format!("/api/posts/{}", p.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"body": "v2"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["body"].as_str().unwrap(), "v2");
}

#[sqlx::test(migrations = false)]
async fn test_update_post_other_poster_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let author_email = test_email("upd-author2");
    let author = create_verified_admin(&backend, &author_email, TEST_PASSWORD).await;
    let p = insert_post(&db, author.id, "v1", post::VISIBILITY_PUBLIC).await;

    let other_email = test_email("upd-other");
    make_user(&backend, &db, &other_email, user::ROLE_POSTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .patch(&format!("/api/posts/{}", p.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"body": "hijack"}))
        .await;
    let status = response.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401/403, got {} body: {}",
        status,
        response.text()
    );
}

#[sqlx::test(migrations = false)]
async fn test_update_post_admin_can_edit_anyone(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;

    // Author is a poster.
    let author_email = test_email("upd-admin-author");
    let author = make_user(&backend, &db, &author_email, user::ROLE_POSTER).await;
    let p = insert_post(&db, author.id, "v1", post::VISIBILITY_PUBLIC).await;

    // Admin (different user) edits.
    let admin_email = test_email("upd-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    login_as(&server, &admin_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .patch(&format!("/api/posts/{}", p.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"visibility": "commenters"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

// ==================== Delete / author check ====================

#[sqlx::test(migrations = false)]
async fn test_delete_post_author_soft_deletes(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("del-author");
    let author = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let p = insert_post(&db, author.id, "bye", post::VISIBILITY_PUBLIC).await;

    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}", p.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::NO_CONTENT);

    // Row exists with deleted_at set (soft delete).
    let row = Post::find_by_id(p.id)
        .one(&db)
        .await
        .unwrap()
        .expect("row should still exist after soft delete");
    assert!(row.deleted_at.is_some());
}

#[sqlx::test(migrations = false)]
async fn test_delete_post_other_poster_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let author_email = test_email("del-author2");
    let author = create_verified_admin(&backend, &author_email, TEST_PASSWORD).await;
    let p = insert_post(&db, author.id, "no", post::VISIBILITY_PUBLIC).await;

    let other_email = test_email("del-other");
    make_user(&backend, &db, &other_email, user::ROLE_POSTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}", p.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    let status = response.status_code();
    assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN);

    // Row was NOT deleted.
    let row = Post::find_by_id(p.id).one(&db).await.unwrap().unwrap();
    assert!(row.deleted_at.is_none());
}

// ==================== Feed visibility tier ====================

#[sqlx::test(migrations = false)]
async fn test_feed_anonymous_sees_only_public(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("feed-a"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "pub", post::VISIBILITY_PUBLIC).await;
    insert_post(&db, admin.id, "comm", post::VISIBILITY_COMMENTERS).await;
    insert_post(&db, admin.id, "pos", post::VISIBILITY_POSTERS).await;

    let response = server.get("/api/feed").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let bodies: Vec<&str> = json["posts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, vec!["pub"], "anon should see only public");
}

#[sqlx::test(migrations = false)]
async fn test_feed_commenter_sees_public_and_commenters(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("feed-a2"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "pub", post::VISIBILITY_PUBLIC).await;
    insert_post(&db, admin.id, "comm", post::VISIBILITY_COMMENTERS).await;
    insert_post(&db, admin.id, "pos", post::VISIBILITY_POSTERS).await;

    let comm_email = test_email("feed-comm");
    make_user(&backend, &db, &comm_email, user::ROLE_COMMENTER).await;
    login_as(&server, &comm_email, TEST_PASSWORD).await;

    let response = server.get("/api/feed").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let bodies: Vec<String> = json["posts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["body"].as_str().unwrap().to_string())
        .collect();
    assert!(bodies.contains(&"pub".to_string()));
    assert!(bodies.contains(&"comm".to_string()));
    assert!(!bodies.contains(&"pos".to_string()));
}

#[sqlx::test(migrations = false)]
async fn test_feed_poster_sees_all_three(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("feed-a3"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "pub", post::VISIBILITY_PUBLIC).await;
    insert_post(&db, admin.id, "comm", post::VISIBILITY_COMMENTERS).await;
    insert_post(&db, admin.id, "pos", post::VISIBILITY_POSTERS).await;

    let poster_email = test_email("feed-p");
    make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    login_as(&server, &poster_email, TEST_PASSWORD).await;

    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    assert_eq!(json["posts"].as_array().unwrap().len(), 3);
}

#[sqlx::test(migrations = false)]
async fn test_feed_excludes_soft_deleted(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("feed-del"), TEST_PASSWORD).await;
    let live = insert_post(&db, admin.id, "alive", post::VISIBILITY_PUBLIC).await;
    let dead = insert_post(&db, admin.id, "gone", post::VISIBILITY_PUBLIC).await;

    let mut active: post::ActiveModel = dead.clone().into();
    active.deleted_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids: Vec<String> = json["posts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&live.id.to_string()));
    assert!(!ids.contains(&dead.id.to_string()));
}

#[sqlx::test(migrations = false)]
async fn test_feed_cursor_pagination(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("feed-cur"), TEST_PASSWORD).await;
    for i in 0..5 {
        insert_post(&db, admin.id, &format!("p{}", i), post::VISIBILITY_PUBLIC).await;
    }

    // Page 1, limit=3, expect 3 posts and a next_cursor.
    let response = server.get("/api/feed?limit=3").await;
    let json1: serde_json::Value = response.json();
    assert_eq!(json1["posts"].as_array().unwrap().len(), 3);
    let cursor = json1["next_cursor"].as_str().expect("next_cursor present");

    // Page 2 with that cursor.
    let response = server
        .get(&format!(
            "/api/feed?limit=3&cursor={}",
            urlencoding::encode(cursor)
        ))
        .await;
    let json2: serde_json::Value = response.json();
    assert_eq!(json2["posts"].as_array().unwrap().len(), 2);
    assert!(json2["next_cursor"].is_null());
}

// ==================== Media upload + serve ====================

#[sqlx::test(migrations = false)]
async fn test_create_post_with_media_uploads_to_s3(pool: sqlx::PgPool) {
    let spy = s3_mock::S3Spy::new();
    let s3 = s3_mock::build_test_s3_service(s3_mock::mock_s3_default(&spy));
    let (server, backend, _db) = build_test_server_with(
        pool,
        TestServices {
            s3: Some(s3),
            ..Default::default()
        },
    )
    .await;
    let email = test_email("media-upload");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new()
        .add_text("body", "with image")
        .add_text("visibility", "public")
        .add_part(
            "media",
            Part::bytes(PNG_BYTES.to_vec())
                .file_name("cat.png")
                .mime_type("image/png"),
        );

    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::CREATED,
        "body: {}",
        response.text()
    );
    let json: serde_json::Value = response.json();
    let media = json["media"].as_array().unwrap();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0]["mime_type"].as_str().unwrap(), "image/png");

    let url = media[0]["url"].as_str().unwrap();
    assert!(url.starts_with("/media/"));

    // Spy captured the upload with the right key shape and content type.
    let uploads = spy.uploads();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].content_type, "image/png");
    assert!(
        uploads[0].key.starts_with("posts/"),
        "expected posts/ prefix, got {}",
        uploads[0].key
    );
}

#[sqlx::test(migrations = false)]
async fn test_create_post_rejects_disallowed_mime(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("media-bad-mime");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new()
        .add_text("body", "evil")
        .add_text("visibility", "public")
        .add_part(
            "media",
            Part::bytes(b"<svg onload=alert(1)/>".to_vec())
                .file_name("evil.svg")
                .mime_type("image/svg+xml"),
        );

    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_create_post_too_many_media_rejected(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("media-too-many");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let mut form = MultipartForm::new()
        .add_text("body", "lots")
        .add_text("visibility", "public");
    for i in 0..9 {
        form = form.add_part(
            "media",
            Part::bytes(PNG_BYTES.to_vec())
                .file_name(format!("img{}.png", i))
                .mime_type("image/png"),
        );
    }

    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_serve_media_public_visible_to_anon(pool: sqlx::PgPool) {
    // Mock S3 returns the same body for every GET; both upload and download
    // captured by the spy.
    let spy = s3_mock::S3Spy::new();
    let s3 =
        s3_mock::build_test_s3_service(s3_mock::mock_s3_get_ok_put_ok(PNG_BYTES.to_vec(), &spy));
    let (server, backend, _db) = build_test_server_with(
        pool,
        TestServices {
            s3: Some(s3),
            ..Default::default()
        },
    )
    .await;
    let email = test_email("serve-pub");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new()
        .add_text("body", "pub media")
        .add_text("visibility", "public")
        .add_part(
            "media",
            Part::bytes(PNG_BYTES.to_vec())
                .file_name("p.png")
                .mime_type("image/png"),
        );

    let create = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(create.status_code(), StatusCode::CREATED);
    let url = create.json::<serde_json::Value>()["media"][0]["url"]
        .as_str()
        .unwrap()
        .to_string();

    // Hit the media endpoint as a fresh anonymous client (no cookies).
    let media_resp = server.get(&url).clear_cookies().await;
    assert_eq!(media_resp.status_code(), StatusCode::OK);
    let cache = media_resp
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cache.contains("public"));
}

#[sqlx::test(migrations = false)]
async fn test_serve_media_commenter_post_blocked_for_anon(pool: sqlx::PgPool) {
    let spy = s3_mock::S3Spy::new();
    let s3 =
        s3_mock::build_test_s3_service(s3_mock::mock_s3_get_ok_put_ok(PNG_BYTES.to_vec(), &spy));
    let (server, backend, _db) = build_test_server_with(
        pool,
        TestServices {
            s3: Some(s3),
            ..Default::default()
        },
    )
    .await;
    let email = test_email("serve-comm");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new()
        .add_text("body", "comm media")
        .add_text("visibility", "commenters")
        .add_part(
            "media",
            Part::bytes(PNG_BYTES.to_vec())
                .file_name("c.png")
                .mime_type("image/png"),
        );

    let create = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(create.status_code(), StatusCode::CREATED);
    let url = create.json::<serde_json::Value>()["media"][0]["url"]
        .as_str()
        .unwrap()
        .to_string();

    // Anonymous request must be blocked.
    let media_resp = server.get(&url).clear_cookies().await;
    assert_ne!(media_resp.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_serve_media_404_for_missing(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let response = server.get(&format!("/media/{}", Uuid::new_v4())).await;
    assert_ne!(response.status_code(), StatusCode::OK);
}

// ==================== Feed search ====================

/// Variant of insert_post that also sets the FTS configuration. Other
/// tests use the default (English).
async fn insert_post_with_lang(
    db: &sea_orm::DatabaseConnection,
    author_id: Uuid,
    body: &str,
    visibility: &str,
    content_lang: &str,
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
        content_lang: Set(content_lang.to_string()),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_finds_matching_post(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-hit"), TEST_PASSWORD).await;
    insert_post(
        &db,
        admin.id,
        "vacation photos from cape cod",
        post::VISIBILITY_PUBLIC,
    )
    .await;
    insert_post(
        &db,
        admin.id,
        "groceries for the week",
        post::VISIBILITY_PUBLIC,
    )
    .await;

    let response = server.get("/api/feed?q=vacation").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert!(posts[0]["body"].as_str().unwrap().contains("vacation"));
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_returns_empty_on_miss(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-miss"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "hello world", post::VISIBILITY_PUBLIC).await;

    let response = server.get("/api/feed?q=zzznothingmatches").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["posts"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_respects_visibility_for_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-vis"), TEST_PASSWORD).await;
    // Private post containing the search term — anonymous viewer must not
    // find it via search even though the term appears in the body.
    insert_post(
        &db,
        admin.id,
        "secret pineapple plans",
        post::VISIBILITY_PRIVATE,
    )
    .await;

    let response = server.get("/api/feed?q=pineapple").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(
        json["posts"].as_array().unwrap().is_empty(),
        "anonymous searcher must not see private posts via search"
    );
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_respects_visibility_for_author(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("search-author");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    insert_post(
        &db,
        admin.id,
        "secret pineapple plans",
        post::VISIBILITY_PRIVATE,
    )
    .await;

    login_as(&server, &email, TEST_PASSWORD).await;
    let response = server.get("/api/feed?q=pineapple").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(
        json["posts"].as_array().unwrap().len(),
        1,
        "author should find their own private post via search"
    );
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_empty_q_is_noop(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-empty"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "post one", post::VISIBILITY_PUBLIC).await;
    insert_post(&db, admin.id, "post two", post::VISIBILITY_PUBLIC).await;

    let response = server.get("/api/feed?q=").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["posts"].as_array().unwrap().len(), 2);

    let response = server.get("/api/feed?q=%20%20").await;
    let json: serde_json::Value = response.json();
    assert_eq!(json["posts"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_english_stemming(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-en"), TEST_PASSWORD).await;
    // English stemmer collapses 'cars' and 'car' to the same stem.
    insert_post_with_lang(
        &db,
        admin.id,
        "the cars are nice",
        post::VISIBILITY_PUBLIC,
        "english",
    )
    .await;

    let response = server.get("/api/feed?q=car&lang=en").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(
        json["posts"].as_array().unwrap().len(),
        1,
        "english stemmer should match 'car' against body containing 'cars'"
    );
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_cross_language_misses(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-xlang"), TEST_PASSWORD).await;
    insert_post_with_lang(
        &db,
        admin.id,
        "estos coches son rápidos",
        post::VISIBILITY_PUBLIC,
        "spanish",
    )
    .await;

    // English searcher: explicit content_lang='english' filter excludes
    // the Spanish post regardless of stemmer overlap.
    let response = server.get("/api/feed?q=coches&lang=en").await;
    let json: serde_json::Value = response.json();
    assert!(
        json["posts"].as_array().unwrap().is_empty(),
        "english search must not match spanish-tagged post"
    );

    // Same query in Spanish does match.
    let response = server.get("/api/feed?q=coches&lang=es").await;
    let json: serde_json::Value = response.json();
    assert_eq!(json["posts"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_spanish_stemming(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-es"), TEST_PASSWORD).await;
    // Spanish stemmer collapses 'corriendo' (running) and 'corre' (runs) to
    // the same root.
    insert_post_with_lang(
        &db,
        admin.id,
        "el perro está corriendo",
        post::VISIBILITY_PUBLIC,
        "spanish",
    )
    .await;

    let response = server.get("/api/feed?q=corre&lang=es").await;
    let json: serde_json::Value = response.json();
    assert_eq!(
        json["posts"].as_array().unwrap().len(),
        1,
        "spanish stemmer should match 'corre' against body containing 'corriendo'"
    );
}

#[sqlx::test(migrations = false)]
async fn test_feed_search_malformed_query_returns_200(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-bad"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "anything", post::VISIBILITY_PUBLIC).await;

    // websearch_to_tsquery is forgiving about unbalanced operators; the
    // server should never 500 on user input.
    for bad in ["AND OR NOT)", "\"unterminated", "(()) -- 'drop'", ":::"] {
        let response = server
            .get(&format!("/api/feed?q={}", urlencoding::encode(bad)))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "malformed q '{}' should not 500",
            bad
        );
    }
}

#[sqlx::test(migrations = false)]
async fn test_create_post_invalid_content_lang_rejected(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("post-bad-lang");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;

    let form = MultipartForm::new()
        .add_text("body", "hello")
        .add_text("visibility", "public")
        .add_text("content_lang", "klingon");
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_create_post_persists_content_lang(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("post-lang-ok");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;

    let form = MultipartForm::new()
        .add_text("body", "bonjour le monde")
        .add_text("visibility", "public")
        .add_text("content_lang", "french");
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
    let json: serde_json::Value = response.json();
    assert_eq!(json["content_lang"].as_str().unwrap(), "french");

    let id = uuid::Uuid::parse_str(json["id"].as_str().unwrap()).unwrap();
    let row = Post::find_by_id(id).one(&db).await.unwrap().unwrap();
    assert_eq!(row.content_lang, "french");
}
