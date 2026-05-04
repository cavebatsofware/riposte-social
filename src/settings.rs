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

use crate::entities::{setting, Setting};

#[derive(Debug, Clone)]
pub struct SettingsService {
    db: DatabaseConnection,
}

impl SettingsService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Get a setting value by key, category, and optional entity_id
    pub async fn get(
        &self,
        key: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<Option<String>> {
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

        let setting = query.one(&self.db).await?;
        Ok(setting.map(|s| s.value))
    }

    /// Get a boolean setting value
    pub async fn get_bool(
        &self,
        key: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<bool> {
        let value = self.get(key, category, entity_id).await?;
        Ok(value.map(|v| v == "true").unwrap_or(false))
    }

    /// Set a setting value, creating it if it doesn't exist
    pub async fn set(
        &self,
        key: &str,
        value: &str,
        category: Option<&str>,
        entity_id: Option<Uuid>,
    ) -> Result<()> {
        // Try to find existing setting
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

        let existing = query.one(&self.db).await?;

        if let Some(existing_setting) = existing {
            // Update existing
            let mut active: setting::ActiveModel = existing_setting.into();
            active.value = Set(value.to_string());
            active.updated_at = Set(chrono::Utc::now().into());
            active.update(&self.db).await?;
        } else {
            // Create new
            let new_setting = setting::ActiveModel {
                id: Set(Uuid::new_v4()),
                key: Set(key.to_string()),
                value: Set(value.to_string()),
                category: Set(category.map(|s| s.to_string())),
                entity_id: Set(entity_id),
                created_at: Set(chrono::Utc::now().into()),
                updated_at: Set(chrono::Utc::now().into()),
            };
            new_setting.insert(&self.db).await?;
        }

        Ok(())
    }

    /// Get all settings
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
}
