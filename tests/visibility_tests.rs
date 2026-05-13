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
//! Integration tests for category-driven visibility (Phase 13b):
//! - posts/albums in a category use the category's visibility, not the
//!   row's own column
//! - the new `user_list` tier reveals content to enumerated members only
//! - uncategorized posts still respect the per-row visibility column
//! - PostResponse / AlbumResponse expose `effective_visibility`
//! - compose-time member check on `user_list` categories
//! - engagement (comment/react) gates honor inheritance

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use axum_test::multipart::MultipartForm;
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{category, category_member, post, user, User};
use riposte_social::visibility::VISIBILITY_USER_LIST;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

// ==================== helpers ====================

async fn set_role(db: &sea_orm::DatabaseConnection, user_id: Uuid, role: &str) {
    let row = User::find_by_id(user_id)
        .one(db)
        .await
        .unwrap()
        .expect("user");
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

async fn insert_category(
    db: &sea_orm::DatabaseConnection,
    name: &str,
    slug: &str,
    visibility: &str,
    created_by: Option<Uuid>,
) -> category::Model {
    category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name.to_string()),
        slug: Set(slug.to_string()),
        ordinal: Set(0),
        color: Set(None),
        visibility: Set(visibility.to_string()),
        created_by: Set(created_by),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn add_member(db: &sea_orm::DatabaseConnection, category_id: Uuid, user_id: Uuid) {
    category_member::ActiveModel {
        category_id: Set(category_id),
        user_id: Set(user_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

async fn insert_post_in_category(
    db: &sea_orm::DatabaseConnection,
    author_id: Uuid,
    body: &str,
    row_visibility: &str,
    category_id: Option<Uuid>,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(body.to_string()),
        visibility: Set(row_visibility.to_string()),
        published_at: Set(now.into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(None),
        category_id: Set(category_id),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

fn feed_post_ids(json: &serde_json::Value) -> Vec<Uuid> {
    json["posts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Uuid::parse_str(p["id"].as_str().unwrap()).unwrap())
        .collect()
}

// ==================== feed inheritance ====================

#[sqlx::test(migrations = false)]
async fn test_feed_categorized_post_user_list_visible_to_member(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-admin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let member_email = test_email("vis-member");
    let member = make_user(&backend, &db, &member_email, user::ROLE_COMMENTER).await;

    let cat = insert_category(&db, "Inner", "inner", VISIBILITY_USER_LIST, Some(admin.id)).await;
    add_member(&db, cat.id, member.id).await;

    // Post lives in user_list category. Row's own visibility is `private`
    // — that should be IGNORED when categorized; the category's
    // `user_list` rules apply.
    let post = insert_post_in_category(
        &db,
        admin.id,
        "secret",
        post::VISIBILITY_PRIVATE,
        Some(cat.id),
    )
    .await;

    login_as(&server, &member_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(ids.contains(&post.id), "member should see categorized post");
}

#[sqlx::test(migrations = false)]
async fn test_feed_categorized_post_user_list_invisible_to_non_member(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-admin2");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let outsider_email = test_email("vis-outsider");
    make_user(&backend, &db, &outsider_email, user::ROLE_COMMENTER).await;

    let cat = insert_category(
        &db,
        "Inner2",
        "inner2",
        VISIBILITY_USER_LIST,
        Some(admin.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, admin.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &outsider_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(!ids.contains(&post.id), "non-member must not see post");
}

#[sqlx::test(migrations = false)]
async fn test_feed_uncategorized_post_respects_post_visibility(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-admin3");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    // Uncategorized public post — anonymous viewer must see it.
    let post =
        insert_post_in_category(&db, admin.id, "public", post::VISIBILITY_PUBLIC, None).await;

    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        ids.contains(&post.id),
        "anon should see uncategorized public post"
    );
}

#[sqlx::test(migrations = false)]
async fn test_feed_categorized_public_visible_to_anon(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-admin4");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let cat = insert_category(&db, "Open", "open", post::VISIBILITY_PUBLIC, Some(admin.id)).await;
    // Row visibility is `private` but category overrides: anon sees it.
    let post =
        insert_post_in_category(&db, admin.id, "x", post::VISIBILITY_PRIVATE, Some(cat.id)).await;

    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        ids.contains(&post.id),
        "categorized public post should be anon-visible regardless of row visibility"
    );
}

#[sqlx::test(migrations = false)]
async fn test_feed_admin_sees_user_list_category_for_moderation(pool: sqlx::PgPool) {
    // Phase 13d: admins gain moderation access at the category layer.
    // A different admin (not the cat creator, not a member) sees content
    // in any user_list category so they can moderate.
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-owner");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let other_admin_email = test_email("vis-other-admin");
    create_verified_admin(&backend, &other_admin_email, TEST_PASSWORD).await;

    let cat = insert_category(
        &db,
        "Closed",
        "closed",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, owner.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &other_admin_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        ids.contains(&post.id),
        "admin should see content in any user_list category for moderation"
    );
}

// ==================== single-row reads ====================

#[sqlx::test(migrations = false)]
async fn test_get_post_404_for_non_member(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-getadmin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let outsider_email = test_email("vis-getoutsider");
    make_user(&backend, &db, &outsider_email, user::ROLE_COMMENTER).await;

    let cat = insert_category(
        &db,
        "Closed2",
        "closed2",
        VISIBILITY_USER_LIST,
        Some(admin.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, admin.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &outsider_email, TEST_PASSWORD).await;
    let response = server.get(&format!("/api/posts/{}", post.id)).await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_post_response_effective_visibility_matches_category(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-eff");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let cat = insert_category(
        &db,
        "Open2",
        "open2",
        post::VISIBILITY_COMMENTERS,
        Some(admin.id),
    )
    .await;
    // Row visibility = public; category visibility = commenters.
    // effective_visibility must be commenters.
    let post =
        insert_post_in_category(&db, admin.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get(&format!("/api/posts/{}", post.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["visibility"].as_str().unwrap(), "public");
    assert_eq!(json["effective_visibility"].as_str().unwrap(), "commenters");
}

#[sqlx::test(migrations = false)]
async fn test_post_response_effective_visibility_uncategorized(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-effu");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let post = insert_post_in_category(&db, admin.id, "x", post::VISIBILITY_COMMENTERS, None).await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get(&format!("/api/posts/{}", post.id)).await;
    let json: serde_json::Value = response.json();
    assert_eq!(json["visibility"].as_str().unwrap(), "commenters");
    assert_eq!(json["effective_visibility"].as_str().unwrap(), "commenters");
}

// ==================== compose-time member check ====================

#[sqlx::test(migrations = false)]
async fn test_compose_post_into_user_list_category_rejected_when_author_not_member(
    pool: sqlx::PgPool,
) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-owner2");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let poster_email = test_email("vis-poster-out");
    make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;

    let cat = insert_category(
        &db,
        "Locked",
        "locked",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;

    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            MultipartForm::new()
                .add_text("body", "trying to post")
                .add_text("visibility", "public")
                .add_text("category_id", cat.id.to_string()),
        )
        .await;
    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = false)]
async fn test_compose_post_into_user_list_category_succeeds_for_member(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-owner3");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let poster_email = test_email("vis-poster-in");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;

    let cat = insert_category(
        &db,
        "Locked2",
        "locked2",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;
    add_member(&db, cat.id, poster.id).await;

    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            MultipartForm::new()
                .add_text("body", "in")
                .add_text("visibility", "public")
                .add_text("category_id", cat.id.to_string()),
        )
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::CREATED,
        "body: {}",
        response.text()
    );
}

#[sqlx::test(migrations = false)]
async fn test_compose_post_into_user_list_category_succeeds_for_admin_non_member(
    pool: sqlx::PgPool,
) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-owner4");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let other_admin_email = test_email("vis-other-admin2");
    create_verified_admin(&backend, &other_admin_email, TEST_PASSWORD).await;

    let cat = insert_category(
        &db,
        "Locked3",
        "locked3",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;

    login_as(&server, &other_admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            MultipartForm::new()
                .add_text("body", "admin override")
                .add_text("visibility", "public")
                .add_text("category_id", cat.id.to_string()),
        )
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
}

// ==================== engagement gates ====================

#[sqlx::test(migrations = false)]
async fn test_engagement_react_blocked_when_category_user_list_excludes_viewer(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-react-owner");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let outsider_email = test_email("vis-react-outsider");
    make_user(&backend, &db, &outsider_email, user::ROLE_COMMENTER).await;

    let cat = insert_category(
        &db,
        "Sealed",
        "sealed",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, owner.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &outsider_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post(&format!("/api/posts/{}/reactions", post.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"kind": "like"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_engagement_comment_blocked_when_category_user_list_excludes_viewer(
    pool: sqlx::PgPool,
) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-comm-owner");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let outsider_email = test_email("vis-comm-outsider");
    make_user(&backend, &db, &outsider_email, user::ROLE_COMMENTER).await;

    let cat = insert_category(
        &db,
        "Sealed2",
        "sealed2",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, owner.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &outsider_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post(&format!("/api/posts/{}/comments", post.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"body": "hi"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_engagement_react_succeeds_for_member(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("vis-react-ok-owner");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let member_email = test_email("vis-react-ok-member");
    let member = make_user(&backend, &db, &member_email, user::ROLE_COMMENTER).await;

    let cat = insert_category(
        &db,
        "Sealed3",
        "sealed3",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;
    add_member(&db, cat.id, member.id).await;
    let post =
        insert_post_in_category(&db, owner.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &member_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post(&format!("/api/posts/{}/reactions", post.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"kind": "like"}))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "body: {}",
        response.text()
    );
}

// ==================== anonymous ====================

#[sqlx::test(migrations = false)]
async fn test_feed_anon_does_not_see_user_list_categories(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("vis-anonadmin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let cat = insert_category(
        &db,
        "Closed3",
        "closed3",
        VISIBILITY_USER_LIST,
        Some(admin.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, admin.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(!ids.contains(&post.id));
}

// ==================== Phase 13d: creator-override ====================

#[sqlx::test(migrations = false)]
async fn test_creator_sees_own_private_category_in_list(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("13d-priv-poster");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    let cat = insert_category(
        &db,
        "Drafts",
        "drafts",
        post::VISIBILITY_PRIVATE,
        Some(poster.id),
    )
    .await;

    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let response = server.get("/api/categories").await;
    let json: serde_json::Value = response.json();
    let cats = json["categories"].as_array().unwrap();
    let ids: Vec<Uuid> = cats
        .iter()
        .map(|c| Uuid::parse_str(c["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(
        ids.contains(&cat.id),
        "poster should see their own private category"
    );
}

#[sqlx::test(migrations = false)]
async fn test_creator_can_read_post_in_own_private_category(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("13d-priv-read");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    let cat = insert_category(
        &db,
        "Drafts2",
        "drafts2",
        post::VISIBILITY_PRIVATE,
        Some(poster.id),
    )
    .await;
    let post = insert_post_in_category(
        &db,
        poster.id,
        "secret",
        post::VISIBILITY_PUBLIC,
        Some(cat.id),
    )
    .await;

    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        ids.contains(&post.id),
        "creator should see their own post in their own private category"
    );

    let response = server.get(&format!("/api/posts/{}", post.id)).await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_creator_sees_own_user_list_category_via_auto_add(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("13d-ul-poster");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;

    // Use the HTTP endpoint so the create-time auto-add fires.
    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/categories")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "name": "Family",
            "slug": "family-13d",
            "visibility": "user_list",
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
    let json: serde_json::Value = response.json();
    let cat_id = Uuid::parse_str(json["id"].as_str().unwrap()).unwrap();

    // Member list immediately includes the creator.
    let members = server
        .get(&format!("/api/categories/{}/members", cat_id))
        .await;
    assert_eq!(members.status_code(), StatusCode::OK);
    let mjson: serde_json::Value = members.json();
    let member_ids: Vec<Uuid> = mjson["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| Uuid::parse_str(m["user_id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(member_ids.contains(&poster.id));

    // And the creator can compose into their own user_list cat without
    // first manually adding themselves.
    let compose = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            MultipartForm::new()
                .add_text("body", "in my own list")
                .add_text("visibility", "public")
                .add_text("category_id", cat_id.to_string()),
        )
        .await;
    assert_eq!(
        compose.status_code(),
        StatusCode::CREATED,
        "body: {}",
        compose.text()
    );
}

// ==================== Phase 13d: admin-override ====================

#[sqlx::test(migrations = false)]
async fn test_admin_sees_other_users_private_category_in_list(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("13d-adm-poster");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    let admin_email = test_email("13d-adm-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    let cat = insert_category(
        &db,
        "PrivP",
        "privp",
        post::VISIBILITY_PRIVATE,
        Some(poster.id),
    )
    .await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get("/api/categories").await;
    let json: serde_json::Value = response.json();
    let ids: Vec<Uuid> = json["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| Uuid::parse_str(c["id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(
        ids.contains(&cat.id),
        "admin should see another user's private category for moderation"
    );
}

#[sqlx::test(migrations = false)]
async fn test_admin_sees_post_in_other_users_private_category(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("13d-mod-poster");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    let admin_email = test_email("13d-mod-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    let cat = insert_category(
        &db,
        "PrivQ",
        "privq",
        post::VISIBILITY_PRIVATE,
        Some(poster.id),
    )
    .await;
    let post =
        insert_post_in_category(&db, poster.id, "x", post::VISIBILITY_PUBLIC, Some(cat.id)).await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        ids.contains(&post.id),
        "admin should see content in private categories for moderation"
    );
}

#[sqlx::test(migrations = false)]
async fn test_admin_uncategorized_private_post_still_invisible(pool: sqlx::PgPool) {
    // Regression: admin-moderation override applies at the category
    // layer only. A poster's uncategorized private post is NOT visible
    // to other admins.
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("13d-unp-poster");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;
    let admin_email = test_email("13d-unp-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    let post = insert_post_in_category(
        &db,
        poster.id,
        "private draft",
        post::VISIBILITY_PRIVATE,
        None,
    )
    .await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        !ids.contains(&post.id),
        "admins must not see other users' uncategorized private posts"
    );
}

// ==================== Phase 13d: user_list creator lock ====================

#[sqlx::test(migrations = false)]
async fn test_user_list_creator_cannot_be_removed_via_delete(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let admin_email = test_email("13d-lock-admin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let create = server
        .post("/api/categories")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "name": "Locked",
            "slug": "locked-13d",
            "visibility": "user_list",
        }))
        .await;
    let cjson: serde_json::Value = create.json();
    let cat_id = Uuid::parse_str(cjson["id"].as_str().unwrap()).unwrap();

    // Try to remove the creator — should be rejected.
    let remove = server
        .delete(&format!("/api/categories/{}/members/{}", cat_id, admin.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(remove.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_user_list_creator_cannot_be_removed_via_put(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("13d-put-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    let other_email = test_email("13d-put-other");
    let other = make_user(&backend, &db, &other_email, user::ROLE_COMMENTER).await;

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let create = server
        .post("/api/categories")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "name": "PutLocked",
            "slug": "putlocked-13d",
            "visibility": "user_list",
        }))
        .await;
    let cjson: serde_json::Value = create.json();
    let cat_id = Uuid::parse_str(cjson["id"].as_str().unwrap()).unwrap();

    // PUT with a list that omits the creator — should be rejected.
    let put = server
        .put(&format!("/api/categories/{}/members", cat_id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "user_ids": [other.id] }))
        .await;
    assert_eq!(put.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_visibility_change_to_user_list_adds_creator_when_missing(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("13d-trans-admin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    // Start as public — no auto-add fires on create.
    let cat = insert_category(
        &db,
        "Transition",
        "trans-13d",
        post::VISIBILITY_PUBLIC,
        Some(admin.id),
    )
    .await;

    // Patch to user_list — auto-add must fire.
    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let patch = server
        .patch(&format!("/api/categories/{}", cat.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "visibility": "user_list" }))
        .await;
    assert_eq!(patch.status_code(), StatusCode::OK);

    // Members list now includes the creator.
    let members = server
        .get(&format!("/api/categories/{}/members", cat.id))
        .await;
    let mjson: serde_json::Value = members.json();
    let ids: Vec<Uuid> = mjson["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| Uuid::parse_str(m["user_id"].as_str().unwrap()).unwrap())
        .collect();
    assert!(ids.contains(&admin.id));

    // Round-trip: patch user_list -> public -> user_list. The second
    // transition must NOT raise a duplicate-key error from the leftover
    // member row.
    let patch_back = server
        .patch(&format!("/api/categories/{}", cat.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "visibility": "public" }))
        .await;
    assert_eq!(patch_back.status_code(), StatusCode::OK);
    let patch_again = server
        .patch(&format!("/api/categories/{}", cat.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "visibility": "user_list" }))
        .await;
    assert_eq!(
        patch_again.status_code(),
        StatusCode::OK,
        "round-trip to user_list must be idempotent"
    );
}

// ==================== Phase 13d: author-override safety net ====================

#[sqlx::test(migrations = false)]
async fn test_author_sees_own_post_in_user_list_category_after_revocation(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let owner_email = test_email("13d-rev-owner");
    let owner = create_verified_admin(&backend, &owner_email, TEST_PASSWORD).await;
    let poster_email = test_email("13d-rev-poster");
    let poster = make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;

    let cat = insert_category(
        &db,
        "Revoke",
        "revoke-13d",
        VISIBILITY_USER_LIST,
        Some(owner.id),
    )
    .await;
    add_member(&db, cat.id, poster.id).await;

    // Poster (a member) composes a post.
    let post = insert_post_in_category(
        &db,
        poster.id,
        "before revoke",
        post::VISIBILITY_PUBLIC,
        Some(cat.id),
    )
    .await;

    // Owner removes the poster from membership.
    login_as(&server, &owner_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let remove = server
        .delete(&format!("/api/categories/{}/members/{}", cat.id, poster.id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(remove.status_code(), StatusCode::NO_CONTENT);

    // Poster logs in — they no longer have category access, but the
    // author-override on the categorized branch keeps their own posts
    // visible to them.
    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let response = server.get("/api/feed").await;
    let json: serde_json::Value = response.json();
    let ids = feed_post_ids(&json);
    assert!(
        ids.contains(&post.id),
        "post author should see their own post even after losing category membership"
    );
}

