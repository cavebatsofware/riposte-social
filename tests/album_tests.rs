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
//! Backend integration tests for the albums alias.
//!
//! Albums are now `posts.kind='album'` rows under the hood; this file
//! exercises the album-shaped wire format (`AlbumResponse`,
//! `AlbumSummary`) and the alias semantics: name maps to `slug`,
//! description maps to `body`, the implicit cover comes from the
//! lowest-ordinal media row, and visibility is the same gate as posts.

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use axum_test::multipart::MultipartForm;
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{post, post_media, user, Post, User};
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

async fn insert_album_row(
    db: &DatabaseConnection,
    author_id: Uuid,
    name: &str,
    description: Option<&str>,
    visibility: &str,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(description.unwrap_or("").to_string()),
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

fn album_form(name: &str, visibility: &str) -> MultipartForm {
    MultipartForm::new()
        .add_text("name", name)
        .add_text("visibility", visibility)
}

// ==================== Create / role gating ====================

#[sqlx::test(migrations = false)]
async fn test_create_album_admin_can(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("album-admin");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new()
        .add_text("name", "Vacation 2026")
        .add_text("description", "trip notes")
        .add_text("visibility", "public");

    let r = server
        .post("/api/albums")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED, "body: {}", r.text());
    let json: serde_json::Value = r.json();
    assert_eq!(json["name"].as_str().unwrap(), "Vacation 2026");
    assert_eq!(json["description"].as_str().unwrap(), "trip notes");
    assert_eq!(json["visibility"].as_str().unwrap(), "public");
    assert_eq!(json["photo_count"].as_i64().unwrap(), 0);
    assert!(json["cover_url"].is_null());
}

#[sqlx::test(migrations = false)]
async fn test_create_album_requires_name(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("album-no-name");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let form = MultipartForm::new().add_text("visibility", "public");
    let r = server
        .post("/api/albums")
        .add_header("x-csrf-token", &csrf)
        .multipart(form)
        .await;
    assert_eq!(r.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_create_album_commenter_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("album-commenter");
    make_user(&backend, &db, &email, user::ROLE_COMMENTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .post("/api/albums")
        .add_header("x-csrf-token", &csrf)
        .multipart(album_form("Nope", "public"))
        .await;
    assert_eq!(r.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_create_album_anonymous_unauthorized(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let csrf = get_csrf_token(&server).await;
    let r = server
        .post("/api/albums")
        .add_header("x-csrf-token", &csrf)
        .multipart(album_form("Nope", "public"))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

// ==================== Get + visibility ====================

#[sqlx::test(migrations = false)]
async fn test_get_public_album_visible_to_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("alb-pub-a"), TEST_PASSWORD).await;
    let alb = insert_album_row(
        &db,
        admin.id,
        "Public Album",
        Some("desc"),
        post::VISIBILITY_PUBLIC,
    )
    .await;

    let r = server.get(&format!("/api/albums/{}", alb.id)).await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let json: serde_json::Value = r.json();
    assert_eq!(json["name"].as_str().unwrap(), "Public Album");
    assert_eq!(json["description"].as_str().unwrap(), "desc");
}

#[sqlx::test(migrations = false)]
async fn test_get_commenter_album_404_for_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("alb-comm-a"), TEST_PASSWORD).await;
    let alb = insert_album_row(
        &db,
        admin.id,
        "Commenter only",
        None,
        post::VISIBILITY_COMMENTERS,
    )
    .await;

    let r = server.get(&format!("/api/albums/{}", alb.id)).await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_get_album_404_for_missing_id(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let r = server.get(&format!("/api/albums/{}", Uuid::new_v4())).await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
}

// ==================== Implicit cover semantics ====================

#[sqlx::test(migrations = false)]
async fn test_album_cover_is_lowest_ordinal_media(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("alb-cover"), TEST_PASSWORD).await;
    let alb = insert_album_row(&db, admin.id, "Photos", None, post::VISIBILITY_PUBLIC).await;
    let m_first = insert_media_row(&db, alb.id, 0).await;
    let _m_second = insert_media_row(&db, alb.id, 1).await;
    let _m_third = insert_media_row(&db, alb.id, 2).await;

    let r = server.get(&format!("/api/albums/{}", alb.id)).await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let json: serde_json::Value = r.json();
    let cover_id = Uuid::parse_str(json["cover_media_id"].as_str().unwrap()).unwrap();
    assert_eq!(cover_id, m_first.id);
    assert_eq!(
        json["cover_url"].as_str().unwrap(),
        format!("/album-media/{}", m_first.id)
    );
    assert_eq!(json["photo_count"].as_i64().unwrap(), 3);
}

#[sqlx::test(migrations = false)]
async fn test_album_no_media_has_null_cover(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("alb-empty"), TEST_PASSWORD).await;
    let alb = insert_album_row(&db, admin.id, "Empty", None, post::VISIBILITY_PUBLIC).await;

    let json: serde_json::Value = server.get(&format!("/api/albums/{}", alb.id)).await.json();
    assert!(json["cover_media_id"].is_null());
    assert!(json["cover_url"].is_null());
    assert_eq!(json["photo_count"].as_i64().unwrap(), 0);
}

