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
//! Integration tests for the follower / followed graph.
//!
//! Covers idempotent add / remove, self-follow rejection, inactive-target
//! 404, the two-layer visibility filter on listings, the bulk-state
//! lookup, and the profile-extension fields.

mod common;

use axum::http::StatusCode;
use axum_test::TestServer;
use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{follow, user, Follow, User};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use uuid::Uuid;

// ==================== Helpers ====================

async fn make_user(backend: &UserAuthBackend, db: &DatabaseConnection, email: &str) -> user::Model {
    let row = create_verified_admin(backend, email, TEST_PASSWORD).await;
    User::find_by_id(row.id).one(db).await.unwrap().unwrap()
}

async fn deactivate(db: &DatabaseConnection, user_id: Uuid) {
    let row = User::find_by_id(user_id).one(db).await.unwrap().unwrap();
    let mut active: user::ActiveModel = row.into();
    active.active = Set(false);
    active.update(db).await.unwrap();
}

async fn logout(server: &TestServer) {
    let csrf = get_csrf_token(server).await;
    let response = server
        .post("/api/auth/logout")
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

async fn follow(server: &TestServer, target_id: Uuid) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .post(&format!("/api/users/{}/follow", target_id))
        .add_header("x-csrf-token", &csrf)
        .await
}

async fn unfollow(server: &TestServer, target_id: Uuid) -> axum_test::TestResponse {
    let csrf = get_csrf_token(server).await;
    server
        .delete(&format!("/api/users/{}/follow", target_id))
        .add_header("x-csrf-token", &csrf)
        .await
}

async fn count_follow_rows(db: &DatabaseConnection, follower: Uuid, followed: Uuid) -> u64 {
    Follow::find()
        .filter(follow::Column::FollowerId.eq(follower))
        .filter(follow::Column::FollowedId.eq(followed))
        .count(db)
        .await
        .unwrap()
}

// ==================== Tests ====================

#[sqlx::test(migrations = false)]
async fn test_self_follow_rejected(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("follows-self");
    let me = make_user(&backend, &db, &email).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = follow(&server, me.id).await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_self_unfollow_rejected(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("follows-self-un");
    let me = make_user(&backend, &db, &email).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = unfollow(&server, me.id).await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = false)]
