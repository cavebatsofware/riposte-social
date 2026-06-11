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
//! Spy transport scaffolding for email-sending tests.
//!
//! Every test that wants to observe outgoing email creates an [`EmailSpy`],
//! builds an `Arc<EmailService>` via [`build_test_email_service`] (which wires
//! a spy [`EmailTransport`] into the service), then plugs it into
//! [`crate::common::TestServices`].

use std::sync::{Arc, Mutex};

use riposte_social::email::{EmailMessage, EmailService, EmailTransport};
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

/// Transport that records every message into an [`EmailSpy`], or fails every
/// send when `fail` is set (for error-propagation tests).
struct SpyTransport {
    spy: EmailSpy,
    fail: bool,
}

#[async_trait::async_trait]
impl EmailTransport for SpyTransport {
    async fn send(&self, msg: &EmailMessage) -> anyhow::Result<()> {
        if self.fail {
            anyhow::bail!("mock rejected");
        }
        self.spy.push(CapturedEmail {
            from: msg.from.clone(),
            to: vec![msg.to.clone()],
            subject: msg.subject.clone(),
            html_body: msg.html_body.clone(),
            text_body: msg.text_body.clone(),
        });
        Ok(())
    }
}

fn build_service(transport: SpyTransport, db: &DatabaseConnection) -> Arc<EmailService> {
    Arc::new(EmailService::with_transport(
        Arc::new(transport),
        SettingsService::new(db.clone()),
        "http://localhost:3000".to_string(),
    ))
}

/// Convenience: build an `Arc<EmailService>` wired to a spy transport that
/// always succeeds. `site_url` defaults to `http://localhost:3000` so
/// existing URL assertions in tests match.
pub fn build_test_email_service(spy: &EmailSpy, db: &DatabaseConnection) -> Arc<EmailService> {
    build_service(
        SpyTransport {
            spy: spy.clone(),
            fail: false,
        },
        db,
    )
}

/// Alias kept from the SES-mock era, when one-shot and any-number-of-sends
/// mocks were distinct; the spy transport handles any number of sends.
pub fn build_test_email_service_any(spy: &EmailSpy, db: &DatabaseConnection) -> Arc<EmailService> {
    build_test_email_service(spy, db)
}

/// Convenience: build an `Arc<EmailService>` that always fails (no spy).
pub fn build_test_email_service_err(db: &DatabaseConnection) -> Arc<EmailService> {
    build_service(
        SpyTransport {
            spy: EmailSpy::new(),
            fail: true,
        },
        db,
    )
}
