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

use common::oidc_mock::{extract_query_param, OidcMockServer, OidcMockUser, TEST_CLIENT_ID};
use common::{build_test_server, build_test_server_with, TestServices};
use riposte_social::entities::{invite_code, user, User};

use axum::http::StatusCode;
use openidconnect::Nonce;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

/// Pre-provision an inert user row for OIDC-bind tests (Flow A.1). The row
/// has no `oidc_sub` and `activated_at = NULL`; it cannot establish a session
/// until a valid invite is presented through `/invite/{code}`. Pass the
/// `invite_code_id` of the corresponding invite (production sets this at
/// user creation time; tests must mirror that).
async fn preprovision_user(
    db: &DatabaseConnection,
    email: &str,
    role: &str,
    invite_code_id: Option<uuid::Uuid>,
) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    user::ActiveModel {
        id: Set(id),
        email: Set(email.to_string()),
        password_hash: Set(format!("invite_pending_{}", uuid::Uuid::new_v4())),
        email_verified: Set(false),
        verification_token: Set(None),
        verification_token_expires_at: Set(None),
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
        role: Set(role.to_string()),
        oidc_sub: Set(None),
        display_name: Set(None),
        avatar_url: Set(None),
        last_login_at: Set(None),
        invite_code_id: Set(invite_code_id),
        activated_at: Set(None),
        handle: Set(format!("test-{}", &id.to_string()[..8])),
        bio: Set(None),
        pronouns: Set(None),
        avatar_s3_key: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    id
}

/// Insert a fully-active OIDC-bound user row for normal-login (Flow B) tests.
/// `oidc_sub` is set, `activated_at` is set; the row should be able to log
/// in via OIDC immediately.
async fn insert_active_oidc_user(
    db: &DatabaseConnection,
    email: &str,
    role: &str,
    oidc_sub: &str,
) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    user::ActiveModel {
        id: Set(id),
        email: Set(email.to_string()),
        password_hash: Set(format!("oidc_user_{}", uuid::Uuid::new_v4())),
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
        role: Set(role.to_string()),
        oidc_sub: Set(Some(oidc_sub.to_string())),
        display_name: Set(None),
        avatar_url: Set(None),
        last_login_at: Set(None),
        invite_code_id: Set(None),
        activated_at: Set(Some(now.into())),
        handle: Set(format!("test-{}", &id.to_string()[..8])),
        bio: Set(None),
        pronouns: Set(None),
        avatar_s3_key: Set(None),
        locale: Set(None),
    }
    .insert(db)
    .await
    .unwrap();
    id
}

/// Stand up a fresh invite + drop the `pending_invite` cookie via
/// `POST /api/invites/confirm` (mirrors the SPA's flow after the visitor
/// accepts the trusted-device + cookie-consent gate). Returns
/// `(Model, plaintext)`.
async fn setup_pending_invite(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
    creator: uuid::Uuid,
    email_hint: Option<&str>,
) -> (invite_code::Model, String) {
    let (invite, plaintext) = issue_invite(db, creator, email_hint).await;
    confirm_invite_via_api(server, &plaintext).await;
    (invite, plaintext)
}

/// Mirror the production POST /api/admin/users flow: issue the invite first,
/// then pre-provision an inert user row with `invite_code_id` pre-stamped.
/// Confirms the invite via the API so the cookie is set for the subsequent
/// OIDC drive. Returns `(Model, plaintext)` for assertions and follow-up.
async fn pre_provision_with_invite(
    server: &axum_test::TestServer,
    db: &DatabaseConnection,
    creator: uuid::Uuid,
    email: &str,
    role: &str,
) -> (invite_code::Model, String) {
    let (invite, plaintext) = issue_invite(db, creator, Some(email)).await;
    preprovision_user(db, email, role, Some(invite.id)).await;
    confirm_invite_via_api(server, &plaintext).await;
    (invite, plaintext)
}

async fn confirm_invite_via_api(server: &axum_test::TestServer, plaintext: &str) {
    let csrf = common::get_csrf_token(server).await;
    let response = server
        .post("/api/invites/confirm")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"code": plaintext}))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "confirm should accept live invite, body: {}",
        response.text()
    );
}

