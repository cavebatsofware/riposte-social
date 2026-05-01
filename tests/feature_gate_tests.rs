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
//! Phase 6 feature gates: backend enforcement + the role-tailored
//! `/api/site/config` payload.
//!
//! Each gate flips a boolean in the `settings` table and asserts the
//! relevant route honors it. The gates default `true` so the negative
//! case requires explicit `set("...", "false")`.

mod common;

use axum::http::StatusCode;
use axum_test::multipart::MultipartForm;
use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};
use riposte_social::admin::UserAuthBackend;
use riposte_social::entities::{post, user, User};
use riposte_social::settings::SettingsService;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

async fn set_role(db: &DatabaseConnection, user_id: Uuid, role: &str) {
    let row = User::find_by_id(user_id).one(db).await.unwrap().unwrap();
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

async fn insert_public_post(db: &DatabaseConnection, author_id: Uuid) -> post::Model {
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set("hello".to_string()),
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

// ===== public_feed_enabled =====

#[sqlx::test(migrations = false)]
async fn test_public_feed_gate_blocks_anon_when_disabled(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin =
        create_verified_admin(&backend, &test_email("fg-pf-a"), TEST_PASSWORD).await;
    let p = insert_public_post(&db, admin.id).await;

    // Default: anon read is allowed.
    assert_eq!(
        server.get("/api/feed").await.status_code(),
        StatusCode::OK
    );

    let settings = SettingsService::new(db.clone());
    settings
        .set("public_feed_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    // Anon: blocked.
    let resp = server.get("/api/feed").await;
    assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    let resp = server.get(&format!("/api/posts/{}", p.id)).await;
    assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_public_feed_gate_lets_authed_through(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("fg-pf-admin");
    let admin = create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;
    insert_public_post(&db, admin.id).await;

    let settings = SettingsService::new(db.clone());
    settings
        .set("public_feed_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    assert_eq!(
        server.get("/api/feed").await.status_code(),
        StatusCode::OK
    );
}

// ===== poster_posting_enabled =====

#[sqlx::test(migrations = false)]
async fn test_poster_posting_gate_blocks_poster_when_disabled(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let poster_email = test_email("fg-pp-poster");
    make_user(&backend, &db, &poster_email, user::ROLE_POSTER).await;

    let settings = SettingsService::new(db.clone());
    settings
        .set("poster_posting_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    login_as(&server, &poster_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            MultipartForm::new()
                .add_text("body", "muted")
                .add_text("visibility", "public"),
        )
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_poster_posting_gate_admin_bypasses(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("fg-pp-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    let settings = SettingsService::new(db.clone());
    settings
        .set("poster_posting_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/posts")
        .add_header("x-csrf-token", &csrf)
        .multipart(
            MultipartForm::new()
                .add_text("body", "admin override")
                .add_text("visibility", "public"),
        )
        .await;
    assert_eq!(response.status_code(), StatusCode::CREATED);
}

// ===== commenter_invites_enabled =====

#[sqlx::test(migrations = false)]
async fn test_commenter_invites_gate_blocks_when_disabled(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("fg-ci-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    let settings = SettingsService::new(db.clone());
    settings
        .set(
            "commenter_invites_enabled",
            "false",
            Some("features"),
            None,
        )
        .await
        .unwrap();

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/admin/invite-codes")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "email_hint": "x@example.com" }))
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_commenter_invites_gate_blocks_password_acceptance(pool: sqlx::PgPool) {
    // Kill-switch semantics: when commenter_invites is off, an existing
    // (un-redeemed) invite cannot be accepted via the password flow.
    // Mirrors the bind-pre-provisioned-admin scenario from
    // tests/invite_acceptance_tests.rs but with the gate flipped off.
    let (server, backend, db) = build_test_server(pool).await;

    // Issue an invite + pre-provision an inert admin row pointing at it.
    let creator =
        create_verified_admin(&backend, &test_email("fg-ci-creator"), TEST_PASSWORD).await;
    let target_email = test_email("fg-ci-target");
    let (invite, plaintext) = riposte_social::invites::issue_invite_for_user(
        &db,
        creator.id,
        Some(target_email.clone()),
    )
    .await
    .unwrap();
    let inert_id = Uuid::new_v4();
    user::ActiveModel {
        id: Set(inert_id),
        email: Set(target_email.clone()),
        password_hash: Set(format!("invite_pending_{}", Uuid::new_v4())),
        email_verified: Set(false),
        active: Set(true),
        role: Set(user::ROLE_ADMINISTRATOR.to_string()),
        invite_code_id: Set(Some(invite.id)),
        activated_at: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let settings = SettingsService::new(db.clone());
    settings
        .set(
            "commenter_invites_enabled",
            "false",
            Some("features"),
            None,
        )
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/auth/invite/accept-password")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "code": plaintext,
            "email": target_email,
            "new_password": "Inv1te!Acc3pt-Pass",
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    // The inert row stays inert; the invite stays unused.
    let row = User::find_by_id(inert_id).one(&db).await.unwrap().unwrap();
    assert!(
        row.activated_at.is_none(),
        "row must remain inert when invite acceptance is gated off"
    );
    let invite_row = riposte_social::entities::InviteCode::find_by_id(invite.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        invite_row.used_at.is_none(),
        "invite must not be marked used when acceptance is gated off"
    );
}

#[sqlx::test(migrations = false)]
async fn test_commenter_invites_gate_blocks_confirm_cookie(pool: sqlx::PgPool) {
    // /api/invites/confirm should refuse to set the pending_invite cookie
    // when the gate is off — return null (same surface as a stale invite).
    let (server, backend, db) = build_test_server(pool).await;
    let creator =
        create_verified_admin(&backend, &test_email("fg-ci-cf-creator"), TEST_PASSWORD).await;
    let (_invite, plaintext) =
        riposte_social::invites::issue_invite_for_user(&db, creator.id, None)
            .await
            .unwrap();

    let settings = SettingsService::new(db.clone());
    settings
        .set(
            "commenter_invites_enabled",
            "false",
            Some("features"),
            None,
        )
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": plaintext }))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(
        json.is_null(),
        "confirm must return null when gate is off, got {}",
        json
    );
}

// ===== fb_import_enabled =====

#[sqlx::test(migrations = false)]
async fn test_fb_import_gate_blocks_when_disabled(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin_email = test_email("fg-fi-admin");
    create_verified_admin(&backend, &admin_email, TEST_PASSWORD).await;

    let settings = SettingsService::new(db.clone());
    settings
        .set("fb_import_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    login_as(&server, &admin_email, TEST_PASSWORD).await;
    let csrf = get_csrf_token(&server).await;
    // Empty multipart is fine: gate fires before the field walk.
    let response = server
        .post("/api/admin/imports/facebook")
        .add_header("x-csrf-token", &csrf)
        .multipart(MultipartForm::new().add_text("visibility", "public"))
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

// ===== /api/site/config role-tailored shape =====

#[sqlx::test(migrations = false)]
async fn test_site_config_anonymous_minimal_payload(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let response = server.get("/api/site/config").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json["site_name"].is_string());
    assert!(json["public_feed_enabled"].is_boolean());
    // Tier-restricted gates must not leak to anon.
    assert!(json.get("poster_posting_enabled").is_none());
    assert!(json.get("commenter_invites_enabled").is_none());
    assert!(json.get("fb_import_enabled").is_none());
}

#[sqlx::test(migrations = false)]
async fn test_site_config_poster_includes_posting_gate(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("fg-sc-poster");
    make_user(&backend, &db, &email, user::ROLE_POSTER).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let json: serde_json::Value = server.get("/api/site/config").await.json();
    assert!(json["poster_posting_enabled"].is_boolean());
    // Posters do not need to know the admin-only gates.
    assert!(json.get("commenter_invites_enabled").is_none());
    assert!(json.get("fb_import_enabled").is_none());
}

#[sqlx::test(migrations = false)]
async fn test_site_config_admin_includes_all_gates(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("fg-sc-admin");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let json: serde_json::Value = server.get("/api/site/config").await.json();
    assert!(json["public_feed_enabled"].is_boolean());
    assert!(json["poster_posting_enabled"].is_boolean());
    assert!(json["commenter_invites_enabled"].is_boolean());
    assert!(json["fb_import_enabled"].is_boolean());
}
