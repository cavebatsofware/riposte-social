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
//! Integration tests for the pg_search BM25 path on `GET /api/feed?q=`.
//! Sister-file to `tests/post_tests.rs`, which covers the basic hit /
//! miss / visibility / empty-q / malformed-q cases. This file focuses
//! on BM25-specific behavior: typo tolerance, partial-word match via
//! the body_ngram alias, score-based ranking, no cursor on search,
//! and composition with the `?author=` and `?category=` filters.

mod common;

use common::{build_test_server, create_verified_admin, test_email, TEST_PASSWORD};

use axum::http::StatusCode;
use riposte_social::entities::{category, post};
use sea_orm::{ActiveModelTrait, Set};
use uuid::Uuid;

async fn insert_post(db: &sea_orm::DatabaseConnection, author_id: Uuid, body: &str) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(body.to_string()),
        visibility: Set(post::VISIBILITY_PUBLIC.to_string()),
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

async fn insert_post_in_category(
    db: &sea_orm::DatabaseConnection,
    author_id: Uuid,
    body: &str,
    category_id: Uuid,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set(body.to_string()),
        visibility: Set(post::VISIBILITY_PUBLIC.to_string()),
        published_at: Set(now.into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(None),
        category_id: Set(Some(category_id)),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_category(
    db: &sea_orm::DatabaseConnection,
    name: &str,
    slug: &str,
) -> category::Model {
    category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name.to_string()),
        slug: Set(slug.to_string()),
        ordinal: Set(0),
        color: Set(None),
        visibility: Set(post::VISIBILITY_PUBLIC.to_string()),
        created_by: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

fn body_at(json: &serde_json::Value, idx: usize) -> &str {
    json["posts"][idx]["body"].as_str().unwrap()
}

#[sqlx::test(migrations = false)]
async fn test_search_partial_word_matches_via_ngram(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-ngram"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "photography tips for beginners").await;
    insert_post(&db, admin.id, "lunch ideas for the week").await;

    // 'photo' is a 5-char prefix of 'photography'; the body_ngram alias
    // (3-4 char ngrams) matches the shared substring even though the
    // whole-word fuzzy match (distance 1) wouldn't span the gap.
    let response = server.get("/api/feed?q=photo").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert!(body_at(&json, 0).contains("photography"));
}

#[sqlx::test(migrations = false)]
async fn test_search_typo_tolerance_matches(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-typo"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "morning hike in the mountains").await;
    insert_post(&db, admin.id, "spaghetti carbonara recipe").await;

    // 'mornign' vs 'morning' is a single transposition (Levenshtein 1).
    // paradedb.match(..., distance => 1) admits it.
    let response = server.get("/api/feed?q=mornign").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1, "single-edit typo should match via fuzzy");
    assert!(body_at(&json, 0).contains("morning"));
}

#[sqlx::test(migrations = false)]
async fn test_search_ranks_by_bm25_score(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-rank"), TEST_PASSWORD).await;
    // The first post mentions 'coffee' three times; the second mentions
    // it once. BM25 weights term frequency, so the first should
    // outrank the second when ?q=coffee.
    let high = insert_post(
        &db,
        admin.id,
        "coffee notes: morning coffee, afternoon coffee",
    )
    .await;
    let low = insert_post(&db, admin.id, "tea or coffee with breakfast").await;

    let response = server.get("/api/feed?q=coffee").await;
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 2);
    let first = Uuid::parse_str(posts[0]["id"].as_str().unwrap()).unwrap();
    let second = Uuid::parse_str(posts[1]["id"].as_str().unwrap()).unwrap();
    assert_eq!(first, high.id, "higher term frequency should rank first");
    assert_eq!(second, low.id);
}

#[sqlx::test(migrations = false)]
async fn test_search_composes_with_author_filter(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let alice =
        create_verified_admin(&backend, &test_email("search-author-a"), TEST_PASSWORD).await;
    let bob = create_verified_admin(&backend, &test_email("search-author-b"), TEST_PASSWORD).await;
    let alice_post = insert_post(&db, alice.id, "kayaking on the river").await;
    let _bob_post = insert_post(&db, bob.id, "kayaking off the cape").await;

    // Both posts match 'kayaking'; the author filter must trim to one.
    let response = server
        .get(&format!("/api/feed?q=kayaking&author={}", alice.id))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["id"].as_str().unwrap(), alice_post.id.to_string());
}

#[sqlx::test(migrations = false)]
async fn test_search_composes_with_category_filter(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-cat"), TEST_PASSWORD).await;
    let travel = insert_category(&db, "Travel", "travel").await;
    let in_cat = insert_post_in_category(&db, admin.id, "vacation in tokyo", travel.id).await;
    let _out_of_cat = insert_post(&db, admin.id, "vacation at home").await;

    // Both match 'vacation'; the category filter must drop the
    // uncategorized post.
    let response = server.get("/api/feed?q=vacation&category=travel").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let posts = json["posts"].as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["id"].as_str().unwrap(), in_cat.id.to_string());
}

#[sqlx::test(migrations = false)]
async fn test_search_response_has_no_cursor(pool: sqlx::PgPool) {
    // Search responses collapse to a single page by design (the cursor
    // format and BM25 score don't compose). The plan trades depth
    // pagination for a simpler implementation; this test pins that
    // contract so a future change has to deliberately undo it.
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("search-cursor"), TEST_PASSWORD).await;
    insert_post(&db, admin.id, "alpha matches").await;
    insert_post(&db, admin.id, "alpha and beta").await;

    let response = server.get("/api/feed?q=alpha").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(
        json["next_cursor"].is_null(),
        "search responses must not paginate; got {:?}",
        json["next_cursor"]
    );
}
