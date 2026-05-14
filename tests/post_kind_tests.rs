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
//! Backend integration tests for the unified posts/albums kind discriminator:
//! - `posts.kind` defaults to `'post'` on standard create.
//! - `/api/feed` only surfaces `kind='post'` rows; album rows stay in
//!   `/api/albums` discovery.
//! - Cross-kind URL hits return 404 (the kind-mismatch case is collapsed
//!   into the same NotFound surface as a missing row so existence isn't
//!   disclosed).

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use axum_test::multipart::MultipartForm;
use riposte_social::entities::{post, Post};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

fn post_form(body: &str, visibility: &str) -> MultipartForm {
    MultipartForm::new()
        .add_text("body", body)
        .add_text("visibility", visibility)
}

fn album_form(name: &str, visibility: &str) -> MultipartForm {
    MultipartForm::new()
        .add_text("name", name)
        .add_text("visibility", visibility)
}

async fn insert_album_row(
    db: &sea_orm::DatabaseConnection,
    author_id: Uuid,
    name: &str,
    visibility: &str,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(String::new()),
        visibility: Set(visibility.to_string()),
        published_at: Set(now.into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(None),
        kind: Set(post::KIND_ALBUM.to_string()),
        slug: Set(Some(name.to_string())),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_post_row(
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

#[sqlx::test(migrations = false)]
async fn test_create_post_defaults_kind_post(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("kind-post-default");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(post_form("hello", "public"))
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);

    let json: serde_json::Value = response.json();
    let post_id = Uuid::parse_str(json["id"].as_str().unwrap()).unwrap();
    let row = Post::find_by_id(post_id).one(&db).await.unwrap().unwrap();
    assert_eq!(row.kind, post::KIND_POST);
    assert!(row.slug.is_none());
}

#[sqlx::test(migrations = false)]
async fn test_create_album_defaults_kind_album(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("kind-album-default");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/albums")
        .add_header("x-csrf-token", &csrf)
        .multipart(album_form("My Album", "public"))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::CREATED,
        "body: {}",
        response.text()
    );

    let json: serde_json::Value = response.json();
    let album_id = Uuid::parse_str(json["id"].as_str().unwrap()).unwrap();
    let row = Post::find_by_id(album_id).one(&db).await.unwrap().unwrap();
    assert_eq!(row.kind, post::KIND_ALBUM);
    assert_eq!(row.slug.as_deref(), Some("My Album"));
    // Album description maps to body; without a description, body is empty
    // string (NOT NULL column).
    assert_eq!(row.body, "");
}

#[sqlx::test(migrations = false)]
async fn test_feed_excludes_album_rows(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("kind-feed"), TEST_PASSWORD).await;

    let post_row = insert_post_row(&db, admin.id, "post body", post::VISIBILITY_PUBLIC).await;
    let album_row = insert_album_row(&db, admin.id, "album name", post::VISIBILITY_PUBLIC).await;

    let response = server.get("/api/feed").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    let ids: Vec<Uuid> = posts
        .iter()
        .map(|p| Uuid::parse_str(p["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(ids.contains(&post_row.id), "feed should include the post");
    assert!(
        !ids.contains(&album_row.id),
        "feed should not include album-kind rows"
    );
}

#[sqlx::test(migrations = false)]
async fn test_get_post_returns_404_for_album_id(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("kind-cross-1"), TEST_PASSWORD).await;
    let album = insert_album_row(&db, admin.id, "x", post::VISIBILITY_PUBLIC).await;

    let response = server.get(&format!("/api/posts/{}", album.id)).await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_get_album_returns_404_for_post_id(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("kind-cross-2"), TEST_PASSWORD).await;
    let p = insert_post_row(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;

    let response = server.get(&format!("/api/albums/{}", p.id)).await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_update_post_returns_404_for_album_id(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("kind-cross-3");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let album = insert_album_row(&db, admin.id, "x", post::VISIBILITY_PUBLIC).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .patch(&format!("/api/posts/{}", album.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "body": "new" }))
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_delete_post_returns_404_for_album_id(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("kind-cross-4");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let album = insert_album_row(&db, admin.id, "x", post::VISIBILITY_PUBLIC).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/posts/{}", album.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_delete_album_returns_404_for_post_id(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("kind-cross-5");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let p = insert_post_row(&db, admin.id, "p", post::VISIBILITY_PUBLIC).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .delete(&format!("/api/albums/{}", p.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_list_albums_excludes_post_rows(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("kind-listalb"), TEST_PASSWORD).await;
    let p = insert_post_row(&db, admin.id, "post", post::VISIBILITY_PUBLIC).await;
    let album = insert_album_row(&db, admin.id, "album", post::VISIBILITY_PUBLIC).await;

    let response = server.get("/api/albums").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let albums = json["albums"].as_array().unwrap();
    let ids: Vec<Uuid> = albums
        .iter()
        .map(|a| Uuid::parse_str(a["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(ids.contains(&album.id));
    assert!(!ids.contains(&p.id), "albums list must exclude post rows");
}
