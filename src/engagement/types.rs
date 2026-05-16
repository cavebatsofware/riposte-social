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
//! Wire-format types shared by the engagement handler files. Reaction
//! summaries, comment responses, and per-media engagement payloads all
//! live here so the per-area handler modules carry only routing and
//! business logic.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ==================== post comments ====================

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

#[derive(Deserialize)]
pub struct EditCommentRequest {
    pub body: String,
}

#[derive(Serialize)]
pub struct CommentResponse {
    pub id: Uuid,
    pub post_id: Uuid,
    pub user_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub body: String,
    pub body_html: String,
    pub created_at: String,
    pub updated_at: String,
    pub edited_at: Option<String>,
    #[serde(default)]
    pub reaction_counts: HashMap<String, i64>,
    #[serde(default)]
    pub viewer_reaction_kinds: Vec<String>,
}

#[derive(Serialize)]
pub struct CommentListResponse {
    pub comments: Vec<CommentResponse>,
}

// ==================== post reactions ====================

#[derive(Deserialize)]
pub struct CreateReactionRequest {
    pub kind: String,
}

/// Compact "reactions on a post" wire summary. Used by both the post
/// reaction endpoints and (via [`MediaEngagementResponse`]) the per-media
/// lightbox.
#[derive(Serialize)]
pub struct ReactionStateResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

// ==================== comment reactions ====================

#[derive(Deserialize)]
pub struct CreateCommentReactionRequest {
    pub kind: String,
}

#[derive(Serialize)]
pub struct CommentReactionStateResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

// ==================== media reactions ====================

#[derive(Deserialize)]
pub struct CreateMediaReactionRequest {
    pub kind: String,
}

#[derive(Serialize)]
pub struct MediaReactionStateResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

#[derive(Serialize)]
pub struct MediaEngagementResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
    pub comment_count: i64,
}

// ==================== media comments ====================

#[derive(Deserialize)]
pub struct CreateMediaCommentRequest {
    pub body: String,
}

#[derive(Deserialize)]
pub struct EditMediaCommentRequest {
    pub body: String,
}

#[derive(Serialize)]
pub struct MediaCommentResponse {
    pub id: Uuid,
    pub media_id: Uuid,
    pub user_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub body: String,
    pub body_html: String,
    pub created_at: String,
    pub updated_at: String,
    pub edited_at: Option<String>,
}

#[derive(Serialize)]
pub struct MediaCommentListResponse {
    pub comments: Vec<MediaCommentResponse>,
}
