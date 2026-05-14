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
//! Post CRUD, feed query, and media upload.
//!
//! Posts have four visibility tiers (`public`, `commenters`, `posters`,
//! `private`). `public` / `commenters` / `posters` are tier-gated by role;
//! `private` is author-only and bypasses the role tier entirely. The feed
//! query filters so anonymous visitors only see public posts, commenters
//! see public + commenter-visible, posters/admins see all three role tiers,
//! and the author of a private post sees their own private posts on top of
//! whatever their role grants.
//!
//! Bodies are authored in markdown and rendered server-side via
//! `pulldown-cmark` with `ammonia` sanitization before being sent to clients.

pub mod markdown;
pub mod routes;
pub mod shared;

// Visibility logic lives in `crate::visibility`. Re-exported here so the
// older `crate::posts::FeedTier` / `crate::posts::can_read_post` import
// paths used across the codebase (and tests) keep working.
pub use crate::visibility::{can_read_post, FeedTier};
