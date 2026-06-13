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

use common::email_mock::{build_test_email_service, build_test_email_service_err, EmailSpy};
use common::{
    build_test_server_with, create_verified_admin, get_csrf_token, login_as, test_email,
    TestServices, TEST_PASSWORD,
};
use riposte_social::entities::{subscriber, user, Subscriber};
use riposte_social::settings::SettingsService;

use axum::http::StatusCode;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

// ==================== Helpers ====================

async fn seed_site_settings(db: &sea_orm::DatabaseConnection) {
    let settings = SettingsService::new(db.clone());
    settings
        .set("site_name", "Test Site", Some("site"), None)
        .await
        .unwrap();
    settings
        .set("from_email", "noreply@test.example", Some("site"), None)
        .await
        .unwrap();
    settings
        .set("contact_email", "ops@test.example", Some("site"), None)
        .await
        .unwrap();
}

/// Build a test server wired to a mocked `EmailService` backed by the given
/// spy. Also seeds `site_name`, `from_email`, and `contact_email` settings so
/// the email bodies are deterministic.
async fn build_server_with_spy(
    pool: sqlx::PgPool,
    spy: &EmailSpy,
) -> (
    axum_test::TestServer,
    riposte_social::admin::UserAuthBackend,
    sea_orm::DatabaseConnection,
) {
    let db = riposte_social::tests::test_db_from_pool(pool.clone()).await;
    seed_site_settings(&db).await;
    let email = build_test_email_service(spy, &db);
    build_test_server_with(
        pool,
        TestServices {
            email: Some(email),
            ..Default::default()
        },
    )
    .await
}

/// Same as `build_server_with_spy` but the mock SES always fails.
async fn build_server_with_ses_err(
    pool: sqlx::PgPool,
) -> (
    axum_test::TestServer,
    riposte_social::admin::UserAuthBackend,
    sea_orm::DatabaseConnection,
) {
    let db = riposte_social::tests::test_db_from_pool(pool.clone()).await;
    seed_site_settings(&db).await;
    let email = build_test_email_service_err(&db);
    build_test_server_with(
        pool,
        TestServices {
            email: Some(email),
            ..Default::default()
        },
    )
    .await
}

// ==================== Invite email HTML escaping ====================

#[sqlx::test(migrations = false)]
async fn test_invite_email_escapes_html(pool: sqlx::PgPool) {
    let db = riposte_social::tests::test_db_from_pool(pool).await;
    seed_site_settings(&db).await;
    let settings = SettingsService::new(db.clone());
    settings
        .set(
            "site_name",
            "Site <script>alert(1)</script>",
            Some("site"),
            None,
        )
        .await
        .unwrap();

    let spy = EmailSpy::new();
    let email = build_test_email_service(&spy, &db);
    email
        .send_invite_email(
            "invitee@example.com",
            "CODE1234",
            "commenter",
            "evil<img src=x onerror=alert(1)>@example.com",
        )
        .await
        .unwrap();

    let sent = spy.captured().pop().expect("one email sent");
    assert!(
        !sent.html_body.contains("<script>") && !sent.html_body.contains("<img"),
        "raw markup from site_name or inviter_email must not reach the HTML body: {}",
        sent.html_body
    );
    assert!(sent.html_body.contains("&lt;script&gt;"));
    assert!(sent.html_body.contains("&lt;img"));
}

// ==================== Subscribe / confirmation email ====================

#[sqlx::test(migrations = false)]
async fn test_subscribe_new_email_sends_confirmation(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, db) = build_server_with_spy(pool, &spy).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/subscribe")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"email": "new-subscriber@example.test"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["success"].as_bool().unwrap());

    // Spy captured exactly one email addressed to the subscriber.
    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.to, vec!["new-subscriber@example.test".to_string()]);
    assert!(
        sent.subject.to_lowercase().contains("subscription")
            || sent.subject.to_lowercase().contains("confirm"),
        "subject was: {}",
        sent.subject
    );
    // The verification URL should be embedded in the HTML body.
    assert!(
        sent.html_body.contains("/api/subscribe/verify?token="),
        "html body missing verification URL"
    );

    // DB row created with matching token.
    let row = Subscriber::find()
        .filter(subscriber::Column::Email.eq("new-subscriber@example.test"))
        .one(&db)
        .await
        .unwrap()
        .expect("subscriber row");
    let token = row.verification_token.expect("verification token");
    assert!(sent.html_body.contains(&token));
}

#[sqlx::test(migrations = false)]
async fn test_subscribe_resend_for_unverified(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, db) = build_server_with_spy(pool, &spy).await;

    // Pre-insert unverified subscriber with a known token.
    let existing_token = Uuid::new_v4().to_string();
    let now = Utc::now();
    subscriber::ActiveModel {
        id: Set(Uuid::new_v4()),
        email: Set("pending@example.test".to_string()),
        verified: Set(false),
        verification_token: Set(Some(existing_token.clone())),
        verified_at: Set(None),
        active: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    }
    .insert(&db)
    .await
    .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/subscribe")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"email": "pending@example.test"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.to, vec!["pending@example.test".to_string()]);
    // Resend path reuses the existing token.
    assert!(
        sent.html_body.contains(&existing_token),
        "resent email should contain existing token"
    );
}