/// Issue an invite via the production helper so it gets hashed at rest.
/// Returns the row plus the plaintext code (only place plaintext exists).
async fn issue_invite(
    db: &DatabaseConnection,
    creator: uuid::Uuid,
    email_hint: Option<&str>,
) -> (invite_code::Model, String) {
    riposte_social::invites::issue_invite_for_user(db, creator, email_hint.map(String::from))
        .await
        .unwrap()
}

// ==================== Helpers ====================

/// Drive the login→callback flow and return the response from the callback.
/// Stages the given user on the mock and calls both /login and /callback.
async fn drive_oidc_flow(
    server: &axum_test::TestServer,
    mock: &OidcMockServer,
    user: OidcMockUser,
    role_claim: &str,
) -> axum_test::TestResponse {
    // 1. Hit /api/auth/oidc/login to initiate the redirect.
    let login_resp = server.get("/api/auth/oidc/login").await;
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
            "/api/auth/oidc/callback?code=fake-auth-code&state={}",
            state
        ))
        .await
}

// ==================== Login redirect ====================

#[sqlx::test(migrations = false)]
async fn test_oidc_login_disabled_returns_error(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/auth/oidc/login").await;

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

    let response = server.get("/api/auth/oidc/login").await;
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
        .get("/api/auth/oidc/callback?code=fake&state=fake")
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
    let login_resp = server.get("/api/auth/oidc/login").await;
    assert_eq!(login_resp.status_code(), StatusCode::TEMPORARY_REDIRECT);

    // Call callback with a WRONG state value.
    let response = server
        .get("/api/auth/oidc/callback?code=fake&state=WRONG-STATE")
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    // No user created in DB.
    let users = User::find().all(&db).await.unwrap();
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

    // Flow A.1: admin row is pre-provisioned (inert) with the invite_code_id
    // pre-stamped, mirroring what POST /api/admin/users does in production.
    // The OIDC callback then binds the row by looking it up via that id.
    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    let (invite, _plaintext) = pre_provision_with_invite(
        &server,
        &db,
        creator_id,
        "admin@keycloak.test",
        "administrator",
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

    let row = User::find()
        .filter(user::Column::Email.eq("admin@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("user row");
    assert_eq!(row.role, "administrator");
    assert!(row.email_verified);
    assert!(
        row.oidc_sub.is_some(),
        "row should be linked to an OIDC sub"
    );
    assert!(row.activated_at.is_some(), "bind should activate the row");
    assert_eq!(row.invite_code_id, Some(invite.id));
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_non_admin_role_creates_commenter(pool: sqlx::PgPool) {
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

    // Flow A.2: no row exists for the IdP-attested email; an invite without
    // an email_hint mints a fresh commenter on acceptance.
    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    let (invite, _plaintext) = setup_pending_invite(&server, &db, creator_id, None).await;

    let user = OidcMockUser::new("commenter@keycloak.test", vec!["user", "reader"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = User::find()
        .filter(user::Column::Email.eq("commenter@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("user row");
    assert_eq!(row.role, "commenter");
    assert_eq!(row.invite_code_id, Some(invite.id));

    // Invite is consumed.
    let used = riposte_social::entities::InviteCode::find_by_id(invite.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(used.used_at.is_some());
    assert_eq!(used.used_by_user_id, Some(row.id));
}

/// Drift defense: if the IdP raises a previously-commenter user to admin
/// (or vice versa), the login fails closed rather than silently elevating.
/// Operators reconcile via the admin UI or fix the Keycloak role assignment.
#[sqlx::test(migrations = false)]
async fn test_oidc_normal_login_rejects_role_drift(pool: sqlx::PgPool) {
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

    // Active commenter row with a known oidc_sub. The IdP now claims admin
    // for the same sub; drift must be rejected.
    insert_active_oidc_user(&db, "drift@keycloak.test", "commenter", "drift-sub").await;

    let user = OidcMockUser::with_sub("drift@keycloak.test", vec!["admin"], "drift-sub");
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(
        response.status_code(),
        StatusCode::UNAUTHORIZED,
        "drift should be rejected, got body: {}",
        response.text()
    );

    // Row is unchanged.
    let row = User::find()
        .filter(user::Column::Email.eq("drift@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("user row");
    assert_eq!(row.role, "commenter");
}

#[sqlx::test(migrations = false)]
async fn test_oidc_callback_session_marked_mfa_verified(pool: sqlx::PgPool) {
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

    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    pre_provision_with_invite(
        &server,
        &db,
        creator_id,
        "mfa-check@keycloak.test",
        "administrator",
    )
    .await;

    let user = OidcMockUser::new("mfa-check@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;
    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    // After OIDC login the session should have mfa_verified=true. Confirm by
    // hitting an admin endpoint that requires auth; if it returns 200 (not
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

    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    pre_provision_with_invite(
        &server,
        &db,
        creator_id,
        "access-tok@keycloak.test",
        "administrator",
    )
    .await;

    // Stage user with NO roles in the ID token, but roles in the access token.
    let user = OidcMockUser::new("access-tok@keycloak.test", vec![])
        .with_access_token_roles(vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = User::find()
        .filter(user::Column::Email.eq("access-tok@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("user row");
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

    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    pre_provision_with_invite(
        &server,
        &db,
        creator_id,
        "custom-claim@keycloak.test",
        "administrator",
    )
    .await;

    let user = OidcMockUser::new("custom-claim@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "custom.nested.roles").await;

    assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);

    let row = User::find()
        .filter(user::Column::Email.eq("custom-claim@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("user row");
    assert_eq!(row.role, "administrator");
}

// ==================== invariants ====================

/// Hijack defense: an invite issued for one email cannot bind to a
/// pre-provisioned row with a different email even if the IdP somehow
/// federates that email back. The email_hint enforcement at bind time
/// rejects this attempt.
#[sqlx::test(migrations = false)]
async fn test_oidc_bind_rejects_email_hint_mismatch(pool: sqlx::PgPool) {
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

    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    // Invite issued for Alice and stamped onto Alice's row, but the visitor
    // authenticates as Bob via the IdP.
    pre_provision_with_invite(
        &server,
        &db,
        creator_id,
        "alice@keycloak.test",
        "administrator",
    )
    .await;

    let user = OidcMockUser::new("bob@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    // Alice's row stays inert.
    let alice = User::find()
        .filter(user::Column::Email.eq("alice@keycloak.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("alice row");
    assert!(alice.activated_at.is_none());
    assert!(alice.oidc_sub.is_none());
}

/// Activation gate: a row with activated_at = NULL cannot establish a
/// session even if its oidc_sub somehow matches an IdP claim. (In practice
/// this state is unreachable through normal flows because activation and
/// oidc_sub are set together; but the defense is structural.)
#[sqlx::test(migrations = false)]
async fn test_oidc_inert_row_login_rejected(pool: sqlx::PgPool) {
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

    // Insert an inert row that already has an oidc_sub but no activated_at.
    let now = chrono::Utc::now();
    let inert_id = uuid::Uuid::new_v4();
    user::ActiveModel {
        id: Set(inert_id),
        email: Set("inert@keycloak.test".to_string()),
        password_hash: Set(format!("oidc_user_{}", uuid::Uuid::new_v4())),
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
        role: Set("commenter".to_string()),
        oidc_sub: Set(Some("inert-sub".to_string())),
        display_name: Set(None),
        avatar_url: Set(None),
        last_login_at: Set(None),
        invite_code_id: Set(None),
        activated_at: Set(None),
        handle: Set(format!("inert-{}", &inert_id.to_string()[..8])),
        bio: Set(None),
        pronouns: Set(None),
        avatar_s3_key: Set(None),
        locale: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    let user = OidcMockUser::with_sub("inert@keycloak.test", vec![], "inert-sub");
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

/// Privilege escalation defense: an IdP-claimed admin or poster with no DB
/// row cannot mint themselves an account through the commenter onboarding
/// path. Privileged accounts must be pre-provisioned.
#[sqlx::test(migrations = false)]
async fn test_oidc_a2_rejects_privileged_claim(pool: sqlx::PgPool) {
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

    // Creator + invite (no email_hint = commenter onboarding flow).
    let creator_id = preprovision_user(&db, "creator@keycloak.test", "administrator", None).await;
    setup_pending_invite(&server, &db, creator_id, None).await;

    // IdP claims admin role for a brand-new user.
    let user = OidcMockUser::new("attacker@keycloak.test", vec!["admin"]);
    let response = drive_oidc_flow(&server, &mock, user, "realm_access.roles").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    // No row was created.
    let row = User::find()
        .filter(user::Column::Email.eq("attacker@keycloak.test"))
        .one(&db)
        .await
        .unwrap();
    assert!(row.is_none());
}
