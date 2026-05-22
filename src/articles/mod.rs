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
//! Articles: long-form markdown content stored on the shared `posts` table
//! with `kind='article'` and a 1-to-1 `article_details` row.
//!
//! Reactions, comments, categories, visibility tiers, moderation and the
//! main feed are reused as-is. Only article-specific fields (subtitle,
//! cover_media_id, excerpt, reading_time_minutes, is_draft) live in
//! `article_details`. Article title reuses `posts.slug`; article body
//! reuses `posts.body`. Inline images and the cover image are normal
//! `post_media` rows attached to the article via the existing media-upload
//! endpoint.

pub mod handlers;
pub mod queries;
pub mod types;

use crate::s3::S3Service;
use crate::settings::SettingsService;
use sea_orm::DatabaseConnection;

pub use handlers::{article_authed_read_routes, article_read_routes, article_write_routes};

#[derive(Clone)]
pub struct ArticlesState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: SettingsService,
}
