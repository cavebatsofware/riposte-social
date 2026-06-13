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
use anyhow::{Context, Result};
use std::env;
use std::sync::Arc;

use crate::settings::SettingsService;

mod catalog;
mod render;
mod sendgrid;
mod ses;

pub use sendgrid::{SendGridTransport, DEFAULT_BASE_URL as SENDGRID_DEFAULT_BASE_URL};
pub use ses::SesTransport;

/// A fully composed outbound email, ready for any transport.
pub struct EmailMessage {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[async_trait::async_trait]
pub trait EmailTransport: Send + Sync {
    async fn send(&self, msg: &EmailMessage) -> Result<()>;
}

/// Picks the transport per send from the `email_provider` setting ("ses" or
/// "sendgrid"), so an admin can switch providers or rotate the SendGrid key
/// without a restart.
pub struct SelectingTransport {
    ses: SesTransport,
    http: reqwest::Client,
    sendgrid_base_url: String,
    settings: SettingsService,
}

impl SelectingTransport {
    pub fn new(
        ses: SesTransport,
        http: reqwest::Client,
        sendgrid_base_url: String,
        settings: SettingsService,
    ) -> Self {
        Self {
            ses,
            http,
            sendgrid_base_url,
            settings,
        }
    }
}

#[async_trait::async_trait]
impl EmailTransport for SelectingTransport {
    async fn send(&self, msg: &EmailMessage) -> Result<()> {
        match self.settings.get_email_provider().await?.as_str() {
            "ses" => self.ses.send(msg).await,
            "sendgrid" => {
                let api_key = self.settings.get_sendgrid_api_key().await?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "email_provider is sendgrid but secret_sendgrid_api_key is not set"
                    )
                })?;
                SendGridTransport::new(self.http.clone(), api_key, self.sendgrid_base_url.clone())
                    .send(msg)
                    .await
            }
            other => {
                anyhow::bail!("unknown email_provider {other:?}, expected \"ses\" or \"sendgrid\"")
            }
        }
    }
}

#[derive(Clone)]
pub struct EmailService {
    transport: Arc<dyn EmailTransport>,
    settings: SettingsService,
    site_url: String,
}

impl EmailService {
    /// Construct an `EmailService` from a pre-built transport. Used by
    /// `new()` for production and by tests that inject a spy transport.
    pub fn with_transport(
        transport: Arc<dyn EmailTransport>,
        settings: SettingsService,
        site_url: String,
    ) -> Self {
        Self {
            transport,
            settings,
            site_url,
        }
    }

    pub async fn new(settings: SettingsService) -> Result<Self> {
        let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

        // Override region if AWS_REGION is set in environment
        if let Ok(region) = env::var("AWS_REGION") {
            config_loader = config_loader.region(aws_sdk_sesv2::config::Region::new(region));
        }

        let config = config_loader.load().await;
        let ses = SesTransport::new(aws_sdk_sesv2::Client::new(&config));

        let site_url = env::var("SITE_URL")
            .map_err(|_| anyhow::anyhow!("SITE_URL environment variable must be set"))?;

        let transport = Arc::new(SelectingTransport::new(
            ses,
            reqwest::Client::new(),
            SENDGRID_DEFAULT_BASE_URL.to_string(),
            settings.clone(),
        ));
        Ok(Self::with_transport(transport, settings, site_url))
    }

    /// Resolve the locale to render an email in: the recipient's stored
    /// preference when it is a supported code, otherwise the site default.
    async fn resolve_locale(&self, requested: Option<&str>) -> Result<String> {
        if let Some(loc) = requested {
            if crate::profile::locale::is_supported(loc) {
                return Ok(loc.to_string());
            }
        }
        self.settings.get_default_locale().await
    }

    /// Wrap rendered parts in an `EmailMessage` and hand it to the transport.
    async fn dispatch(
        &self,
        from: String,
        to: String,
        parts: render::EmailParts,
        context: &'static str,
    ) -> Result<()> {
        let msg = EmailMessage {
            from,
            to,
            subject: parts.subject,
            html_body: parts.html_body,
            text_body: parts.text_body,
        };
        self.transport.send(&msg).await.context(context)
    }

