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
//! Albums (Phase 9d) — first-class media collections.
//!
//! Albums are explicitly *not* posts. They never appear in `/api/feed`;
//! they're discovered via the left-rail Albums group and rendered at
//! `/album/:id`. Each album has a name, optional description, ordered
//! media items with per-item captions, a cover, the same four-value
//! visibility enum as posts, and an FB import dedup key.
//!
//! Visibility filtering reuses [`crate::posts::can_read_post`] so the
//! private-tier author override behaves the same as it does for posts.

pub mod handlers;
pub mod queries;
pub mod types;

use crate::s3::S3Service;
use crate::settings::SettingsService;
use sea_orm::DatabaseConnection;

pub use handlers::{album_read_routes, album_write_routes};

#[derive(Clone)]
pub struct AlbumsState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: SettingsService,
}
