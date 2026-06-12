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
use aws_sdk_sesv2::{
    types::{Body, Content, Destination, EmailContent, Message},
    Client as SesClient,
};

use super::{EmailMessage, EmailTransport};

/// Sends mail through Amazon SES v2.
pub struct SesTransport {
    client: SesClient,
}

impl SesTransport {
    pub fn new(client: SesClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl EmailTransport for SesTransport {
    async fn send(&self, msg: &EmailMessage) -> Result<()> {
        let destination = Destination::builder().to_addresses(&msg.to).build();
        let subject = Content::builder()
            .data(&msg.subject)
            .charset("UTF-8")
            .build()?;
        let html = Content::builder()
            .data(&msg.html_body)
            .charset("UTF-8")
            .build()?;
        let text = Content::builder()
            .data(&msg.text_body)
            .charset("UTF-8")
            .build()?;
        let body = Body::builder().html(html).text(text).build();
        let message = Message::builder().subject(subject).body(body).build();
        let content = EmailContent::builder().simple(message).build();

        self.client
            .send_email()
            .from_email_address(&msg.from)
            .destination(destination)
            .content(content)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("SES send_email: {}", crate::s3::format_aws_error(&e)))?;
        Ok(())
    }
}
