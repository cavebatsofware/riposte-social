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
//! Backend integration tests for articles. Covers create/get/edit/delete,
//! the draft -> publish flow, the feed kind filter (`?kind`), drafts being
//! invisible to non-authors, the BM25 title-match path, and the rule that
//! post/album fetch paths never reach into article_details.

mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};

use axum::http::StatusCode;
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{post, user, Post, User};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::json;
use uuid::Uuid;

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

async fn create_article_via_api(
    server: &axum_test::TestServer,
    body: serde_json::Value,
) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .post("/api/articles")
        .add_header("x-csrf-token", &csrf)
        .json(&body)
        .await
}

async fn patch_article_via_api(
    server: &axum_test::TestServer,
    id: Uuid,
    body: serde_json::Value,
) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .patch(&format!("/api/articles/{}", id))
        .add_header("x-csrf-token", &csrf)
        .json(&body)
        .await
}

#[sqlx::test(migrations = false)]
async fn test_create_article_requires_title(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-empty-title");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = create_article_via_api(&server, json!({ "title": "" })).await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);

    let response = create_article_via_api(&server, json!({ "title": "   " })).await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_create_article_ignores_cover_media_id(pool: sqlx::PgPool) {
    // The create payload does not accept `cover_media_id` (set the
    // cover via PATCH after uploading). Clients that still send the
    // field get the standard serde "unknown field ignored" behavior,
    // and the resulting article has no cover.
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-cover-on-create");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = create_article_via_api(
        &server,
        json!({
            "title": "Has cover",
            "body": "x",
            "cover_media_id": Uuid::new_v4(),
        }),
    )
    .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    assert_eq!(body["cover_media_id"], serde_json::Value::Null);
    assert_eq!(body["cover_url"], serde_json::Value::Null);
}

#[sqlx::test(migrations = false)]
async fn test_create_article_draft_defaults_to_private(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-draft");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = create_article_via_api(
        &server,
        json!({ "title": "My first long-form piece", "body": "Hello article." }),
    )
    .await;
    assert_eq!(
        response.status_code(),
        StatusCode::CREATED,
        "body: {}",
        response.text()
    );
    let body: serde_json::Value = response.json();
    assert_eq!(body["title"], "My first long-form piece");
    assert_eq!(body["is_draft"], true);
    assert_eq!(body["visibility"], "private");
    assert!(body["id"].is_string());
}

#[sqlx::test(migrations = false)]
async fn test_publish_makes_article_visible_in_feed(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-publish");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let created = create_article_via_api(
        &server,
        json!({
            "title": "Article goes public",
            "body": "Body of the article.",
            "is_draft": false,
            "visibility": "public",
        }),
    )
    .await;
    assert_eq!(created.status_code(), StatusCode::CREATED);
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    let feed = server.get("/api/feed").await;
    assert_eq!(feed.status_code(), StatusCode::OK);
    let feed_json: serde_json::Value = feed.json();
    let posts = feed_json["posts"].as_array().unwrap();
    let found = posts
        .iter()
        .find(|p| p["id"] == id.to_string())
        .expect("article in feed");
    assert_eq!(found["kind"], "article");
    let article = &found["article"];
    assert!(!article.is_null(), "feed item exposes article preview");
    assert_eq!(article["title"], "Article goes public");
    assert!(article["reading_time_minutes"].as_i64().unwrap() >= 1);
}