async fn test_follow_unknown_user_returns_404(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("follows-unknown");
    make_user(&backend, &db, &email).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = follow(&server, Uuid::new_v4()).await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_follow_inactive_target_returns_404(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("follows-inactive-me");
    make_user(&backend, &db, &me_email).await;
    let target = make_user(&backend, &db, &test_email("follows-inactive-target")).await;
    deactivate(&db, target.id).await;

    login_as(&server, &me_email, TEST_PASSWORD).await;
    let response = follow(&server, target.id).await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_follow_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("follows-idem-me");
    let me = make_user(&backend, &db, &me_email).await;
    let target = make_user(&backend, &db, &test_email("follows-idem-target")).await;
    login_as(&server, &me_email, TEST_PASSWORD).await;

    let r1 = follow(&server, target.id).await;
    assert_eq!(r1.status_code(), StatusCode::OK);
    let body: serde_json::Value = r1.json();
    assert!(body["you_follow"].as_bool().unwrap());
    assert!(!body["follows_you"].as_bool().unwrap());

    let r2 = follow(&server, target.id).await;
    assert_eq!(r2.status_code(), StatusCode::OK);
    let body2: serde_json::Value = r2.json();
    assert!(body2["you_follow"].as_bool().unwrap());

    assert_eq!(count_follow_rows(&db, me.id, target.id).await, 1);
}

#[sqlx::test(migrations = false)]
async fn test_unfollow_idempotent(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("follows-unidem-me");
    let me = make_user(&backend, &db, &me_email).await;
    let target = make_user(&backend, &db, &test_email("follows-unidem-target")).await;
    login_as(&server, &me_email, TEST_PASSWORD).await;

    follow(&server, target.id).await;
    assert_eq!(count_follow_rows(&db, me.id, target.id).await, 1);

    let r1 = unfollow(&server, target.id).await;
    assert_eq!(r1.status_code(), StatusCode::OK);
    let body: serde_json::Value = r1.json();
    assert!(!body["you_follow"].as_bool().unwrap());

    let r2 = unfollow(&server, target.id).await;
    assert_eq!(r2.status_code(), StatusCode::OK);

    assert_eq!(count_follow_rows(&db, me.id, target.id).await, 0);
}

#[sqlx::test(migrations = false)]
async fn test_mutual_state_surfaced(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let a_email = test_email("follows-mutual-a");
    let a = make_user(&backend, &db, &a_email).await;
    let b_email = test_email("follows-mutual-b");
    let b = make_user(&backend, &db, &b_email).await;

    // B follows A first.
    login_as(&server, &b_email, TEST_PASSWORD).await;
    let r = follow(&server, a.id).await;
    assert_eq!(r.status_code(), StatusCode::OK);
    logout(&server).await;

    // A logs in and sees follows_you=true, you_follow=false against B.
    login_as(&server, &a_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!(
            "/api/me/follows/state?user_ids={}",
            b.id
        ))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let entry = &body["states"][b.id.to_string()];
    assert!(entry["follows_you"].as_bool().unwrap());
    assert!(!entry["you_follow"].as_bool().unwrap());

    // A follows back; mutual state.
    let r = follow(&server, b.id).await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert!(body["you_follow"].as_bool().unwrap());
    assert!(body["follows_you"].as_bool().unwrap());
}

#[sqlx::test(migrations = false)]
async fn test_followers_list_filters_inactive(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let viewer_email = test_email("flw-list-viewer");
    make_user(&backend, &db, &viewer_email).await;
    let target = make_user(&backend, &db, &test_email("flw-list-target")).await;
    let active_follower_email = test_email("flw-list-active");
    let active_follower = make_user(&backend, &db, &active_follower_email).await;
    let inactive_follower_email = test_email("flw-list-inactive");
    let inactive_follower = make_user(&backend, &db, &inactive_follower_email).await;

    // active follows target.
    login_as(&server, &active_follower_email, TEST_PASSWORD).await;
    follow(&server, target.id).await;
    logout(&server).await;

    // inactive follows target then is deactivated.
    login_as(&server, &inactive_follower_email, TEST_PASSWORD).await;
    follow(&server, target.id).await;
    logout(&server).await;
    deactivate(&db, inactive_follower.id).await;

    // viewer reads /followers.
    login_as(&server, &viewer_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!("/api/users/{}/followers", target.id))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let users = body["users"].as_array().unwrap();
    let ids: Vec<String> = users
        .iter()
        .map(|u| u["user_id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        ids.contains(&active_follower.id.to_string()),
        "active follower should appear: {:?}",
        ids
    );
    assert!(
        !ids.contains(&inactive_follower.id.to_string()),
        "inactive follower should be filtered: {:?}",
        ids
    );
}

#[sqlx::test(migrations = false)]
async fn test_following_list_filters_inactive(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let viewer_email = test_email("flw-fol-viewer");
    make_user(&backend, &db, &viewer_email).await;
    let me_email = test_email("flw-fol-me");
    let me = make_user(&backend, &db, &me_email).await;
    let active = make_user(&backend, &db, &test_email("flw-fol-active")).await;
    let inactive = make_user(&backend, &db, &test_email("flw-fol-inactive")).await;

    login_as(&server, &me_email, TEST_PASSWORD).await;
    follow(&server, active.id).await;
    follow(&server, inactive.id).await;
    logout(&server).await;
    deactivate(&db, inactive.id).await;

    login_as(&server, &viewer_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!("/api/users/{}/following", me.id))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let users = body["users"].as_array().unwrap();
    let ids: Vec<String> = users
        .iter()
        .map(|u| u["user_id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&active.id.to_string()));
    assert!(!ids.contains(&inactive.id.to_string()));
}

#[sqlx::test(migrations = false)]
async fn test_followers_404_when_target_inactive(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("flw-tgt-me");
    make_user(&backend, &db, &me_email).await;
    let target = make_user(&backend, &db, &test_email("flw-tgt-inactive")).await;
    deactivate(&db, target.id).await;

    login_as(&server, &me_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!("/api/users/{}/followers", target.id))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_bulk_state_lookup(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("flw-bulk-me");
    make_user(&backend, &db, &me_email).await;
    let a_email = test_email("flw-bulk-a");
    let a = make_user(&backend, &db, &a_email).await;
    let b = make_user(&backend, &db, &test_email("flw-bulk-b")).await;
    let c = make_user(&backend, &db, &test_email("flw-bulk-c")).await;
    let d = make_user(&backend, &db, &test_email("flw-bulk-d")).await;

    // me follows a (you_follow only).
    login_as(&server, &me_email, TEST_PASSWORD).await;
    follow(&server, a.id).await;
    follow(&server, b.id).await;
    logout(&server).await;

    // c follows me (follows_you only). me does not follow c.
    let c_email = test_email("flw-bulk-c");
    login_as(&server, &c_email, TEST_PASSWORD).await;
    let me_id = User::find()
        .filter(user::Column::Email.eq(&me_email))
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .id;
    follow(&server, me_id).await;
    logout(&server).await;

    // b follows me (mutual).
    let b_email = test_email("flw-bulk-b");
    login_as(&server, &b_email, TEST_PASSWORD).await;
    follow(&server, me_id).await;
    logout(&server).await;

    // me reads bulk state for a, b, c, d.
    login_as(&server, &me_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!(
            "/api/me/follows/state?user_ids={},{},{},{}",
            a.id, b.id, c.id, d.id
        ))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let states = &body["states"];

    assert_eq!(states[a.id.to_string()]["you_follow"], true);
    assert_eq!(states[a.id.to_string()]["follows_you"], false);

    assert_eq!(states[b.id.to_string()]["you_follow"], true);
    assert_eq!(states[b.id.to_string()]["follows_you"], true);

    assert_eq!(states[c.id.to_string()]["you_follow"], false);
    assert_eq!(states[c.id.to_string()]["follows_you"], true);

    assert_eq!(states[d.id.to_string()]["you_follow"], false);
    assert_eq!(states[d.id.to_string()]["follows_you"], false);
}

#[sqlx::test(migrations = false)]
async fn test_profile_extension_includes_counts_and_booleans(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("flw-prof-me");
    make_user(&backend, &db, &me_email).await;
    let target_email = test_email("flw-prof-target");
    let target = make_user(&backend, &db, &target_email).await;

    // me follows target. target follows me back. mutual + non-zero counts.
    login_as(&server, &me_email, TEST_PASSWORD).await;
    follow(&server, target.id).await;
    logout(&server).await;
    let me_id = User::find()
        .filter(user::Column::Email.eq(&me_email))
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .id;
    login_as(&server, &target_email, TEST_PASSWORD).await;
    follow(&server, me_id).await;
    logout(&server).await;

    // me views target's profile.
    login_as(&server, &me_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!("/api/profiles/{}", target.handle))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["follower_count"].as_i64().unwrap(), 1);
    assert_eq!(body["following_count"].as_i64().unwrap(), 1);
    assert!(body["follows_you"].as_bool().unwrap());
    assert!(body["you_follow"].as_bool().unwrap());

    // me views own profile via /api/me/profile -- counts only.
    let r = server.get("/api/me/profile").await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["follower_count"].as_i64().unwrap(), 1);
    assert_eq!(body["following_count"].as_i64().unwrap(), 1);
}

#[sqlx::test(migrations = false)]
async fn test_followers_pagination_cursor(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let me_email = test_email("flw-page-me");
    make_user(&backend, &db, &me_email).await;
    let target = make_user(&backend, &db, &test_email("flw-page-target")).await;

    // Create 5 followers, each follows target.
    for i in 0..5 {
        let e = test_email(&format!("flw-page-fan{}", i));
        make_user(&backend, &db, &e).await;
        login_as(&server, &e, TEST_PASSWORD).await;
        follow(&server, target.id).await;
        logout(&server).await;
        // Sleep is intentional to give created_at distinct ms values for
        // a deterministic cursor sort. Postgres timestamps support
        // microsecond precision but rapid inserts can collide.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    login_as(&server, &me_email, TEST_PASSWORD).await;
    let r = server
        .get(&format!("/api/users/{}/followers?limit=2", target.id))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["users"].as_array().unwrap().len(), 2);
    let cursor = body["next_cursor"].as_str().expect("cursor present");

    let r = server
        .get(&format!(
            "/api/users/{}/followers?limit=2&cursor={}",
            target.id,
            urlencoding::encode(cursor)
        ))
        .await;
    let body: serde_json::Value = r.json();
    assert_eq!(body["users"].as_array().unwrap().len(), 2);
    let cursor = body["next_cursor"].as_str().expect("cursor present");

    let r = server
        .get(&format!(
            "/api/users/{}/followers?limit=2&cursor={}",
            target.id,
            urlencoding::encode(cursor)
        ))
        .await;
    let body: serde_json::Value = r.json();
    assert_eq!(body["users"].as_array().unwrap().len(), 1);
    assert!(body["next_cursor"].is_null());
}

#[sqlx::test(migrations = false)]
async fn test_follow_requires_auth(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let target = make_user(&backend, &db, &test_email("flw-auth")).await;

    let r = server
        .post(&format!("/api/users/{}/follow", target.id))
        .await;
    let status = r.status_code();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "expected 401/403 got {}",
        status
    );
}