#[sqlx::test(migrations = false)]
async fn test_subscribe_propagates_ses_error(pool: sqlx::PgPool) {
    let (server, _backend, db) = build_server_with_ses_err(pool).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/subscribe")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"email": "fail-subscriber@example.test"}))
        .await;

    // subscribe() inserts the row, then sends the email. If SES fails the
    // response is 500 but the row remains (the handler currently does not
    // roll back). The important thing for this test is the error is
    // surfaced.
    assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = response.json();
    assert!(!body["success"].as_bool().unwrap());

    // Row WAS created before the email send attempt.
    let row = Subscriber::find()
        .filter(subscriber::Column::Email.eq("fail-subscriber@example.test"))
        .one(&db)
        .await
        .unwrap();
    assert!(row.is_some());
}

// ==================== Contact form ====================

#[sqlx::test(migrations = false)]
async fn test_contact_form_sends_email_with_escaped_html(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, _db) = build_server_with_spy(pool, &spy).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/contact")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "name": "Alice",
            "email": "alice@example.test",
            "subject": "Hi there",
            "message": "<script>alert(1)</script>"
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];

    // Contact form destination is `contact_email` setting.
    assert_eq!(sent.to, vec!["ops@test.example".to_string()]);
    // Sender is `from_email` setting.
    assert_eq!(sent.from, "noreply@test.example");
    // HTML escaping: raw <script> tag must not appear.
    assert!(
        !sent.html_body.contains("<script>alert(1)</script>"),
        "html body should have escaped the script tag"
    );
    assert!(
        sent.html_body.contains("&lt;script&gt;"),
        "html body should contain html-escaped entity, got: {}",
        sent.html_body
    );
}

#[sqlx::test(migrations = false)]
async fn test_contact_form_uses_contact_email_setting(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, _backend, db) = build_server_with_spy(pool, &spy).await;

    // Override contact_email after the defaults seeded by the helper.
    SettingsService::new(db.clone())
        .set("contact_email", "support@acme.test", Some("site"), None)
        .await
        .unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post("/api/contact")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "name": "Bob",
            "email": "bob@example.test",
            "subject": "Question",
            "message": "Hello"
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].to, vec!["support@acme.test".to_string()]);
}

// ==================== Admin user endpoints ====================

#[sqlx::test(migrations = false)]
async fn test_admin_resend_verification_sends_email(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, db) = build_server_with_spy(pool, &spy).await;

    // Acting admin (administrator).
    let actor_email = test_email("email-actor");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    // Target admin to resend verification for.
    let target_email = test_email("email-target");
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;

    // Make the target unverified so resend-verification has a reason to run.
    let mut active: user::ActiveModel = target.clone().into();
    active.email_verified = Set(false);
    active.update(&db).await.unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .post(&format!(
            "/api/admin/users/{}/resend-verification",
            target.id
        ))
        .add_header("x-csrf-token", &csrf)
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.to, vec![target_email.clone()]);
    assert!(sent.html_body.contains("/admin/verify-email?token="));
}

#[sqlx::test(migrations = false)]
async fn test_admin_reactivation_triggers_verification_email(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, db) = build_server_with_spy(pool, &spy).await;

    let actor_email = test_email("reactivate-actor");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let target_email = test_email("reactivate-target");
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;

    // Deactivate the target directly so we can exercise the reactivation branch.
    let mut active: user::ActiveModel = target.clone().into();
    active.active = Set(false);
    active.deactivated_at = Set(Some(Utc::now().into()));
    active.update(&db).await.unwrap();

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"active": true}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.to, vec![target_email.clone()]);
    assert!(sent.html_body.contains("/admin/verify-email?token="));
}

#[sqlx::test(migrations = false)]
async fn test_admin_password_change_notification_sent(pool: sqlx::PgPool) {
    let spy = EmailSpy::new();
    let (server, backend, _db) = build_server_with_spy(pool, &spy).await;

    let actor_email = test_email("pwd-actor");
    create_verified_admin(&backend, &actor_email, TEST_PASSWORD).await;
    login_as(&server, &actor_email, TEST_PASSWORD).await;

    let target_email = test_email("pwd-target");
    let target = create_verified_admin(&backend, &target_email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put(&format!("/api/admin/users/{}", target.id))
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({"new_password": "Diff3rent!Passw0rd99"}))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let captured = spy.captured();
    assert_eq!(captured.len(), 1);
    let sent = &captured[0];
    assert_eq!(sent.to, vec![target_email.clone()]);
    assert!(
        sent.subject.to_lowercase().contains("password"),
        "subject should mention password, got: {}",
        sent.subject
    );
    // The admin-initiated path should include wording indicating an admin made
    // the change (see send_password_changed_notification).
    assert!(
        sent.text_body.to_lowercase().contains("administrator")
            || sent.html_body.to_lowercase().contains("administrator"),
        "body should indicate admin-initiated change"
    );
}
