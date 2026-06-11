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
//! Tests for the email transports: SendGrid request shape against a wiremock
//! server, the SES transport against a mocked SDK client, and the
//! per-send provider selection of `SelectingTransport`.

use std::sync::{Arc, Mutex};

use aws_sdk_sesv2::operation::send_email::{SendEmailInput, SendEmailOutput};
use aws_smithy_mocks::{mock, mock_client, RuleMode};
use riposte_social::email::{
    EmailMessage, EmailTransport, SelectingTransport, SendGridTransport, SesTransport,
};
use riposte_social::settings::SettingsService;
use riposte_social::tests::test_db_from_pool;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_message() -> EmailMessage {
    EmailMessage {
        from: "noreply@example.com".to_string(),
        to: "user@example.com".to_string(),
        subject: "Test subject".to_string(),
        html_body: "<p>Hello</p>".to_string(),
        text_body: "Hello".to_string(),
    }
}

/// SES `send_email` inputs captured by the mocked SDK client.
type SesCaptures = Arc<Mutex<Vec<SendEmailInput>>>;

fn mock_ses_client(captures: &SesCaptures) -> aws_sdk_sesv2::Client {
    let captures = captures.clone();
    let rule = mock!(aws_sdk_sesv2::Client::send_email).then_compute_output(
        move |input: &SendEmailInput| {
            captures.lock().unwrap().push(input.clone());
            SendEmailOutput::builder()
                .message_id("mock-message-id")
                .build()
        },
    );
    mock_client!(aws_sdk_sesv2, RuleMode::MatchAny, [&rule])
}

async fn mount_sendgrid_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v3/mail/send"))
        .and(header("authorization", "Bearer test-key"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(202))
        .mount(server)
        .await;
}

// ==================== SendGrid transport ====================

#[tokio::test]
async fn sendgrid_transport_sends_v3_mail_send_request() {
    let server = MockServer::start().await;
    mount_sendgrid_ok(&server).await;

    let transport =
        SendGridTransport::new(reqwest::Client::new(), "test-key".to_string(), server.uri());
    transport.send(&sample_message()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

    assert_eq!(
        body["personalizations"][0]["to"][0]["email"],
        "user@example.com"
    );
    assert_eq!(body["from"]["email"], "noreply@example.com");
    assert_eq!(body["subject"], "Test subject");
    // SendGrid requires text/plain before text/html in `content`.
    assert_eq!(body["content"][0]["type"], "text/plain");
    assert_eq!(body["content"][0]["value"], "Hello");
    assert_eq!(body["content"][1]["type"], "text/html");
    assert_eq!(body["content"][1]["value"], "<p>Hello</p>");
}

#[tokio::test]
async fn sendgrid_transport_error_includes_status_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v3/mail/send"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string(r#"{"errors":[{"message":"bad key"}]}"#),
        )
        .mount(&server)
        .await;

    let transport =
        SendGridTransport::new(reqwest::Client::new(), "test-key".to_string(), server.uri());
    let err = transport.send(&sample_message()).await.unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("401"), "should include status: {msg}");
    assert!(msg.contains("bad key"), "should include body: {msg}");
}

// ==================== SES transport ====================

#[tokio::test]
async fn ses_transport_sends_via_sdk_client() {
    let captures: SesCaptures = Arc::default();
    let transport = SesTransport::new(mock_ses_client(&captures));

    transport.send(&sample_message()).await.unwrap();

    let captured = captures.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let input = &captured[0];
    assert_eq!(input.from_email_address(), Some("noreply@example.com"));
    assert_eq!(
        input.destination().unwrap().to_addresses(),
        ["user@example.com".to_string()]
    );
    let message = input.content().unwrap().simple().unwrap();
    assert_eq!(message.subject().unwrap().data(), "Test subject");
    let body = message.body().unwrap();
    assert_eq!(body.html().unwrap().data(), "<p>Hello</p>");
    assert_eq!(body.text().unwrap().data(), "Hello");
}

// ==================== Provider selection ====================

fn selecting_transport(
    captures: &SesCaptures,
    sendgrid_base_url: String,
    settings: SettingsService,
) -> SelectingTransport {
    SelectingTransport::new(
        SesTransport::new(mock_ses_client(captures)),
        reqwest::Client::new(),
        sendgrid_base_url,
        settings,
    )
}

#[sqlx::test(migrations = false)]
async fn selecting_transport_defaults_to_ses(pool: sqlx::PgPool) {
    dotenvy::dotenv().ok();
    let db = test_db_from_pool(pool).await;
    let settings = SettingsService::new(db);

    let captures: SesCaptures = Arc::default();
    let transport = selecting_transport(&captures, "http://unused.invalid".to_string(), settings);

    transport.send(&sample_message()).await.unwrap();

    assert_eq!(captures.lock().unwrap().len(), 1);
}

#[sqlx::test(migrations = false)]
async fn selecting_transport_uses_sendgrid_when_selected(pool: sqlx::PgPool) {
    dotenvy::dotenv().ok();
    let db = test_db_from_pool(pool).await;
    let settings = SettingsService::new(db);
    settings
        .set("email_provider", "sendgrid", Some("email"), None)
        .await
        .unwrap();
    settings
        .set_encrypted("secret_sendgrid_api_key", "test-key", Some("email"), None)
        .await
        .unwrap();

    let server = MockServer::start().await;
    mount_sendgrid_ok(&server).await;

    let captures: SesCaptures = Arc::default();
    let transport = selecting_transport(&captures, server.uri(), settings);

    transport.send(&sample_message()).await.unwrap();

    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(captures.lock().unwrap().len(), 0, "SES must not be used");
}

#[sqlx::test(migrations = false)]
async fn selecting_transport_errors_when_sendgrid_key_missing(pool: sqlx::PgPool) {
    dotenvy::dotenv().ok();
    let db = test_db_from_pool(pool).await;
    let settings = SettingsService::new(db);
    // The seed migration leaves secret_sendgrid_api_key as an empty
    // encrypted placeholder; only the provider is switched here.
    settings
        .set("email_provider", "sendgrid", Some("email"), None)
        .await
        .unwrap();

    let captures: SesCaptures = Arc::default();
    let transport = selecting_transport(&captures, "http://unused.invalid".to_string(), settings);

    let err = transport.send(&sample_message()).await.unwrap_err();
    assert!(
        err.to_string().contains("secret_sendgrid_api_key"),
        "error should name the missing setting: {err}"
    );
}

#[sqlx::test(migrations = false)]
async fn selecting_transport_rejects_unknown_provider(pool: sqlx::PgPool) {
    dotenvy::dotenv().ok();
    let db = test_db_from_pool(pool).await;
    let settings = SettingsService::new(db);
    settings
        .set("email_provider", "smoke-signals", Some("email"), None)
        .await
        .unwrap();

    let captures: SesCaptures = Arc::default();
    let transport = selecting_transport(&captures, "http://unused.invalid".to_string(), settings);

    let err = transport.send(&sample_message()).await.unwrap_err();
    assert!(
        err.to_string().contains("smoke-signals"),
        "error should name the bad value: {err}"
    );
}
