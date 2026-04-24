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
//! Mock SES client scaffolding for email-sending tests.
//!
//! Every test that wants to observe outgoing email creates an [`EmailSpy`],
//! builds a mocked `aws_sdk_sesv2::Client` via [`build_test_email_service`],
//! then plugs the resulting `Arc<EmailService>` into
//! [`crate::common::TestServices`].

use std::sync::{Arc, Mutex};

use aws_sdk_sesv2::operation::send_email::{SendEmailInput, SendEmailOutput};
use aws_sdk_sesv2::types::EmailContent;
use aws_sdk_sesv2::Client as SesClient;
use aws_smithy_mocks::{mock, mock_client, RuleMode};
use riposte_social::email::EmailService;
use riposte_social::settings::SettingsService;
use sea_orm::DatabaseConnection;

/// A single captured outbound email.
#[derive(Debug, Clone)]
pub struct CapturedEmail {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

/// Thread-safe handle tests hold onto to inspect captured emails after the
/// test drives the API.
#[derive(Clone, Default)]
pub struct EmailSpy {
    inner: Arc<Mutex<Vec<CapturedEmail>>>,
}

impl EmailSpy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn captured(&self) -> Vec<CapturedEmail> {
        self.inner.lock().unwrap().clone()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    fn push(&self, email: CapturedEmail) {
        self.inner.lock().unwrap().push(email);
    }
}

fn extract_email(input: &SendEmailInput) -> CapturedEmail {
    let from = input.from_email_address().unwrap_or_default().to_string();
    let to: Vec<String> = input
        .destination()
        .and_then(|d| d.to_addresses.as_ref())
        .cloned()
        .unwrap_or_default();

    let (subject, html_body, text_body) = match input.content() {
        Some(EmailContent { simple: Some(msg), .. }) => {
            let subject = msg
                .subject()
                .map(|c| c.data().to_string())
                .unwrap_or_default();
            let (html, text) = if let Some(body) = msg.body() {
                (
                    body.html().map(|c| c.data().to_string()).unwrap_or_default(),
                    body.text().map(|c| c.data().to_string()).unwrap_or_default(),
                )
            } else {
                (String::new(), String::new())
            };
            (subject, html, text)
        }
        _ => (String::new(), String::new(), String::new()),
    };

    CapturedEmail {
        from,
        to,
        subject,
        html_body,
        text_body,
    }
}

/// Build a mocked SES client whose `send_email` always succeeds and pushes
/// the captured input into `spy`. Uses `Sequential` rule mode — suitable for
/// tests that send exactly one email.
pub fn mock_ses_ok(spy: &EmailSpy) -> SesClient {
    let spy_clone = spy.clone();
    let rule = mock!(SesClient::send_email)
        .then_compute_output(move |input: &SendEmailInput| {
            spy_clone.push(extract_email(input));
            SendEmailOutput::builder().message_id("mock-message-id").build()
        });

    mock_client!(aws_sdk_sesv2, RuleMode::Sequential, [&rule])
}

/// Build a mocked SES client that succeeds for any number of `send_email`
/// calls. Uses `MatchAny` rule mode — suitable for tests that send multiple
/// emails (e.g. registration + password reset).
pub fn mock_ses_ok_any(spy: &EmailSpy) -> SesClient {
    let spy_clone = spy.clone();
    let rule = mock!(SesClient::send_email)
        .then_compute_output(move |input: &SendEmailInput| {
            spy_clone.push(extract_email(input));
            SendEmailOutput::builder().message_id("mock-message-id").build()
        });

    mock_client!(aws_sdk_sesv2, RuleMode::MatchAny, [&rule])
}

/// Convenience: build an `Arc<EmailService>` wired to a successful mock SES
/// client that handles multiple sends, plus the given spy.
pub fn build_test_email_service_any(spy: &EmailSpy, db: &DatabaseConnection) -> Arc<EmailService> {
    let client = mock_ses_ok_any(spy);
    Arc::new(EmailService::with_client(
        client,
        SettingsService::new(db.clone()),
        "http://localhost:3000".to_string(),
    ))
}

/// Build a mocked SES client whose `send_email` always fails with a generic
/// service error. Useful for testing error propagation paths.
pub fn mock_ses_err() -> SesClient {
    use aws_sdk_sesv2::operation::send_email::SendEmailError;
    use aws_sdk_sesv2::types::error::MessageRejected;

    let rule = mock!(SesClient::send_email).then_error(|| {
        SendEmailError::MessageRejected(
            MessageRejected::builder()
                .message("mock rejected")
                .build(),
        )
    });

    mock_client!(aws_sdk_sesv2, RuleMode::Sequential, [&rule])
}

/// Convenience: build an `Arc<EmailService>` wired to a successful mock SES
/// client plus the given spy. `site_url` defaults to
/// `http://localhost:3000` so existing URL assertions in tests match.
pub fn build_test_email_service(spy: &EmailSpy, db: &DatabaseConnection) -> Arc<EmailService> {
    let client = mock_ses_ok(spy);
    Arc::new(EmailService::with_client(
        client,
        SettingsService::new(db.clone()),
        "http://localhost:3000".to_string(),
    ))
}

/// Convenience: build an `Arc<EmailService>` that always fails (no spy).
pub fn build_test_email_service_err(db: &DatabaseConnection) -> Arc<EmailService> {
    let client = mock_ses_err();
    Arc::new(EmailService::with_client(
        client,
        SettingsService::new(db.clone()),
        "http://localhost:3000".to_string(),
    ))
}