    /// Send an invite email when an admin pre-provisions a user via
    /// `POST /api/admin/users`. The body includes the `/invite/{code}` link
    /// the recipient clicks to activate their account (either by signing in
    /// via SSO when OIDC is enabled, or by setting a password when not).
    pub async fn send_invite_email(
        &self,
        to_email: &str,
        invite_code: &str,
        role: &str,
        inviter_email: &str,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let site_name = self.settings.get_site_name().await?;
        let from_email = self.settings.get_from_email().await?;

        let invite_url = format!("{}/invite/{}", self.site_url, invite_code);
        let role_key = match role {
            crate::entities::user::ROLE_ADMINISTRATOR => "roles.administrator",
            crate::entities::user::ROLE_POSTER => "roles.poster",
            _ => "roles.member",
        };
        let role_label = render::tr(&locale, role_key).unwrap_or_default();

        let parts = render::build(
            &locale,
            "invite",
            Some(&invite_url),
            &[
                ("site", site_name),
                ("inviter", inviter_email.to_string()),
                ("role", role_label),
            ],
        )?;

        self.dispatch(
            from_email,
            to_email.to_string(),
            parts,
            "Failed to send invite email",
        )
        .await?;

        tracing::info!("Invite email sent to {} (role={})", to_email, role);
        Ok(())
    }

    pub async fn send_verification_email(
        &self,
        to_email: &str,
        verification_token: &str,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let site_name = self.settings.get_site_name().await?;
        let from_email = self.settings.get_from_email().await?;

        let verification_url = format!(
            "{}/admin/verify-email?token={}",
            self.site_url, verification_token
        );

        let parts = render::build(
            &locale,
            "verify_admin",
            Some(&verification_url),
            &[("site", site_name)],
        )?;

        self.dispatch(
            from_email,
            to_email.to_string(),
            parts,
            "Failed to send verification email",
        )
        .await?;

        tracing::info!("Verification email sent to {}", to_email);

        Ok(())
    }

    pub async fn send_contact_form_email(
        &self,
        from_name: &str,
        from_email: &str,
        subject: &str,
        message: &str,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let site_name = self.settings.get_site_name().await?;
        let to_email = self.settings.get_contact_email().await?;
        let sender_email = self.settings.get_from_email().await?;

        let parts = render::build(
            &locale,
            "contact_form",
            None,
            &[
                ("name", from_name.to_string()),
                ("email", from_email.to_string()),
                ("subject", subject.to_string()),
                ("message", message.to_string()),
                ("site", site_name),
            ],
        )?;

        self.dispatch(
            sender_email,
            to_email,
            parts,
            "Failed to send contact form email",
        )
        .await?;

        tracing::info!("Contact form email sent from {}", from_email);

        Ok(())
    }

    /// Notify the business owner of a new order (business module). Renders
    /// generically from the order's `title`/`summary`, the customer identity,
    /// and an optional quoted estimate; it has no knowledge of any specific
    /// product. Plaintext PII is passed in here only to compose the message and
    /// is never logged. Recipient is the configured order email.
    #[cfg(feature = "business")]
    #[allow(clippy::too_many_arguments)]
    pub async fn send_order_notification(
        &self,
        title: &str,
        summary: &str,
        customer_name: &str,
        customer_phone: &str,
        customer_email: Option<&str>,
        estimate_total: Option<f64>,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let site_name = self.settings.get_site_name().await?;
        let to_email = self.settings.get_order_email().await?;
        let sender_email = self.settings.get_from_email().await?;

        let not_provided =
            render::tr(&locale, "order_notification.not_provided").unwrap_or_default();
        let estimate_str = match estimate_total {
            Some(v) => format!("${:.2}", v),
            None => not_provided.clone(),
        };
        let email_str = customer_email.map(str::to_string).unwrap_or(not_provided);

        let parts = render::build(
            &locale,
            "order_notification",
            None,
            &[
                ("title", title.to_string()),
                ("name", customer_name.to_string()),
                ("phone", customer_phone.to_string()),
                ("email", email_str),
                ("estimate", estimate_str),
                ("summary", summary.to_string()),
                ("site", site_name),
            ],
        )?;

        self.dispatch(
            sender_email,
            to_email,
            parts,
            "Failed to send order notification email",
        )
        .await?;

        tracing::info!("Order notification email sent for {}", title);

        Ok(())
    }

