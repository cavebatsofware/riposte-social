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
use crate::entities::{category, post, post_media, user};
use crate::posts::media;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Default)]
pub struct UpdateAlbumRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    /// Explicit cover override. Translated under the hood into a reorder
    /// that moves the chosen media to ordinal=0 and shifts the others.
    #[serde(default)]
    pub cover_media_id: Option<Uuid>,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub clear_category: bool,
}

#[derive(Deserialize, Default)]
pub struct UpdateAlbumMediaRequest {
    #[serde(default)]
    pub caption: Option<String>,
    /// Direct ordinal patch. Callers that want a full reorder (cover
    /// promotion, drag-to-reorder UI) should use the album-level
    /// `cover_media_id` PATCH or compose multiple PATCH calls.
    #[serde(default)]
    pub ordinal: Option<i32>,
}

#[derive(Serialize)]
pub struct AlbumMediaResponse {
    pub id: Uuid,
    pub album_id: Uuid,
    pub url: String,
    pub mime_type: String,
    pub media_kind: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ordinal: i32,
    pub caption: Option<String>,
}

impl AlbumMediaResponse {
    pub fn from_model(album_id: Uuid, m: post_media::Model) -> Self {
        let media_kind = if crate::posts::media::is_video_mime(&m.mime_type) {
            "video"
        } else {
            "image"
        };
        Self {
            url: format!("/album-media/{}", m.id),
            id: m.id,
            album_id,
            mime_type: m.mime_type,
            media_kind,
            width: m.width,
            height: m.height,
            ordinal: m.ordinal,
            caption: m.caption,
        }
    }
}

#[derive(Serialize)]
pub struct AlbumResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub name: String,
    pub description: Option<String>,
    /// Implicit cover: the lowest-ordinal media. Null when the album has
    /// no media.
    pub cover_media_id: Option<Uuid>,
    pub cover_url: Option<String>,
    /// Visibility stored on the album row. For categorized albums this
    /// is preserved-but-ignored; the category drives access.
    pub visibility: String,
    /// Visibility actually enforced. Equals the category's visibility
    /// when categorized, else the album's own visibility.
    pub effective_visibility: String,
    pub published_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub media: Vec<AlbumMediaResponse>,
    pub photo_count: i64,
    pub category_id: Option<Uuid>,
}

pub fn build_album_response(
    row: post::Model,
    author: Option<&user::Model>,
    media: Vec<post_media::Model>,
    category: Option<&category::Model>,
) -> AlbumResponse {
    let cover_media_id = media::implicit_cover_id(&media);
    let cover_url = cover_media_id.map(|id| format!("/album-media/{}", id));
    let photo_count = media.len() as i64;
    let album_id = row.id;
    let media_responses = media
        .into_iter()
        .map(|m| AlbumMediaResponse::from_model(album_id, m))
        .collect();
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    let description = if row.body.is_empty() {
        None
    } else {
        Some(row.body)
    };
    AlbumResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        name: row.slug.unwrap_or_default(),
        description,
        cover_media_id,
        cover_url,
        visibility: row.visibility,
        effective_visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        media: media_responses,
        photo_count,
        category_id: row.category_id,
    }
}

#[derive(Deserialize)]
pub struct ListAlbumsQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub author: Option<Uuid>,
    /// Optional category filter, by slug. `uncategorized` matches albums
    /// with `category_id IS NULL`.
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Serialize)]
pub struct ListAlbumsResponse {
    pub albums: Vec<AlbumSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct AlbumSummary {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub visibility: String,
    pub published_at: String,
    pub photo_count: i64,
}
