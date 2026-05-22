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
use crate::engagement::PostEngagement;
use crate::entities::{article_details, category, post, post_media, user};
use crate::posts::markdown;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Deserialize, Default, TS)]
#[ts(export)]
pub struct CreateArticleRequest {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub cover_media_id: Option<Uuid>,
    /// Defaults to true: composer's first call mints a draft.
    /// Set to false to publish directly without a draft round-trip.
    #[serde(default = "default_true")]
    pub is_draft: bool,
}

#[derive(Deserialize, Default, TS)]
#[ts(export)]
pub struct UpdateArticleRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub clear_category: bool,
    #[serde(default)]
    pub cover_media_id: Option<Uuid>,
    #[serde(default)]
    pub clear_cover: bool,
    /// Publish toggle: passing `false` here flips a draft into a published
    /// article (bumps `published_at`, applies the chosen visibility).
    /// Going from published to draft is not supported.
    #[serde(default)]
    pub is_draft: Option<bool>,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ArticleResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub body: String,
    pub body_html: String,
    pub excerpt: Option<String>,
    #[ts(type = "number")]
    pub reading_time_minutes: i32,
    pub cover_media_id: Option<Uuid>,
    pub cover_url: Option<String>,
    pub is_draft: bool,
    pub visibility: String,
    pub effective_visibility: String,
    pub published_at: String,
    pub created_at: String,
    pub updated_at: String,
    #[ts(type = "Record<string, number>")]
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
    #[ts(type = "number")]
    pub comment_count: i64,
    pub category: Option<crate::posts::types::PostCategoryRef>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ArticleSummary {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub excerpt: Option<String>,
    #[ts(type = "number")]
    pub reading_time_minutes: i32,
    pub cover_media_id: Option<Uuid>,
    pub cover_url: Option<String>,
    pub is_draft: bool,
    pub visibility: String,
    pub effective_visibility: String,
    pub published_at: String,
    pub updated_at: String,
    #[ts(type = "number")]
    pub reaction_count: i64,
    #[ts(type = "number")]
    pub comment_count: i64,
    pub category: Option<crate::posts::types::PostCategoryRef>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ListArticlesQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub limit: Option<u64>,
    #[serde(default)]
    pub author: Option<Uuid>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ListArticlesResponse {
    pub articles: Vec<ArticleSummary>,
    pub next_cursor: Option<String>,
}

/// Word-count to minutes at 200 wpm, floored to a minimum of one. Used by
/// create/update handlers to populate `article_details.reading_time_minutes`.
pub fn compute_reading_time_minutes(body: &str) -> i32 {
    let words = body.split_whitespace().count();
    let minutes = words.div_ceil(200);
    minutes.max(1) as i32
}

/// Length cap on the auto-derived excerpt, in characters (Unicode scalar
/// values, not bytes). Roughly two sentences; matches the feed card budget.
pub const EXCERPT_MAX_CHARS: usize = 200;

/// Derive a plain-text excerpt from markdown when the author hasn't set one.
/// Strips markdown syntax via a server-side render then collapses whitespace,
/// so the output is suitable for the feed card preview without re-rendering.
pub fn derive_excerpt(body: &str) -> Option<String> {
    if body.trim().is_empty() {
        return None;
    }
    let html = markdown::render_to_html(body);
    let text = html_to_plain_text(&html);
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let truncated: String = collapsed.chars().take(EXCERPT_MAX_CHARS).collect();
    // If we cut mid-stream, append a single-character ellipsis so readers know.
    if collapsed.chars().count() > EXCERPT_MAX_CHARS {
        Some(format!("{}\u{2026}", truncated.trim_end()))
    } else {
        Some(truncated)
    }
}

fn html_to_plain_text(html: &str) -> String {
    // Lightweight tag stripper. The HTML comes from our own server-side
    // markdown renderer (ammonia-sanitized), so it's well-formed and there
    // are no embedded scripts/styles to worry about; a full DOM parse would
    // be overkill for a feed-card excerpt.
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match (ch, in_tag) {
            ('<', _) => in_tag = true,
            ('>', _) => in_tag = false,
            (c, false) => out.push(c),
            _ => {}
        }
    }
    // Decode the handful of named entities ammonia emits. Rare enough to
    // skip a full HTML-entities crate dependency.
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

pub fn build_article_response(
    row: post::Model,
    details: article_details::Model,
    author: Option<&user::Model>,
    cover: Option<&post_media::Model>,
    engagement: PostEngagement,
    category: Option<&category::Model>,
) -> ArticleResponse {
    let body_html = markdown::render_to_html(&row.body);
    let category_ref =
        category.map(|c| crate::posts::types::PostCategoryRef {
            id: c.id,
            slug: c.slug.clone(),
            name: c.name.clone(),
            color: c.color.clone(),
        });
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    let cover_url = cover.map(|m| format!("/media/{}", m.id));
    ArticleResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        title: row.slug.clone().unwrap_or_default(),
        subtitle: details.subtitle,
        body: row.body,
        body_html,
        excerpt: details.excerpt,
        reading_time_minutes: details.reading_time_minutes,
        cover_media_id: details.cover_media_id,
        cover_url,
        is_draft: details.is_draft,
        visibility: row.visibility,
        effective_visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        reaction_counts: engagement.reaction_counts,
        viewer_reaction_kinds: engagement.viewer_reaction_kinds,
        comment_count: engagement.comment_count,
        category: category_ref,
    }
}

pub fn build_article_summary(
    row: post::Model,
    details: article_details::Model,
    author: Option<&user::Model>,
    cover: Option<&post_media::Model>,
    engagement: &PostEngagement,
    category: Option<&category::Model>,
) -> ArticleSummary {
    let category_ref =
        category.map(|c| crate::posts::types::PostCategoryRef {
            id: c.id,
            slug: c.slug.clone(),
            name: c.name.clone(),
            color: c.color.clone(),
        });
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    let cover_url = cover.map(|m| format!("/media/{}", m.id));
    let reaction_count: i64 = engagement.reaction_counts.values().sum();
    ArticleSummary {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        title: row.slug.unwrap_or_default(),
        subtitle: details.subtitle,
        excerpt: details.excerpt,
        reading_time_minutes: details.reading_time_minutes,
        cover_media_id: details.cover_media_id,
        cover_url,
        is_draft: details.is_draft,
        visibility: row.visibility,
        effective_visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        reaction_count,
        comment_count: engagement.comment_count,
        category: category_ref,
    }
}

