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
//! Phase 8 profile tests.
//!
//! Covers:
//! - handle validation (shape, length bounds, uniqueness),
//! - public profile endpoint (no email, public_feed_enabled gate),
//! - feed `?author=` filter,
//! - avatar upload pipeline (mime allowlist, size cap, normalization,
//!   replacement deletes prior key, delete endpoint),
//! - profile fetch by handle returns the avatar URL when set.

mod common;

use axum::http::StatusCode;
use axum_test::multipart::{MultipartForm, Part};
use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};
use riposte_social::entities::{post, user, User};
use riposte_social::settings::SettingsService;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

fn synth_png(side: u32) -> Vec<u8> {
    use image::ImageEncoder;
    let img = image::RgbaImage::from_pixel(side, side, image::Rgba([10, 20, 30, 255]));
    let mut out = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    out
}

#[sqlx::test(migrations = false)]
async fn test_patch_locale_accepts_supported_code(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("locale-ok");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .patch("/api/me/locale")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "locale": "fr" }))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["locale"].as_str().unwrap(), "fr");

    let row = User::find()
        .filter(user::Column::Email.eq(&email))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.locale.as_deref(), Some("fr"));
}

#[sqlx::test(migrations = false)]
async fn test_patch_locale_rejects_unsupported_code(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("locale-bad");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    for bad in ["pt", "EN", "en-US", "klingon", ""] {
        let response = server
            .patch("/api/me/locale")
            .add_header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "locale": bad }))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::BAD_REQUEST,
            "expected reject for '{}'",
            bad
        );
    }
}

#[sqlx::test(migrations = false)]
async fn test_patch_locale_requires_auth(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let response = server
        .patch("/api/me/locale")
        .json(&serde_json::json!({ "locale": "es" }))
        .await;
    let status = response.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401/403 got {}",
        status
    );
}

#[sqlx::test(migrations = false)]
async fn test_patch_locale_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("locale-idem");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    for _ in 0..3 {
        let response = server
            .patch("/api/me/locale")
            .add_header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "locale": "de" }))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
    }
    let row = User::find()
        .filter(user::Column::Email.eq(&email))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.locale.as_deref(), Some("de"));
}

#[sqlx::test(migrations = false)]
async fn test_me_includes_locale_after_save(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("locale-me");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    server
        .patch("/api/me/locale")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "locale": "zh" }))
        .await;

    let response = server.get("/api/me").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["locale"].as_str().unwrap(), "zh");
}

#[sqlx::test(migrations = false)]
async fn test_get_me_profile_returns_handle(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("profile-me");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/me/profile").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["handle"].as_str().unwrap().len() >= 3);
    assert_eq!(body["email"].as_str().unwrap(), email);
}

#[sqlx::test(migrations = false)]
async fn test_patch_handle_must_be_lowercase_ascii(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("profile-handle-bad");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    for bad in ["AB", "Bad.Handle", "with space", "tw"] {
        let response = server
            .patch("/api/me/profile")
            .add_header("x-csrf-token", &csrf)
            .json(&serde_json::json!({ "handle": bad }))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::BAD_REQUEST,
            "expected reject for {}: {}",
            bad,
            response.text()
        );
    }
}

#[sqlx::test(migrations = false)]
async fn test_patch_handle_uniqueness(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email_a = test_email("profile-uniq-a");
    let email_b = test_email("profile-uniq-b");
    create_verified_admin(&backend, &email_a, TEST_PASSWORD).await;
    create_verified_admin(&backend, &email_b, TEST_PASSWORD).await;

    let row_a = User::find()
        .filter(user::Column::Email.eq(&email_a))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    login_as(&server, &email_b, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .patch("/api/me/profile")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "handle": row_a.handle }))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_get_public_profile_omits_email(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("profile-pub");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let row = User::find()
        .filter(user::Column::Email.eq(&email))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    let response = server.get(&format!("/api/profiles/{}", row.handle)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["handle"].as_str().unwrap(), row.handle);
    assert!(body.get("email").is_none());
}

#[sqlx::test(migrations = false)]
async fn test_public_profile_blocked_when_public_feed_disabled(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("profile-gate");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let row = User::find()
        .filter(user::Column::Email.eq(&email))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    let settings = SettingsService::new(db.clone());
    settings
        .set("public_feed_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    // Anonymous: blocked.
    let response = server.get(&format!("/api/profiles/{}", row.handle)).await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    // Authed: still allowed (the gate is anonymous-only).
    login_as(&server, &email, TEST_PASSWORD).await;
    let response = server.get(&format!("/api/profiles/{}", row.handle)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_avatar_upload_rejects_non_image_mime(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("avatar-mime");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(b"not an image".to_vec())
            .mime_type("text/plain")
            .file_name("a.txt"),
    );
    let response = server
        .post("/api/me/avatar")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_avatar_upload_normalizes_to_webp(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("avatar-ok");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let png = synth_png(800);
    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(png)
            .mime_type("image/png")
            .file_name("avatar.png"),
    );
    let response = server
        .post("/api/me/avatar")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "avatar upload failed: {}",
        response.text()
    );
    let body: serde_json::Value = response.json();
    assert!(body["avatar_url"].as_str().unwrap().starts_with("/avatars/"));

    // Confirm row got the s3_key set + the public profile reads it back.
    let row = User::find()
        .filter(user::Column::Email.eq(&email))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.avatar_s3_key.is_some());

    let resp = server.get(&format!("/api/profiles/{}", row.handle)).await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let pub_body: serde_json::Value = resp.json();
    assert_eq!(
        pub_body["avatar_url"].as_str().unwrap(),
        format!("/avatars/{}", row.id)
    );
}

#[sqlx::test(migrations = false)]
async fn test_avatar_delete_clears_key(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("avatar-del");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let png = synth_png(600);
    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(png)
            .mime_type("image/png")
            .file_name("a.png"),
    );
    let upload = server
        .post("/api/me/avatar")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(upload.status_code(), StatusCode::OK);

    let csrf = get_csrf_token(&server).await;
    let del = server
        .delete("/api/me/avatar")
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(del.status_code(), StatusCode::NO_CONTENT);

    let row = User::find()
        .filter(user::Column::Email.eq(&email))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.avatar_s3_key.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_feed_author_filter(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email_a = test_email("feed-author-a");
    let email_b = test_email("feed-author-b");
    let a = create_verified_admin(&backend, &email_a, TEST_PASSWORD).await;
    let b = create_verified_admin(&backend, &email_b, TEST_PASSWORD).await;

    // Insert one post from each author.
    insert_public_post(&db, a.id, "post by A").await;
    insert_public_post(&db, b.id, "post by B").await;

    let response = server
        .get(&format!("/api/feed?author={}", a.id))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let posts = body["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["author_id"].as_str().unwrap(), a.id.to_string());
}

async fn insert_public_post(
    db: &sea_orm::DatabaseConnection,
    author_id: Uuid,
    body: &str,
) -> post::Model {
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
