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
use anyhow::Result;
use serde_json::json;

use super::{EmailMessage, EmailTransport};

pub const DEFAULT_BASE_URL: &str = "https://api.sendgrid.com";

/// Sends mail through the SendGrid v3 REST API. `base_url` is injectable so
/// tests can point it at a local mock server.
pub struct SendGridTransport {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl SendGridTransport {
    pub fn new(http: reqwest::Client, api_key: String, base_url: String) -> Self {
        Self {
            http,
            api_key,
            base_url,
        }
    }
}

#[async_trait::async_trait]
impl EmailTransport for SendGridTransport {
    async fn send(&self, msg: &EmailMessage) -> Result<()> {
        // SendGrid requires text/plain before text/html in `content`.
        let body = json!({
            "personalizations": [{ "to": [{ "email": msg.to }] }],
            "from": { "email": msg.from },
            "subject": msg.subject,
            "content": [
                { "type": "text/plain", "value": msg.text_body },
                { "type": "text/html", "value": msg.html_body },
            ],
        });

        let resp = self
            .http
            .post(format!("{}/v3/mail/send", self.base_url))
            .bearer_auth(&self.api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&body)?)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("SendGrid HTTP {}: {}", status, body);
        }
        Ok(())
    }
}
