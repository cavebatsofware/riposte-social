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
//! Reactions and comments on posts.
//!
//! - Reactions are simple facts: `(post_id, user_id, kind)` is unique. Adding a
//!   reaction is idempotent (insert-or-ignore on the unique index); removing
//!   it is a delete by the same triple. Counts are aggregated per kind for
//!   feed/post payloads. Today only `like` is allowed; the schema and the
//!   `ALLOWED_KINDS` allowlist let us add more without DDL.
//! - Comments carry a soft-delete column so admin moderation can hide a
//!   comment without losing it. Live counts and listings filter on
//!   `deleted_at IS NULL`.
//! - Both sets of endpoints require the caller to have read access to the
//!   parent post via the same `FeedTier` gate the post handlers use.

pub mod aggregate;
pub mod comment_reactions;
pub mod comments;
pub mod media_comments;
pub mod media_reactions;
pub mod reactions;

pub use aggregate::{
    fetch_engagement_for_media, fetch_engagement_for_posts, MediaEngagement, PostEngagement,
};
pub use comment_reactions::{fetch_comment_engagement, CommentEngagement};

use axum::Router;
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct EngagementState {
    pub db: DatabaseConnection,
    pub settings: crate::settings::SettingsService,
}

/// Routes that require an authenticated principal: writing reactions and
/// comments and deleting comments. Visibility-filter checks against the
/// parent post happen inside each handler.
pub fn engagement_write_routes() -> Router<EngagementState> {
    Router::new()
        .merge(reactions::reaction_write_routes())
        .merge(comments::comment_write_routes())
        .merge(comment_reactions::comment_reaction_routes())
        .merge(media_reactions::media_reaction_routes())
        .merge(media_comments::media_comment_write_routes())
}

/// Public read routes: list comments on a post or one of its media items,
/// plus the media engagement summary (reaction counts + comment count)
/// the lightbox lazy-loads on open. Visibility is filtered against the
/// parent post so under-tier callers see the same 404 as missing-post.
pub fn engagement_read_routes() -> Router<EngagementState> {
    Router::new()
        .merge(comments::comment_read_routes())
        .merge(media_comments::media_comment_read_routes())
        .merge(media_reactions::media_engagement_read_routes())
}
