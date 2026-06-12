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
    ) -> Result<()> {
        let site_name = self.settings.get_site_name().await?;
        let from_email = self.settings.get_from_email().await?;

        let invite_url = format!("{}/invite/{}", self.site_url, invite_code);
        let role_label = match role {
            crate::entities::user::ROLE_ADMINISTRATOR => "an administrator",
            crate::entities::user::ROLE_POSTER => "a poster",
            _ => "a member",
        };

        let subject = format!("You're invited to {}", site_name);
        let html_body = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>You're invited</title>
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background-color: #f4f4f4; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h1 style="color: #2c3e50; margin-top: 0;">Welcome to {}</h1>
        <p>{} has invited you to join {} as {}. Click the button below to activate your account.</p>
    </div>

    <div style="background-color: white; border: 1px solid #ddd; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <div style="text-align: center; margin: 30px 0;">
            <a href="{}"
               style="background-color: #3498db; color: white; padding: 12px 30px; text-decoration: none; border-radius: 5px; display: inline-block; font-weight: bold;">
                Accept Invite
            </a>
        </div>
        <p style="color: #666; font-size: 14px;">Or copy and paste this link into your browser:</p>
        <p style="word-break: break-all; color: #3498db; font-size: 14px;">{}</p>
    </div>

    <div style="color: #666; font-size: 12px; text-align: center;">
        <p>This invite link will expire in 7 days.</p>
        <p>If you weren't expecting this invitation, you can safely ignore this email.</p>
    </div>
</body>
</html>
"#,
            site_name, inviter_email, site_name, role_label, invite_url, invite_url
        );

        let text_body = format!(
            r#"
You're invited to {}

{} has invited you to join {} as {}.

Activate your account: {}

This invite link will expire in 7 days.

If you weren't expecting this invitation, you can safely ignore this email.
"#,
            site_name, inviter_email, site_name, role_label, invite_url
        );

        let msg = EmailMessage {
            from: from_email,
            to: to_email.to_string(),
            subject,
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send invite email")?;

        tracing::info!("Invite email sent to {} (role={})", to_email, role);
        Ok(())
    }

    pub async fn send_verification_email(
        &self,
        to_email: &str,
        verification_token: &str,
    ) -> Result<()> {
        let site_name = self.settings.get_site_name().await?;
        let from_email = self.settings.get_from_email().await?;

        let verification_url = format!(
            "{}/admin/verify-email?token={}",
            self.site_url, verification_token
        );

        let subject = "Verify Your Admin Account";
        let html_body = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Verify Your Email</title>
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background-color: #f4f4f4; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h1 style="color: #2c3e50; margin-top: 0;">Welcome to {} Admin</h1>
        <p>Thank you for registering as an admin user. Please verify your email address to complete your registration.</p>
    </div>

    <div style="background-color: white; border: 1px solid #ddd; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <p>Click the button below to verify your email address:</p>
        <div style="text-align: center; margin: 30px 0;">
            <a href="{}"
               style="background-color: #3498db; color: white; padding: 12px 30px; text-decoration: none; border-radius: 5px; display: inline-block; font-weight: bold;">
                Verify Email Address
            </a>
        </div>
        <p style="color: #666; font-size: 14px;">Or copy and paste this link into your browser:</p>
        <p style="word-break: break-all; color: #3498db; font-size: 14px;">{}</p>
    </div>

    <div style="color: #666; font-size: 12px; text-align: center;">
        <p>This verification link will expire in 24 hours.</p>
        <p>If you didn't request this verification email, you can safely ignore it.</p>
    </div>
</body>
</html>
"#,
            site_name, verification_url, verification_url
        );

        let text_body = format!(
            r#"
Welcome to {} Admin

Thank you for registering as an admin user. Please verify your email address to complete your registration.

Verification Link: {}

This verification link will expire in 24 hours.

If you didn't request this verification email, you can safely ignore it.
"#,
            site_name, verification_url
        );

        let msg = EmailMessage {
            from: from_email,
            to: to_email.to_string(),
            subject: subject.to_string(),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send verification email")?;

        tracing::info!("Verification email sent to {}", to_email);

        Ok(())
    }

    pub async fn send_contact_form_email(
        &self,
        from_name: &str,
        from_email: &str,
        subject: &str,
        message: &str,
    ) -> Result<()> {
        let site_name = self.settings.get_site_name().await?;
        let to_email = self.settings.get_contact_email().await?;
        let sender_email = self.settings.get_from_email().await?;

        let html_body = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Contact Form Submission</title>
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background-color: #f4f4f4; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h1 style="color: #2c3e50; margin-top: 0;">New Contact Form Submission</h1>
    </div>

    <div style="background-color: white; border: 1px solid #ddd; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h2 style="color: #2c3e50; margin-top: 0;">Contact Information</h2>
        <p><strong>Name:</strong> {}</p>
        <p><strong>Email:</strong> {}</p>
        <p><strong>Subject:</strong> {}</p>

        <h2 style="color: #2c3e50; margin-top: 30px;">Message</h2>
        <div style="background-color: #f9f9f9; padding: 15px; border-left: 4px solid #3498db; border-radius: 3px;">
            <p style="margin: 0; white-space: pre-wrap;">{}</p>
        </div>
    </div>

    <div style="color: #666; font-size: 12px; text-align: center;">
        <p>This message was sent via the contact form on {}</p>
    </div>
</body>
</html>
"#,
            html_escape(from_name),
            html_escape(from_email),
            html_escape(subject),
            html_escape(message),
            site_name
        );

        let text_body = format!(
            r#"
New Contact Form Submission

Name: {}
Email: {}
Subject: {}

Message:
{}

---
This message was sent via the contact form on {}
"#,
            from_name, from_email, subject, message, site_name
        );

        let msg = EmailMessage {
            from: sender_email,
            to: to_email,
            subject: format!("Contact Form: {}", subject),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send contact form email")?;

        tracing::info!("Contact form email sent from {}", from_email);

        Ok(())
    }

    /// Notify the business owner of a new order (business module). Renders
    /// generically from the order's `title`/`summary`, the customer identity,
    /// and an optional quoted estimate; it has no knowledge of any specific
    /// product. Plaintext PII is passed in here only to compose the message and
    /// is never logged. Recipient is the configured order email.
    #[cfg(feature = "business")]
    pub async fn send_order_notification(
        &self,
        title: &str,
        summary: &str,
        customer_name: &str,
        customer_phone: &str,
        customer_email: Option<&str>,
        estimate_total: Option<f64>,
    ) -> Result<()> {
        let site_name = self.settings.get_site_name().await?;
        let to_email = self.settings.get_order_email().await?;
        let sender_email = self.settings.get_from_email().await?;

        let estimate_str = match estimate_total {
            Some(v) => format!("${:.2}", v),
            None => "not provided".to_string(),
        };
        let email_row = customer_email
            .map(|e| {
                format!(
                    r#"<tr><td style="padding:6px 0;color:#7b8794;">Email</td><td style="padding:6px 0;"><a href="mailto:{0}" style="color:#2c7be5;text-decoration:none;">{0}</a></td></tr>"#,
                    html_escape(e)
                )
            })
            .unwrap_or_default();
        let email_text = customer_email
            .map(|e| format!("Email: {}\n", e))
            .unwrap_or_default();

        let html_body = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="UTF-8"><title>New order</title></head>
<body style="margin:0;background:#f4f4f4;font-family:Arial,Helvetica,sans-serif;color:#2c3e50;">
  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f4f4f4;padding:24px 0;">
    <tr><td align="center">
      <table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;background:#ffffff;border:1px solid #e2e2e2;border-radius:8px;overflow:hidden;">
        <tr><td style="background:#2c3e50;padding:20px 24px;">
          <div style="color:#9fb3c8;font-size:12px;text-transform:uppercase;letter-spacing:.08em;">New order</div>
          <div style="color:#ffffff;font-size:20px;font-weight:bold;margin-top:4px;">{title}</div>
        </td></tr>
        <tr><td style="padding:24px;">
          <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="font-size:14px;">
            <tr><td style="padding:6px 0;color:#7b8794;width:90px;">Customer</td><td style="padding:6px 0;font-weight:bold;">{name}</td></tr>
            <tr><td style="padding:6px 0;color:#7b8794;">Phone</td><td style="padding:6px 0;"><a href="tel:{phone}" style="color:#2c7be5;text-decoration:none;">{phone}</a></td></tr>
            {email_row}
            <tr><td style="padding:6px 0;color:#7b8794;">Estimate</td><td style="padding:6px 0;font-weight:bold;">{estimate}</td></tr>
          </table>
          <div style="margin-top:20px;font-size:12px;text-transform:uppercase;letter-spacing:.08em;color:#7b8794;">Order details</div>
          <pre style="margin:8px 0 0;padding:14px;background:#f7f9fb;border:1px solid #e2e8f0;border-radius:6px;font-family:Consolas,'SFMono-Regular',monospace;font-size:13px;line-height:1.5;white-space:pre-wrap;color:#33404d;">{summary}</pre>
        </td></tr>
        <tr><td style="padding:16px 24px;border-top:1px solid #eeeeee;color:#9aa5b1;font-size:12px;">Submitted via {site}</td></tr>
      </table>
    </td></tr>
  </table>
</body>
</html>"#,
            title = html_escape(title),
            name = html_escape(customer_name),
            phone = html_escape(customer_phone),
            email_row = email_row,
            estimate = html_escape(&estimate_str),
            summary = html_escape(summary),
            site = html_escape(&site_name),
        );

        let text_body = format!(
            r#"
New Order: {}

Name: {}
Phone: {}
{}Estimate: {}

Order:
{}

---
This order was submitted on {}
"#,
            title, customer_name, customer_phone, email_text, estimate_str, summary, site_name
        );

        let msg = EmailMessage {
            from: sender_email,
            to: to_email,
            subject: format!("New order: {}", title),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send order notification email")?;

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
    ) -> Result<()> {
        let site_name = self.settings.get_site_name().await?;
        let sender_email = self.settings.get_from_email().await?;

        let estimate_str = match estimate_total {
            Some(v) => format!("${:.2}", v),
            None => "to be confirmed".to_string(),
        };

        let html_body = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="UTF-8"><title>Order received</title></head>
<body style="margin:0;background:#f4f4f4;font-family:Arial,Helvetica,sans-serif;color:#2c3e50;">
  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f4f4f4;padding:24px 0;">
    <tr><td align="center">
      <table role="presentation" width="600" cellpadding="0" cellspacing="0" style="max-width:600px;background:#ffffff;border:1px solid #e2e2e2;border-radius:8px;overflow:hidden;">
        <tr><td style="background:#2c3e50;padding:20px 24px;">
          <div style="color:#9fb3c8;font-size:12px;text-transform:uppercase;letter-spacing:.08em;">Order received</div>
          <div style="color:#ffffff;font-size:20px;font-weight:bold;margin-top:4px;">{title}</div>
        </td></tr>
        <tr><td style="padding:24px;font-size:14px;line-height:1.6;">
          <p style="margin:0 0 12px;">Hi {name},</p>
          <p style="margin:0 0 12px;">Thanks for your order. I will personally call you to confirm the details and walk through the deposit before anything is charged. Nothing has been charged yet.</p>
          <p style="margin:0 0 4px;"><strong>Estimate:</strong> {estimate}</p>
          <div style="margin-top:20px;font-size:12px;text-transform:uppercase;letter-spacing:.08em;color:#7b8794;">Your order</div>
          <pre style="margin:8px 0 0;padding:14px;background:#f7f9fb;border:1px solid #e2e8f0;border-radius:6px;font-family:Consolas,'SFMono-Regular',monospace;font-size:13px;line-height:1.5;white-space:pre-wrap;color:#33404d;">{summary}</pre>
          <p style="margin:16px 0 0;color:#7b8794;font-size:13px;">Please keep this email for your records.</p>
        </td></tr>
        <tr><td style="padding:16px 24px;border-top:1px solid #eeeeee;color:#9aa5b1;font-size:12px;">{site}</td></tr>
      </table>
    </td></tr>
  </table>
</body>
</html>"#,
            title = html_escape(title),
            name = html_escape(customer_name),
            estimate = html_escape(&estimate_str),
            summary = html_escape(summary),
            site = html_escape(&site_name),
        );

        let text_body = format!(
            "Hi {name},\n\nThanks for your order. I will personally call you to confirm the details and walk through the deposit before anything is charged. Nothing has been charged yet.\n\n{title}\nEstimate: {estimate}\n\nYour order:\n{summary}\n\nPlease keep this email for your records.\n\n{site}\n",
            name = customer_name,
            title = title,
            estimate = estimate_str,
            summary = summary,
            site = site_name,
        );

        let msg = EmailMessage {
            from: sender_email,
            to: to_email.to_string(),
            subject: format!("We received your order: {}", title),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send order confirmation email")?;

        tracing::info!("Order confirmation email sent to customer");

        Ok(())
    }

    pub async fn send_subscription_confirmation(
        &self,
        to_email: &str,
        verification_token: &str,
    ) -> Result<()> {
        let site_name = self.settings.get_site_name().await?;
        let from_email = self.settings.get_from_email().await?;

        let verification_url = format!(
            "{}/api/subscribe/verify?token={}",
            self.site_url, verification_token
        );

        let subject = "Confirm Your Blog Subscription";
        let html_body = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Confirm Your Subscription</title>
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background-color: #f4f4f4; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h1 style="color: #2c3e50; margin-top: 0;">Welcome to {} Blog!</h1>
        <p>Thank you for subscribing. Please confirm your email address to start receiving updates.</p>
    </div>

    <div style="background-color: white; border: 1px solid #ddd; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <p>Click the button below to confirm your subscription:</p>
        <div style="text-align: center; margin: 30px 0;">
            <a href="{}"
               style="background-color: #3498db; color: white; padding: 12px 30px; text-decoration: none; border-radius: 5px; display: inline-block; font-weight: bold;">
                Confirm Subscription
            </a>
        </div>
        <p style="color: #666; font-size: 14px;">Or copy and paste this link into your browser:</p>
        <p style="word-break: break-all; color: #3498db; font-size: 14px;">{}</p>
    </div>

    <div style="color: #666; font-size: 12px; text-align: center;">
        <p>This confirmation link will expire in 7 days.</p>
        <p>If you didn't subscribe to this blog, you can safely ignore this email.</p>
    </div>
</body>
</html>
"#,
            site_name, verification_url, verification_url
        );

        let text_body = format!(
            r#"
Welcome to {} Blog!

Thank you for subscribing. Please confirm your email address to start receiving updates.

Confirmation Link: {}

This confirmation link will expire in 7 days.

If you didn't subscribe to this blog, you can safely ignore this email.
"#,
            site_name, verification_url
        );

        let msg = EmailMessage {
            from: from_email,
            to: to_email.to_string(),
            subject: subject.to_string(),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send subscription confirmation email")?;

        tracing::info!("Subscription confirmation email sent to {}", to_email);

        Ok(())
    }

    pub async fn send_password_changed_notification(
        &self,
        to_email: &str,
        changed_by_admin: bool,
    ) -> Result<()> {
        let from_email = self.settings.get_from_email().await?;

        let subject = "Your Password Has Been Changed";
        let change_source = if changed_by_admin {
            "by an administrator"
        } else {
            "using your account"
        };

        let html_body = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Password Changed</title>
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background-color: #f4f4f4; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h1 style="color: #2c3e50; margin-top: 0;">Password Changed</h1>
        <p>Your admin account password was recently changed {}.</p>
    </div>

    <div style="background-color: white; border: 1px solid #ddd; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <p>If you made this change, you can safely ignore this email.</p>
        <p style="color: #c0392b;"><strong>If you did not make this change</strong>, please contact an administrator immediately as your account may have been compromised.</p>
    </div>

    <div style="color: #666; font-size: 12px; text-align: center;">
        <p>This is an automated security notification.</p>
    </div>
</body>
</html>
"#,
            change_source
        );

        let text_body = format!(
            r#"
Password Changed

Your admin account password was recently changed {}.

If you made this change, you can safely ignore this email.

If you did NOT make this change, please contact an administrator immediately as your account may have been compromised.

---
This is an automated security notification.
"#,
            change_source
        );

        let msg = EmailMessage {
            from: from_email,
            to: to_email.to_string(),
            subject: subject.to_string(),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send password change notification")?;

        tracing::info!("Password change notification sent to {}", to_email);

        Ok(())
    }

    pub async fn send_password_reset_email(&self, to_email: &str, reset_token: &str) -> Result<()> {
        let from_email = self.settings.get_from_email().await?;
        let reset_url = format!(
            "{}/admin/reset-password?token={}",
            self.site_url, reset_token
        );

        let subject = "Password Reset Request";
        let html_body = format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Reset Your Password</title>
</head>
<body style="font-family: Arial, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px;">
    <div style="background-color: #f4f4f4; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <h1 style="color: #2c3e50; margin-top: 0;">Password Reset Request</h1>
        <p>We received a request to reset your admin account password.</p>
    </div>

    <div style="background-color: white; border: 1px solid #ddd; border-radius: 5px; padding: 20px; margin-bottom: 20px;">
        <p>Click the button below to reset your password:</p>
        <div style="text-align: center; margin: 30px 0;">
            <a href="{}"
               style="background-color: #3498db; color: white; padding: 12px 30px; text-decoration: none; border-radius: 5px; display: inline-block; font-weight: bold;">
                Reset Password
            </a>
        </div>
        <p style="color: #666; font-size: 14px;">Or copy and paste this link into your browser:</p>
        <p style="word-break: break-all; color: #3498db; font-size: 14px;">{}</p>
    </div>

    <div style="color: #666; font-size: 12px; text-align: center;">
        <p><strong>This link will expire in 1 hour.</strong></p>
        <p>If you didn't request a password reset, you can safely ignore this email. Your password will remain unchanged.</p>
    </div>
</body>
</html>
"#,
            reset_url, reset_url
        );

        let text_body = format!(
            r#"
Password Reset Request

We received a request to reset your admin account password.

Reset Link: {}

This link will expire in 1 hour.

If you didn't request a password reset, you can safely ignore this email. Your password will remain unchanged.
"#,
            reset_url
        );

        let msg = EmailMessage {
            from: from_email,
            to: to_email.to_string(),
            subject: subject.to_string(),
            html_body,
            text_body,
        };
        self.transport
            .send(&msg)
            .await
            .context("Failed to send password reset email")?;

        tracing::info!("Password reset email sent to {}", to_email);

        Ok(())
    }
}

// Helper function to escape HTML entities
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