// ==================== List ====================

#[sqlx::test(migrations = false)]
async fn test_list_albums_includes_visible_only(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("alb-list-a"), TEST_PASSWORD).await;
    let public = insert_album_row(&db, admin.id, "Public", None, post::VISIBILITY_PUBLIC).await;
    let _commenters = insert_album_row(
        &db,
        admin.id,
        "Commenters",
        None,
        post::VISIBILITY_COMMENTERS,
    )
    .await;

    let json: serde_json::Value = server.get("/api/albums").await.json();
    let albums = json["albums"].as_array().unwrap();
    let ids: Vec<Uuid> = albums
        .iter()
        .map(|a| Uuid::parse_str(a["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(ids.contains(&public.id), "anon should see public album");
    assert_eq!(
        ids.len(),
        1,
        "anon should see only the public album, not the commenters one"
    );
}

// ==================== Update ====================

#[sqlx::test(migrations = false)]
async fn test_update_album_metadata(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("alb-update");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let alb = insert_album_row(
        &db,
        admin.id,
        "Old",
        Some("old desc"),
        post::VISIBILITY_PUBLIC,
    )
    .await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .patch(&format!("/api/albums/{}", alb.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "name": "New",
            "description": "new desc",
            "visibility": "commenters",
        }))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let json: serde_json::Value = r.json();
    assert_eq!(json["name"].as_str().unwrap(), "New");
    assert_eq!(json["description"].as_str().unwrap(), "new desc");
    assert_eq!(json["visibility"].as_str().unwrap(), "commenters");

    // Verify the underlying post row was updated.
    let row = Post::find_by_id(alb.id).one(&db).await.unwrap().unwrap();
    assert_eq!(row.slug.as_deref(), Some("New"));
    assert_eq!(row.body, "new desc");
    assert_eq!(row.visibility, post::VISIBILITY_COMMENTERS);
}

#[sqlx::test(migrations = false)]
async fn test_update_album_cover_promotes_chosen_media(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("alb-cover-update");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let alb = insert_album_row(&db, admin.id, "Photos", None, post::VISIBILITY_PUBLIC).await;
    let _first = insert_media_row(&db, alb.id, 0).await;
    let second = insert_media_row(&db, alb.id, 1).await;
    let _third = insert_media_row(&db, alb.id, 2).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .patch(&format!("/api/albums/{}", alb.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "cover_media_id": second.id }))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let json: serde_json::Value = r.json();
    let cover_id = Uuid::parse_str(json["cover_media_id"].as_str().unwrap()).unwrap();
    assert_eq!(
        cover_id, second.id,
        "the chosen media should be promoted to the implicit cover"
    );
    let media = json["media"].as_array().unwrap();
    let first_in_response = Uuid::parse_str(media[0]["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        first_in_response, second.id,
        "the chosen media should be at ordinal 0 in the returned list"
    );
}

// ==================== Delete ====================

#[sqlx::test(migrations = false)]
async fn test_delete_album_admin_can(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("alb-del-admin");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    let alb = insert_album_row(&db, admin.id, "Doomed", None, post::VISIBILITY_PUBLIC).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .delete(&format!("/api/albums/{}", alb.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(r.status_code(), StatusCode::NO_CONTENT);

    // Soft delete: row still exists with deleted_at set; subsequent GET 404s.
    let r2 = server.get(&format!("/api/albums/{}", alb.id)).await;
    assert_eq!(r2.status_code(), StatusCode::NOT_FOUND);
    let row = Post::find_by_id(alb.id).one(&db).await.unwrap().unwrap();
    assert!(row.deleted_at.is_some());
}

#[sqlx::test(migrations = false)]
async fn test_delete_album_other_user_forbidden(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner = create_verified_admin(&backend, &test_email("alb-owner"), TEST_PASSWORD).await;
    let alb = insert_album_row(&db, owner.id, "Owned", None, post::VISIBILITY_PUBLIC).await;

    let other_email = test_email("alb-other");
    make_user(&backend, &db, &other_email, user::ROLE_POSTER).await;
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let r = server
        .delete(&format!("/api/albums/{}", alb.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(r.status_code(), StatusCode::FORBIDDEN);
}
