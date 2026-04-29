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
//! Posts have three visibility tiers (`public`, `commenters`, `posters`).
//! The feed query filters by the caller's tier so anonymous visitors only
//! see public posts, commenters see public + commenter-visible, and
//! posters/admins see everything. Bodies are authored in markdown and
//! rendered server-side via `pulldown-cmark` with `ammonia` sanitization
//! before being sent to clients.

pub mod markdown;
pub mod routes;

use crate::entities::{post, user};

/// Caller's effective visibility tier for the feed query. Maps the user's
/// role to the set of visibility values the caller is permitted to see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedTier {
    /// Anonymous visitors. Public posts only.
    Anonymous,
    /// Commenters see public + commenter-only posts.
    Commenter,
    /// Posters and administrators see all tiers.
    Privileged,
}

impl FeedTier {
    pub fn from_role(role: Option<&str>) -> Self {
        match role {
            Some(user::ROLE_ADMINISTRATOR) | Some(user::ROLE_POSTER) => FeedTier::Privileged,
            Some(user::ROLE_COMMENTER) => FeedTier::Commenter,
            _ => FeedTier::Anonymous,
        }
    }

    /// Visibility values this tier is permitted to read.
    pub fn allowed_visibilities(self) -> &'static [&'static str] {
        match self {
            FeedTier::Anonymous => &[post::VISIBILITY_PUBLIC],
            FeedTier::Commenter => &[post::VISIBILITY_PUBLIC, post::VISIBILITY_COMMENTERS],
            FeedTier::Privileged => &[
                post::VISIBILITY_PUBLIC,
                post::VISIBILITY_COMMENTERS,
                post::VISIBILITY_POSTERS,
            ],
        }
    }

    pub fn can_read(self, visibility: &str) -> bool {
        self.allowed_visibilities().contains(&visibility)
    }
}
