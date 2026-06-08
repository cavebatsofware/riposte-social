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
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::crypto;
use crate::entities::{setting, Setting};

/// Returned by `SettingsService` when a caller chooses the wrong
/// encryption-mode helper (e.g. `get` on an encrypted row, or `set` on
/// a row that is already encrypted). Admin handlers downcast to this
/// to surface a 400 rather than a 500.
#[derive(Debug, thiserror::Error)]
pub enum EncryptionMismatch {
    #[error("setting {key:?} is encrypted; use get_encrypted")]
    GetOnEncryptedRow { key: String },
    #[error("setting {key:?} is not encrypted; use get")]
    GetOnPlaintextRow { key: String },
    #[error(
        "setting {key:?} encryption mismatch: row encrypted={row_encrypted}, \
         write encrypted={write_encrypted}"
    )]
    UpsertModeMismatch {
        key: String,
        row_encrypted: bool,
        write_encrypted: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SettingsService {
    db: DatabaseConnection,
}

impl SettingsService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn find_one(
        &self,
        key: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<Option<setting::Model>> {
        let mut query = Setting::find().filter(setting::Column::Key.eq(key));

        if let Some(cat) = category {
            query = query.filter(setting::Column::Category.eq(cat));
        } else {
            query = query.filter(setting::Column::Category.is_null());
        }

        if let Some(eid) = entity_id {
            query = query.filter(setting::Column::EntityId.eq(eid));
        } else {
            query = query.filter(setting::Column::EntityId.is_null());
        }

        Ok(query.one(&self.db).await?)
    }

    /// Get a plaintext setting value. Returns `None` if the row is
    /// absent. Errors if the row is marked `encrypted = true`; callers
    /// must use `get_encrypted` for those.
    pub async fn get(
        &self,
        key: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<Option<String>> {
        match self.find_one(key, category, entity_id).await? {
            Some(row) if row.encrypted => {
                Err(EncryptionMismatch::GetOnEncryptedRow { key: row.key }.into())
            }
            Some(row) => Ok(Some(row.value)),
            None => Ok(None),
        }
    }

    /// Decrypt and return an encrypted setting value. Errors if the row
    /// exists but is not marked `encrypted = true`.
    pub async fn get_encrypted(
        &self,
        key: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<Option<String>> {
        match self.find_one(key, category, entity_id).await? {
            Some(row) if !row.encrypted => {
                Err(EncryptionMismatch::GetOnPlaintextRow { key: row.key }.into())
            }
            Some(row) => Ok(Some(crypto::decrypt_value(&row.value)?)),
            None => Ok(None),
        }
    }

    pub async fn get_bool(
        &self,
        key: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<bool> {
        let value = self.get(key, category, entity_id).await?;
        Ok(value.map(|v| v == "true").unwrap_or(false))
    }

    /// Insert or update a plaintext setting. Errors if an existing row
    /// is marked encrypted; callers must use `set_encrypted` for those.
    pub async fn set(
        &self,
        key: &str,
        value: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<()> {
        self.upsert(key, value, category, entity_id, false).await
    }

    /// Encrypt and store a value with `encrypted = true`. Errors if an
    /// existing row is plaintext; callers must use `set` for those.
    pub async fn set_encrypted(
        &self,
        key: &str,
        value: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<()> {
        let ciphertext = crypto::encrypt_value(value)?;
        self.upsert(key, &ciphertext, category, entity_id, true)
            .await
    }

    async fn upsert(
        &self,
        key: &str,
        stored_value: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
        encrypted: bool,
    ) -> Result<()> {
        let existing = self.find_one(key, category, entity_id).await?;

        if let Some(existing_setting) = existing {
            if existing_setting.encrypted != encrypted {
                return Err(EncryptionMismatch::UpsertModeMismatch {
                    key: existing_setting.key,
                    row_encrypted: existing_setting.encrypted,
                    write_encrypted: encrypted,
                }
                .into());
            }
            let mut active: setting::ActiveModel = existing_setting.into();
            active.value = Set(stored_value.to_string());
            active.updated_at = Set(chrono::Utc::now().into());
            active.update(&self.db).await?;
        } else {
            let new_setting = setting::ActiveModel {
                id: Set(Uuid::new_v4()),
                key: Set(key.to_string()),
                value: Set(stored_value.to_string()),
                category: Set(category.map(|s| s.to_string())),
                entity_id: Set(entity_id),
                created_at: Set(chrono::Utc::now().into()),
                updated_at: Set(chrono::Utc::now().into()),
                encrypted: Set(encrypted),
            };
            new_setting.insert(&self.db).await?;
        }

        Ok(())
    }

    pub async fn get_all(&self) -> Result<Vec<setting::Model>> {
        let settings = Setting::find().all(&self.db).await?;
        Ok(settings)
    }

    /// Get site name (falls back to env SITE_NAME or default)
    pub async fn get_site_name(&self) -> Result<String> {
        if let Some(name) = self.get("site_name", Some("site"), None).await? {
            return Ok(name);
        }
        Ok(std::env::var("SITE_NAME").unwrap_or_else(|_| "riposte-social".to_string()))
    }

    /// Get contact email (falls back to env CONTACT_EMAIL)
    pub async fn get_contact_email(&self) -> Result<String> {
        if let Some(email) = self.get("contact_email", Some("site"), None).await? {
            return Ok(email);
        }
        Ok(std::env::var("CONTACT_EMAIL").unwrap_or_else(|_| "contact@example.com".to_string()))
    }

    /// Get from email (falls back to env AWS_SES_FROM_EMAIL)
    pub async fn get_from_email(&self) -> Result<String> {
        if let Some(email) = self.get("from_email", Some("site"), None).await? {
            return Ok(email);
        }
        Ok(std::env::var("AWS_SES_FROM_EMAIL")
            .unwrap_or_else(|_| "noreply@example.com".to_string()))
    }

    /// Check if admin registration is enabled (defaults to false for security)
    pub async fn get_admin_registration_enabled(&self) -> Result<bool> {
        self.get_bool("admin_registration_enabled", Some("system"), None)
            .await
    }

    /// Check if access codes feature is enabled (defaults to true)
    pub async fn get_access_codes_enabled(&self) -> Result<bool> {
        self.get_bool("access_codes_enabled", Some("features"), None)
            .await
    }

    /// Check if contact form feature is enabled (defaults to true)
    pub async fn get_contact_form_enabled(&self) -> Result<bool> {
        self.get_bool("contact_form_enabled", Some("features"), None)
            .await
    }

    /// Check if newsletter subscriptions feature is enabled (defaults to true)
    pub async fn get_subscriptions_enabled(&self) -> Result<bool> {
        self.get_bool("subscriptions_enabled", Some("features"), None)
            .await
    }

    /// Whether the business module (order intake) is live (defaults to false).
    /// On top of the compile-time `business` feature: a built-but-off site
    /// returns 404 from the order endpoint.
    pub async fn get_business_enabled(&self) -> Result<bool> {
        self.get_bool("business_enabled", Some("features"), None)
            .await
    }

    /// Public URL of the storefront/shop (e.g. https://shop.example.com), shown
    /// as a link from the social + admin frontends. `None`/empty when unset.
    pub async fn get_shop_url(&self) -> Result<Option<String>> {
        let v = self.get("shop_url", Some("business"), None).await?;
        Ok(v.filter(|s| !s.is_empty()))
    }

    /// Configurable order-status workflow (admins edit the comma-separated list
    /// in settings). Falls back to a sensible default when unset/empty.
    pub async fn get_order_statuses(&self) -> Result<Vec<String>> {
        const DEFAULT: &[&str] = &[
            "placed",
            "pending deposit",
            "material prep",
            "construction",
            "finishing",
            "complete",
            "cancelled",
        ];
        let list: Vec<String> = self
            .get("order_statuses", Some("business"), None)
            .await?
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if list.is_empty() {
            Ok(DEFAULT.iter().map(|s| s.to_string()).collect())
        } else {
            Ok(list)
        }
    }

    /// Recipient for order notifications. Falls back to the contact email.
    pub async fn get_order_email(&self) -> Result<String> {
        if let Some(email) = self.get("order_email", Some("business"), None).await? {
            return Ok(email);
        }
        self.get_contact_email().await
    }

    /// Whether order SMS notifications are sent (defaults to false). Reserved
    /// for a future SMS/RCS messaging feature; not currently wired (orders
    /// notify by email only).
    pub async fn get_order_sms_enabled(&self) -> Result<bool> {
        self.get_bool("order_sms_enabled", Some("features"), None)
            .await
    }

    /// Destination phone number for order SMS, stored encrypted (the `secret_`
    /// prefix makes the admin settings API encrypt it). `None` when unset.
    /// Reserved for a future SMS/RCS messaging feature; not currently wired.
    pub async fn get_order_sms_to(&self) -> Result<Option<String>> {
        self.get_encrypted("secret_order_sms_to", Some("business"), None)
            .await
    }

    /// Cloudflare Turnstile secret for the order endpoint, stored encrypted
    /// (the `secret_` prefix makes the admin settings API encrypt it). `None`
    /// when unset (captcha verification skipped); the admin sets it in the
    /// settings UI.
    pub async fn get_turnstile_secret(&self) -> Result<Option<String>> {
        self.get_encrypted("secret_turnstile", Some("business"), None)
            .await
    }

    /// Whether order phone numbers are verified via Twilio Lookup (defaults to
    /// false). On top of the compile-time `business` feature and the presence of
    /// Twilio credentials.
    pub async fn get_phone_verification_enabled(&self) -> Result<bool> {
        self.get_bool("phone_verification_enabled", Some("features"), None)
            .await
    }

    /// Twilio Account SID (not secret). `None`/empty when unset.
    pub async fn get_twilio_account_sid(&self) -> Result<Option<String>> {
        let v = self.get("twilio_account_sid", Some("business"), None).await?;
        Ok(v.filter(|s| !s.is_empty()))
    }

    /// Twilio Auth Token, stored encrypted (the `secret_` prefix makes the admin
    /// settings API encrypt it). `None` when unset; the admin sets it in the UI.
    pub async fn get_twilio_auth_token(&self) -> Result<Option<String>> {
        self.get_encrypted("secret_twilio_auth_token", Some("business"), None)
            .await
    }

    /// Whether posters (not admins) can create new posts. Admins always
    /// bypass this gate. Used to mute the poster tier without revoking
    /// access entirely.
    pub async fn get_poster_posting_enabled(&self) -> Result<bool> {
        self.get_bool("poster_posting_enabled", Some("features"), None)
            .await
    }

    /// Whether the admin invite-creation endpoint accepts new invites.
    /// Site-mode toggle: turning this off prevents new commenters from
    /// being onboarded without removing existing invites.
    pub async fn get_commenter_invites_enabled(&self) -> Result<bool> {
        self.get_bool("commenter_invites_enabled", Some("features"), None)
            .await
    }

    /// Whether anonymous visitors can read the public feed. When false,
    /// `/api/feed`, `/api/posts/{id}`, `/media/{id}`, and the public
    /// comments list all return 401 to anonymous callers; authed users
    /// of any tier are unaffected.
    pub async fn get_public_feed_enabled(&self) -> Result<bool> {
        self.get_bool("public_feed_enabled", Some("features"), None)
            .await
    }

    /// Whether the Facebook archive import endpoint accepts new uploads.
    /// Useful for locking the importer while an admin investigates a
    /// partial run.
    pub async fn get_fb_import_enabled(&self) -> Result<bool> {
        self.get_bool("fb_import_enabled", Some("features"), None)
            .await
    }

    /// Whether posters (not admins) can create, edit, delete, or manage
    /// membership of categories. Admins always bypass. When off, posters
    /// see no Manage controls in the social-frontend Categories page and
    /// every poster-side write endpoint returns 403.
    pub async fn get_poster_category_management_enabled(&self) -> Result<bool> {
        self.get_bool("poster_category_management_enabled", Some("features"), None)
            .await
    }

    /// Maximum width or height in pixels accepted for an uploaded image
    /// (post media and avatars). Inputs exceeding the limit are rejected
    /// before decode to bound peak memory: an NxN RGBA decode is ~4*N^2
    /// bytes resident. 8000 covers every consumer camera at ~256 MiB
    /// worst case; a photography-focused deployment can raise it, an
    /// invite-only family network can drop it to a few thousand to keep
    /// stored bytes small.
    pub async fn get_max_image_dimension(&self) -> Result<u32> {
        const DEFAULT: u32 = 8_000;
        if let Some(value) = self.get("max_image_dimension", Some("media"), None).await? {
            if let Ok(parsed) = value.parse::<u32>() {
                if parsed > 0 {
                    return Ok(parsed);
                }
            }
        }
        Ok(DEFAULT)
    }
}
