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

use crate::entities::{post, user};
use uuid::Uuid;

/// Caller's effective visibility tier for the feed query. Maps the user's
/// role to the set of visibility values the caller is permitted to see by
/// role alone — the `private` tier is handled separately by [`can_read_post`]
/// since it's author-scoped, not role-scoped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedTier {
    /// Anonymous visitors. Public posts only.
    Anonymous,
    /// Commenters see public + commenter-only posts.
    Commenter,
    /// Posters and administrators see all role-gated tiers.
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

    /// Visibility values this tier is permitted to read by role. Excludes
    /// `private` because that tier is author-scoped (use [`can_read_post`]
    /// for the full check).
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

    /// Role-only check. Returns true when the tier may read the visibility
    /// purely by role. Always returns false for `private` because that
    /// tier requires the author-id check that this method doesn't have.
    pub fn can_read(self, visibility: &str) -> bool {
        self.allowed_visibilities().contains(&visibility)
    }
}

/// Full visibility check that honors both role-based tiers and the
/// author-only `private` tier. Returns true when:
/// - the post's visibility is one of the role-allowed tiers, OR
/// - the post is `private` and `viewer_id` is the author.
///
/// Anonymous viewers (`viewer_id = None`) can never read `private` posts.
/// Admins are NOT a special case — by explicit user direction in Phase 9,
/// they cannot peek at other users' private posts via feed/listing
/// surfaces. Admin moderation against a known post id still works through
/// the dedicated moderation routes that operate without the visibility
/// filter.
pub fn can_read_post(tier: FeedTier, visibility: &str, author_id: Uuid, viewer_id: Option<Uuid>) -> bool {
    if tier.can_read(visibility) {
        return true;
    }
    if visibility == post::VISIBILITY_PRIVATE {
        return viewer_id == Some(author_id);
    }
    false
}