    /// Confirmation sent to the customer after they place an order (business
    /// module). Reuses the order `title`/`summary` (which includes the bill of
    /// materials) so the customer keeps a record; a call to confirm the deposit
    /// follows. Plaintext PII is used only to compose the message, never logged.
    #[cfg(feature = "business")]
    pub async fn send_order_confirmation(
        &self,
        to_email: &str,
        customer_name: &str,
        title: &str,
        summary: &str,
        estimate_total: Option<f64>,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let site_name = self.settings.get_site_name().await?;
        let sender_email = self.settings.get_from_email().await?;

        let estimate_str = match estimate_total {
            Some(v) => format!("${:.2}", v),
            None => render::tr(&locale, "order_confirmation.estimate_tbd").unwrap_or_default(),
        };

        let parts = render::build(
            &locale,
            "order_confirmation",
            None,
            &[
                ("title", title.to_string()),
                ("name", customer_name.to_string()),
                ("estimate", estimate_str),
                ("summary", summary.to_string()),
                ("site", site_name),
            ],
        )?;

        self.dispatch(
            sender_email,
            to_email.to_string(),
            parts,
            "Failed to send order confirmation email",
        )
        .await?;

        tracing::info!("Order confirmation email sent to customer");

        Ok(())
    }

    pub async fn send_subscription_confirmation(
        &self,
        to_email: &str,
        verification_token: &str,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let site_name = self.settings.get_site_name().await?;
        let from_email = self.settings.get_from_email().await?;

        let verification_url = format!(
            "{}/api/subscribe/verify?token={}",
            self.site_url, verification_token
        );

        let parts = render::build(
            &locale,
            "subscription_confirmation",
            Some(&verification_url),
            &[("site", site_name)],
        )?;

        self.dispatch(
            from_email,
            to_email.to_string(),
            parts,
            "Failed to send subscription confirmation email",
        )
        .await?;

        tracing::info!("Subscription confirmation email sent to {}", to_email);

        Ok(())
    }

    pub async fn send_password_changed_notification(
        &self,
        to_email: &str,
        changed_by_admin: bool,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let from_email = self.settings.get_from_email().await?;

        let source_key = if changed_by_admin {
            "password_changed.source_admin"
        } else {
            "password_changed.source_self"
        };
        let change_source = render::tr(&locale, source_key).unwrap_or_default();

        let parts = render::build(
            &locale,
            "password_changed",
            None,
            &[("change_source", change_source)],
        )?;

        self.dispatch(
            from_email,
            to_email.to_string(),
            parts,
            "Failed to send password change notification",
        )
        .await?;

        tracing::info!("Password change notification sent to {}", to_email);

        Ok(())
    }

    pub async fn send_password_reset_email(
        &self,
        to_email: &str,
        reset_token: &str,
        locale: Option<&str>,
    ) -> Result<()> {
        let locale = self.resolve_locale(locale).await?;
        let from_email = self.settings.get_from_email().await?;
        let reset_url = format!(
            "{}/admin/reset-password?token={}",
            self.site_url, reset_token
        );

        let parts = render::build(&locale, "password_reset", Some(&reset_url), &[])?;

        self.dispatch(
            from_email,
            to_email.to_string(),
            parts,
            "Failed to send password reset email",
        )
        .await?;

        tracing::info!("Password reset email sent to {}", to_email);

        Ok(())
    }
}