#[sqlx::test(migrations = false)]
async fn test_feed_kind_filter_partitions(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("feed-kinds");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;

    // Seed a regular post.
    server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("body", "regular post")
                .add_text("visibility", "public"),
        )
        .await
        .assert_status(StatusCode::CREATED);

    // Seed a published article.
    create_article_via_api(
        &server,
        json!({
            "title": "Article one",
            "body": "Body",
            "is_draft": false,
            "visibility": "public",
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    let _ = (admin, db); // both seeded, no further assertions needed before the feed check

    let all = server.get("/api/feed").await;
    let posts_only = server.get("/api/feed?kind=posts").await;
    let articles_only = server.get("/api/feed?kind=articles").await;

    let by_kind = |resp: axum_test::TestResponse| -> Vec<String> {
        resp.json::<serde_json::Value>()["posts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["kind"].as_str().unwrap().to_string())
            .collect()
    };
    let all_kinds = by_kind(all);
    assert!(all_kinds.contains(&"post".to_string()));
    assert!(all_kinds.contains(&"article".to_string()));

    let post_kinds = by_kind(posts_only);
    assert!(post_kinds.iter().all(|k| k == "post"));

    let article_kinds = by_kind(articles_only);
    assert!(article_kinds.iter().all(|k| k == "article"));
}

#[sqlx::test(migrations = false)]
async fn test_other_user_cannot_see_draft(pool: sqlx::PgPool) {
    let (mut server, backend, db) = build_test_server(pool).await;
    let author_email = test_email("article-author");
    create_verified_admin(&backend, &author_email, TEST_PASSWORD).await;
    let other_email = test_email("article-other");
    make_user(&backend, &db, &other_email, user::ROLE_POSTER).await;

    login_as(&server, &author_email, TEST_PASSWORD).await;
    let created =
        create_article_via_api(&server, json!({ "title": "Secret draft", "body": "wip" })).await;
    assert_eq!(created.status_code(), StatusCode::CREATED);
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    // Switch to a different authenticated user.
    server.clear_cookies();
    login_as(&server, &other_email, TEST_PASSWORD).await;

    let r = server.get(&format!("/api/articles/{}", id)).await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);

    let list = server.get("/api/articles").await;
    let articles = list.json::<serde_json::Value>()["articles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["id"] == id.to_string())
        .count();
    assert_eq!(articles, 0);
}

#[sqlx::test(migrations = false)]
async fn test_draft_visibility_pinned_on_patch(pool: sqlx::PgPool) {
    let (mut server, backend, db) = build_test_server(pool).await;
    let author_email = test_email("article-pin-author");
    create_verified_admin(&backend, &author_email, TEST_PASSWORD).await;
    let other_email = test_email("article-pin-other");
    make_user(&backend, &db, &other_email, user::ROLE_POSTER).await;

    login_as(&server, &author_email, TEST_PASSWORD).await;
    let created =
        create_article_via_api(&server, json!({ "title": "Pin draft", "body": "wip" })).await;
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    // Attempt to relax visibility while keeping the row a draft. The
    // server must pin visibility back to private regardless of the
    // submitted value.
    let patched = patch_article_via_api(&server, id, json!({ "visibility": "public" })).await;
    assert_eq!(patched.status_code(), StatusCode::OK, "{}", patched.text());
    let body: serde_json::Value = patched.json();
    assert_eq!(body["is_draft"], true);
    assert_eq!(body["visibility"], "private");

    // Another viewer still 404s, because the visibility was never relaxed.
    server.clear_cookies();
    login_as(&server, &other_email, TEST_PASSWORD).await;
    let r = server.get(&format!("/api/articles/{}", id)).await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_publish_applies_supplied_visibility(pool: sqlx::PgPool) {
    // Companion check to the draft pinning rule: when the same PATCH
    // both flips is_draft=false and supplies a visibility tier, the new
    // tier sticks. Confirms the pinning logic doesn't strangle the
    // normal publish path.
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-publish-vis");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let created =
        create_article_via_api(&server, json!({ "title": "Publish vis", "body": "x" })).await;
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    let patched = patch_article_via_api(
        &server,
        id,
        json!({ "is_draft": false, "visibility": "commenters" }),
    )
    .await;
    assert_eq!(patched.status_code(), StatusCode::OK, "{}", patched.text());
    let body: serde_json::Value = patched.json();
    assert_eq!(body["is_draft"], false);
    assert_eq!(body["visibility"], "commenters");
}

#[sqlx::test(migrations = false)]
async fn test_get_article_404s_draft_regardless_of_visibility(pool: sqlx::PgPool) {
    // Defense-in-depth: even if a draft row somehow ends up with a
    // non-private visibility (older buggy data, a future code path that
    // bypasses update_article, an admin SQL fix gone wrong), the read
    // path must still hide it from non-authors. Bypass the API and flip
    // visibility directly to construct the bad state.
    let (mut server, backend, db) = build_test_server(pool).await;
    let author_email = test_email("article-defense-author");
    create_verified_admin(&backend, &author_email, TEST_PASSWORD).await;
    let other_email = test_email("article-defense-other");
    make_user(&backend, &db, &other_email, user::ROLE_POSTER).await;

    login_as(&server, &author_email, TEST_PASSWORD).await;
    let created =
        create_article_via_api(&server, json!({ "title": "Leaky draft", "body": "x" })).await;
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    let row = Post::find_by_id(id).one(&db).await.unwrap().expect("post");
    let mut active: post::ActiveModel = row.into();
    active.visibility = Set(post::VISIBILITY_PUBLIC.to_string());
    active.update(&db).await.unwrap();

    // Non-author still 404s because get_article rejects drafts even when
    // the row's visibility column would otherwise allow the read.
    server.clear_cookies();
    login_as(&server, &other_email, TEST_PASSWORD).await;
    let r = server.get(&format!("/api/articles/{}", id)).await;
    assert_eq!(r.status_code(), StatusCode::NOT_FOUND);

    // Author still sees their own draft.
    server.clear_cookies();
    login_as(&server, &author_email, TEST_PASSWORD).await;
    let r = server.get(&format!("/api/articles/{}", id)).await;
    assert_eq!(r.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = false)]
async fn test_drafts_endpoint_lists_only_own_drafts(pool: sqlx::PgPool) {
    let (mut server, backend, db) = build_test_server(pool).await;
    let mine = test_email("article-mine");
    let other = test_email("article-other-drafts");
    create_verified_admin(&backend, &mine, TEST_PASSWORD).await;
    make_user(&backend, &db, &other, user::ROLE_POSTER).await;

    // Other user's draft (should not appear in my drafts).
    login_as(&server, &other, TEST_PASSWORD).await;
    create_article_via_api(&server, json!({ "title": "Other draft" }))
        .await
        .assert_status(StatusCode::CREATED);
    server.clear_cookies();

    login_as(&server, &mine, TEST_PASSWORD).await;
    let mine_created = create_article_via_api(&server, json!({ "title": "Mine draft" }))
        .await
        .json::<serde_json::Value>();
    let mine_id = mine_created["id"].as_str().unwrap().to_string();

    let drafts = server.get("/api/articles/drafts").await;
    assert_eq!(drafts.status_code(), StatusCode::OK);
    let body: serde_json::Value = drafts.json();
    let ids: Vec<String> = body["articles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&mine_id));
    assert_eq!(
        ids.len(),
        1,
        "drafts endpoint should only return own drafts"
    );
}

#[sqlx::test(migrations = false)]
async fn test_update_recomputes_reading_time_and_excerpt(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-update");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let created = create_article_via_api(&server, json!({ "title": "Draft" })).await;
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    // ~250 words of body. div_ceil(250, 200) = 2 minutes.
    let long_body = "word ".repeat(250);
    let patched = patch_article_via_api(&server, id, json!({ "body": long_body.clone() })).await;
    assert_eq!(patched.status_code(), StatusCode::OK, "{}", patched.text());
    let body: serde_json::Value = patched.json();
    assert_eq!(body["reading_time_minutes"], 2);
    assert!(
        body["excerpt"]
            .as_str()
            .unwrap()
            .starts_with("word word word"),
        "excerpt should be auto-derived from body"
    );
}

#[sqlx::test(migrations = false)]
async fn test_cannot_unpublish_an_article(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-unpublish");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let created = create_article_via_api(
        &server,
        json!({
            "title": "Published",
            "body": "x",
            "is_draft": false,
            "visibility": "public",
        }),
    )
    .await;
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    let attempt = patch_article_via_api(&server, id, json!({ "is_draft": true })).await;
    assert_eq!(attempt.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_delete_article_removes_from_listings(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-delete");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let created = create_article_via_api(
        &server,
        json!({
            "title": "Will be deleted",
            "body": "Body",
            "is_draft": false,
            "visibility": "public",
        }),
    )
    .await;
    let id = Uuid::parse_str(created.json::<serde_json::Value>()["id"].as_str().unwrap()).unwrap();

    let csrf = get_csrf_token(&server).await;
    let resp = server
        .delete(&format!("/api/articles/{}", id))
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);

    let get = server.get(&format!("/api/articles/{}", id)).await;
    assert_eq!(get.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_cross_kind_routing(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-cross-kind");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;

    let post_resp = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("body", "regular post")
                .add_text("visibility", "public"),
        )
        .await;
    assert_eq!(post_resp.status_code(), StatusCode::CREATED);
    let post_id = Uuid::parse_str(
        post_resp.json::<serde_json::Value>()["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();

    let article_resp = create_article_via_api(
        &server,
        json!({
            "title": "Article",
            "body": "x",
            "is_draft": false,
            "visibility": "public",
        }),
    )
    .await;
    assert_eq!(article_resp.status_code(), StatusCode::CREATED);
    let article_id = Uuid::parse_str(
        article_resp.json::<serde_json::Value>()["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // /api/posts/{article_id} must 404 (kind mismatch).
    let cross_a = server.get(&format!("/api/posts/{}", article_id)).await;
    assert_eq!(cross_a.status_code(), StatusCode::NOT_FOUND);

    // /api/articles/{post_id} must 404.
    let cross_b = server.get(&format!("/api/articles/{}", post_id)).await;
    assert_eq!(cross_b.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = false)]
async fn test_post_response_omits_article_field(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-post-noembed");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;

    let post_resp = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("body", "a regular post")
                .add_text("visibility", "public"),
        )
        .await;
    assert_eq!(post_resp.status_code(), StatusCode::CREATED);
    let post_id = Uuid::parse_str(
        post_resp.json::<serde_json::Value>()["id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();

    let r = server.get(&format!("/api/posts/{}", post_id)).await;
    let body: serde_json::Value = r.json();
    assert_eq!(body["kind"], "post");
    assert!(
        body["article"].is_null(),
        "post response must not embed article preview"
    );
}

#[sqlx::test(migrations = false)]
async fn test_bm25_title_only_term_finds_article(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("article-bm25");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    create_article_via_api(
        &server,
        json!({
            "title": "Photosynthesis explained",
            "body": "plants do things",
            "is_draft": false,
            "visibility": "public",
        }),
    )
    .await
    .assert_status(StatusCode::CREATED);

    let hits = server.get("/api/feed?q=photosynthesis").await;
    assert_eq!(hits.status_code(), StatusCode::OK);
    let body: serde_json::Value = hits.json();
    let titles: Vec<String> = body["posts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| {
            p["article"]
                .as_object()
                .and_then(|a| a.get("title"))
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("Photosynthesis")),
        "title-only BM25 search should match via slug index: titles = {:?}",
        titles
    );
}
