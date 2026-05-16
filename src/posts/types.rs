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
use crate::engagement::comments::build_comment_response;
use crate::engagement::types::CommentResponse;
use crate::engagement::PostEngagement;
use crate::entities::{category, post, post_media, user};
use crate::posts::markdown;
use crate::posts::media::is_video_mime;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct UpdatePostMediaRequest {
    /// Empty/whitespace clears the caption (NULL); a present value
    /// replaces it. Omitted means "leave alone".
    #[serde(default)]
    pub caption: Option<String>,
}

#[derive(Deserialize)]
pub struct MediaOrderItem {
    pub id: Uuid,
    pub ordinal: i32,
}

#[derive(Deserialize)]
pub struct UpdatePostRequest {
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    /// Phase 9e: assign / move to a different category. Provide a UUID to
    /// set; omit to leave alone. Pair with `clear_category=true` to
    /// remove a categorization without a sentinel value.
    #[serde(default)]
    pub category_id: Option<Uuid>,
    /// Explicit boolean: set to true to clear the post's category.
    /// Omitted = no change, true = clear, false = no change.
    #[serde(default)]
    pub clear_category: bool,
}

#[derive(Serialize)]
pub struct PostCategoryRef {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize)]
pub struct PostResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub body: String,
    pub body_html: String,
    /// Visibility stored on the post row. For categorized posts this is
    /// preserved-but-ignored; the category drives access.
    pub visibility: String,
    /// Visibility actually enforced for this post.
    pub effective_visibility: String,
    pub published_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub media: Vec<PostMediaResponse>,
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
    pub comment_count: i64,
    pub category: Option<PostCategoryRef>,
    pub top_comments: Vec<CommentResponse>,
}

#[derive(Serialize)]
pub struct PostMediaResponse {
    pub id: Uuid,
    /// Browser-facing URL. Hits `/media/{media_id}` which checks tier
    /// visibility before serving from S3.
    pub url: String,
    pub mime_type: String,
    /// Coarse kind derived from `mime_type`: `"image"` or `"video"`.
    pub media_kind: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ordinal: i32,
    pub caption: Option<String>,
}

impl PostMediaResponse {
    pub fn from_model(m: post_media::Model) -> Self {
        let media_kind = if is_video_mime(&m.mime_type) {
            "video"
        } else {
            "image"
        };
        Self {
            url: format!("/media/{}", m.id),
            id: m.id,
            mime_type: m.mime_type,
            media_kind,
            width: m.width,
            height: m.height,
            ordinal: m.ordinal,
            caption: m.caption,
        }
    }
}

pub fn build_post_response(
    row: post::Model,
    author: Option<&user::Model>,
    media: Vec<post_media::Model>,
    engagement: PostEngagement,
    category: Option<&category::Model>,
    top_comment_authors: &HashMap<Uuid, user::Model>,
) -> PostResponse {
    let body_html = markdown::render_to_html(&row.body);
    let category_ref = category.map(|c| PostCategoryRef {
        id: c.id,
        slug: c.slug.clone(),
        name: c.name.clone(),
        color: c.color.clone(),
    });
    let top_comments = engagement
        .top_comments
        .into_iter()
        .map(|c| {
            let cauthor = top_comment_authors.get(&c.user_id);
            build_comment_response(c, cauthor)
        })
        .collect();
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    PostResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        body: row.body,
        body_html,
        visibility: row.visibility,
        effective_visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        media: media
            .into_iter()
            .map(PostMediaResponse::from_model)
            .collect(),
        reaction_counts: engagement.reaction_counts,
        viewer_reaction_kinds: engagement.viewer_reaction_kinds,
        comment_count: engagement.comment_count,
        category: category_ref,
        top_comments,
    }
}

#[derive(Deserialize)]
pub struct FeedQuery {
    /// Cursor in the form `{published_at_rfc3339}_{post_id}`. Returned in
    /// the previous page's `next_cursor`. Omitted on first page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size. Capped server-side.
    #[serde(default)]
    pub limit: Option<u64>,
    /// Optional author filter.
    #[serde(default)]
    pub author: Option<Uuid>,
    /// Optional category filter. Slug, not id, for nice URLs.
    /// `?category=uncategorized` matches posts with `category_id IS NULL`.
    #[serde(default)]
    pub category: Option<String>,
    /// Search term. Empty/whitespace = no filter. When set, ordering
    /// switches to BM25 relevance and pagination collapses to a single page.
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct FeedResponse {
    pub posts: Vec<PostResponse>,
    pub next_cursor: Option<String>,
}
