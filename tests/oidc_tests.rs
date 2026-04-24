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
mod common;

use riposte_social::entities::{admin_user, AdminUser};
use common::oidc_mock::{extract_query_param, OidcMockServer, OidcMockUser, TEST_CLIENT_ID};
use common::{build_test_server, build_test_server_with, TestServices};

use axum::http::StatusCode;
use openidconnect::Nonce;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

// ==================== Helpers ====================

/// Drive the login→callback flow and return the response from the callback.
/// Stages the given user on the mock and calls both /login and /callback.
async fn drive_oidc_flow(
    server: &axum_test::TestServer,
    mock: &OidcMockServer,
    user: OidcMockUser,
    role_claim: &str,
) -> axum_test::TestResponse {
    // 1. Hit /api/admin/oidc/login to initiate the redirect.
    let login_resp = server.get("/api/admin/oidc/login").await;
    assert_eq!(
        login_resp.status_code(),
        StatusCode::TEMPORARY_REDIRECT,
        "login should redirect: {}",
        login_resp.text()
    );

    let location = login_resp
        .headers()
        .get("location")
        .expect("location header")
        .to_str()
        .unwrap()
        .to_string();

    // 2. Extract query params from the redirect URL.
    let state = extract_query_param(&location, "state").expect("state param");
    let nonce_str = extract_query_param(&location, "nonce").expect("nonce param");

    // 3. Stage the mock token with the extracted nonce.
    mock.stage_token(user, Nonce::new(nonce_str), role_claim);

    // 4. Call the callback as if the IdP redirected the user back.
    server
        .get(&format!(
            "/api/admin/oidc/callback?code=fake-auth-code&state={}",
            state
        ))
        .await
}

// ==================== Login redirect ====================

#[sqlx::test(migrations = false)]
async fn test_oidc_login_disabled_returns_error(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/oidc/login").await;

    // With OIDC disabled, authorization_url() returns Err.
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_oidc_login_redirects_to_authorization_endpoint(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, _db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    let response = server.get("/api/admin/oidc/login").await;
    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();

    // Location should point at the mock server's /authorize endpoint.
    assert!(
        location.contains(&mock.issuer_url()),
        "redirect should point at mock: {}",
        location
    );
    assert!(location.contains("client_id=test-client"));
    assert!(location.contains("scope=openid"));
    assert!(location.contains("state="));
    assert!(location.contains("nonce="));
    assert!(location.contains("code_challenge="));
}

// ==================== Callback error paths ====================

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_without_prior_login_returns_error(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, _db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    let response = server
        .get("/api/admin/oidc/callback?code=fake&state=fake")
        .await;

    // No session state means the middleware returns 401.
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_state_mismatch_returns_error(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    // Do login to populate session with state.
    let login_resp = server.get("/api/admin/oidc/login").await;
    assert_eq!(login_resp.status_code(), StatusCode::TEMPORARY_REDIRECT);

    // Call callback with a WRONG state value.
    let response = server
        .get("/api/admin/oidc/callback?code=fake&state=WRONG-STATE")
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    // No user created in DB.
    let users = AdminUser::find().all(&db).await.unwrap();
    assert_eq!(users.len(), 0);
}

// ==================== Callback happy path ====================

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_admin_role_creates_administrator(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    let user = OidcMockUser::new("admin@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(location, "/admin");

    let row = AdminUser::find()
        .filter(admin_user::Column::Email.eq("admin@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("admin_user row");
    assert_eq!(row.role, "administrator");
    assert!(row.email_verified);
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_non_admin_role_creates_viewer(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    let user = OidcMockUser::new("viewer@keycloak.test", vec!["user", "reader"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = AdminUser::find()
        .filter(admin_user::Column::Email.eq("viewer@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("admin_user row");
    assert_eq!(row.role, "viewer");
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_existing_user_updates_role(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    // Pre-insert user as viewer.
    let now = chrono::Utc::now();
    admin_user::ActiveModel {
        id: Set(uuid::Uuid::new_v4()),
        email: Set("upgrade@keycloak.test".to_string()),
        password_hash: Set("oidc_user_fake".to_string()),
        email_verified: Set(true),
        verification_token: Set(None),
        verification_token_expires_at: Set(None),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        totp_secret: Set(None),
        totp_enabled: Set(Some(false)),
        totp_enabled_at: Set(None),
        mfa_failed_attempts: Set(Some(0)),
        mfa_locked_until: Set(None),
        active: Set(true),
        deactivated_at: Set(None),
        force_password_change: Set(false),
        password_reset_token: Set(None),
        password_reset_token_expires_at: Set(None),
        role: Set("viewer".to_string()),
    }
    .insert(&db)
    .await
    .unwrap();

    let user = OidcMockUser::new("upgrade@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = AdminUser::find()
        .filter(admin_user::Column::Email.eq("upgrade@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("admin_user row");
    assert_eq!(row.role, "administrator");
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_session_marked_mfa_verified(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, _db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    let user = OidcMockUser::new("mfa-check@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;
    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    // After OIDC login the session should have mfa_verified=true. Confirm by
    // hitting an admin endpoint that requires auth — if it returns 200 (not
    // 403 "MFA verification required") then the session is fully authorized.
    let admin_resp = server.get("/api/admin/users").await;
    assert_eq!(
        admin_resp.status_code(),
        StatusCode::OK,
        "admin route should be accessible post-OIDC login: {}",
        admin_resp.text()
    );
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_role_claim_from_access_token_fallback(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock.build_oidc_service(TEST_CLIENT_ID).await;
    let (server, _backend, db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    // Stage user with NO roles in the ID token, but roles in the access token.
    let user = OidcMockUser::new("access-tok@keycloak.test", vec![])
        .with_access_token_roles(vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = AdminUser::find()
        .filter(admin_user::Column::Email.eq("access-tok@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("admin_user row");
    assert_eq!(row.role, "administrator");
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_custom_role_claim_path(pool: sqlx::PgPool) {
    let mock = OidcMockServer::start().await;
    let oidc = mock
        .build_oidc_service_with_role_claim(TEST_CLIENT_ID, "custom.nested.roles")
        .await;
    let (server, _backend, db) = build_test_server_with(
        pool,
        TestServices {
            oidc: Some(oidc),
            ..Default::default()
        },
    )
    .await;

    let user = OidcMockUser::new("custom-claim@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "custom.nested.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = AdminUser::find()
        .filter(admin_user::Column::Email.eq("custom-claim@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("admin_user row");
    assert_eq!(row.role, "administrator");
}
