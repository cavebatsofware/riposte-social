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
//! Tests for the public invite endpoints used by the social-frontend's
//! invite-acceptance flow:
//! - `GET /invite/{code}` (SPA landing that sets the pending_invite cookie)
//! - `GET /api/invites/current` (splash polls this on mount)
//! - `POST /api/auth/logout/invite` (explicit cookie clear)

mod common;

use common::{build_test_server, create_verified_admin, get_csrf_token, test_email, TEST_PASSWORD};

use axum::http::StatusCode;

#[sqlx::test(migrations = false)]
async fn test_invite_landing_does_not_set_cookie(pool: sqlx::PgPool) {
    // The landing page only serves the SPA. The cookie is set later by
    // POST /api/invites/confirm after the visitor accepts the trusted-
    // device + cookie-consent gate. This keeps a visitor on a shared
    // device free of any persisted invite state if they decline.
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("ip-creator"), TEST_PASSWORD).await;
    let (_invite, plaintext) = riposte_social::invites::issue_invite_for_user(&db, admin.id, None)
        .await
        .unwrap();

    let response = server.get(&format!("/invite/{}", plaintext)).await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let pending: Vec<_> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter(|c| c.starts_with("pending_invite="))
        .collect();
    assert!(
        pending.is_empty(),
        "landing must not set cookie before consent, got: {:?}",
        pending
    );
}

#[sqlx::test(migrations = false)]
async fn test_invite_landing_serves_spa_for_invalid_code(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server
        .get("/invite/this-code-does-not-exist-anywhere")
        .await;
    // Always serves the SPA so React Router can render the gate; the SPA
    // posts to /api/invites/confirm which returns null for an invalid code.
    assert_eq!(response.status_code(), StatusCode::OK);
    let pending: Vec<_> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter(|c| c.starts_with("pending_invite="))
        .collect();
    assert!(pending.is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_confirm_invite_sets_cookie_for_live_code(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin =
        create_verified_admin(&backend, &test_email("ip-confirm-admin"), TEST_PASSWORD).await;
    let (_invite, plaintext) = riposte_social::invites::issue_invite_for_user(
        &db,
        admin.id,
        Some("recipient@example.com".to_string()),
    )
    .await
    .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"code": plaintext}))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // Body echoes plaintext + email_hint + expires_at.
    let json: serde_json::Value = response.json();
    assert_eq!(json["code"].as_str().unwrap(), plaintext);
    assert_eq!(
        json["email_hint"].as_str().unwrap(),
        "recipient@example.com"
    );

    // Cookie was set on the response.
    let pending: String = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with("pending_invite="))
        .map(|c| c.to_string())
        .expect("pending_invite cookie set on confirm response");
    assert!(pending.contains(&plaintext));
}

#[sqlx::test(migrations = false)]
async fn test_confirm_invite_returns_null_for_invalid_code(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"code": "no-such-code"}))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.text().trim(), "null");

    // No cookie set on null path.
    let pending: Vec<_> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter(|c| c.starts_with("pending_invite="))
        .collect();
    assert!(pending.is_empty());
}

#[sqlx::test(migrations = false)]
async fn test_current_invite_returns_null_without_cookie(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;
    let response = server.get("/api/invites/current").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = response.text();
    // serde serializes None as JSON null.
    assert_eq!(body.trim(), "null");
}

#[sqlx::test(migrations = false)]
async fn test_current_invite_returns_details_after_confirm(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin =
        create_verified_admin(&backend, &test_email("ip-current-admin"), TEST_PASSWORD).await;
    let (_invite, plaintext) = riposte_social::invites::issue_invite_for_user(
        &db,
        admin.id,
        Some("recipient@example.com".to_string()),
    )
    .await
    .unwrap();

    // Confirm sets the cookie; TestServer carries it to /api/invites/current.
    let csrf = get_csrf_token(&server).await;
    let confirm = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"code": plaintext}))
        .await;
    assert_eq!(confirm.status_code(), StatusCode::OK);

    let response = server.get("/api/invites/current").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert_eq!(json["code"].as_str().unwrap(), plaintext);
    assert_eq!(
        json["email_hint"].as_str().unwrap(),
        "recipient@example.com"
    );
    assert!(json["expires_at"].is_string());
}

#[sqlx::test(migrations = false)]
async fn test_current_invite_returns_null_for_revoked_code(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin =
        create_verified_admin(&backend, &test_email("ip-revoked-admin"), TEST_PASSWORD).await;
    let (invite, plaintext) = riposte_social::invites::issue_invite_for_user(&db, admin.id, None)
        .await
        .unwrap();

    // Confirm to plant the cookie.
    let csrf = get_csrf_token(&server).await;
    let confirm = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"code": plaintext}))
        .await;
    assert_eq!(confirm.status_code(), StatusCode::OK);

    // Revoke the invite directly in DB.
    use riposte_social::entities::invite_code;
    use sea_orm::{ActiveModelTrait, Set};
    let mut active: invite_code::ActiveModel = invite.into();
    active.revoked_at = Set(Some(chrono::Utc::now().into()));
    active.update(&db).await.unwrap();

    let response = server.get("/api/invites/current").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.text().trim(), "null");
}

#[sqlx::test(migrations = false)]
async fn test_logout_invite_clears_cookie(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin =
        create_verified_admin(&backend, &test_email("ip-logout-admin"), TEST_PASSWORD).await;
    let (_invite, plaintext) = riposte_social::invites::issue_invite_for_user(&db, admin.id, None)
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    // Set the cookie via confirm.
    let confirm = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"code": plaintext}))
        .await;
    assert_eq!(confirm.status_code(), StatusCode::OK);

    // Verify it's live.
    let pre = server.get("/api/invites/current").await;
    assert_ne!(pre.text().trim(), "null");

    // Clear it.
    let clear = server
        .post("/api/auth/logout/invite")
        .add_header("x-csrf-token", &csrf)
        .await;
    assert_eq!(clear.status_code(), StatusCode::NO_CONTENT);

    // Cookie cleared. /api/invites/current returns null.
    let post = server.get("/api/invites/current").await;
    assert_eq!(post.text().trim(), "null");
}
