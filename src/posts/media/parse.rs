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
//! Multipart parsing for compose + append-media requests.
//!
//! Format-level value checks (visibility allowlist, slug length) run here.
//! DB-level checks (category exists, member rules) run in `insert.rs`.

use crate::entities::post;
use crate::errors::{AppError, AppResult};
use crate::posts::media::{is_allowed_media_mime, max_bytes_for_mime, media_files_max_for_kind};
use axum::extract::multipart::Field;
use axum::extract::Multipart;
use std::collections::HashMap;
use uuid::Uuid;

/// Buffered media upload: bytes plus the mime type the browser supplied
/// plus the optional caption (used by album compose; None for posts).
pub struct PendingMedia {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub caption: Option<String>,
}

/// Parsed multipart payload ready for `commit_compose`. Field naming
/// reflects the unified post row: `body` and `slug` (the album name lives
/// in `slug` for kind=album rows; the album description lives in `body`).
pub struct ComposeInput {
    pub kind: String,
    pub body: String,
    pub slug: Option<String>,
    pub visibility: String,
    pub category_id: Option<Uuid>,
    pub media: Vec<PendingMedia>,
}

/// Multipart fields recognized for posts:
///   `body` (text, required), `visibility`, `category_id`,
///   `slug` (optional), `media` (file, repeated).
///
/// Multipart fields recognized for albums:
///   `name` (text, required → slug), `description` (text, optional → body),
///   `visibility`, `category_id`, `media` (file, repeated),
///   `caption_<index>` (text, optional, attached to media at index).
pub async fn parse_compose_multipart(
    multipart: &mut Multipart,
    kind: &str,
) -> AppResult<ComposeInput> {
    if !post::is_valid_kind(kind) {
        return Err(AppError::ValidationError(format!(
            "Invalid kind '{}'",
            kind
        )));
    }

    let mut body: Option<String> = None;
    let mut name: Option<String> = None;
    let mut slug: Option<String> = None;
    let mut visibility = post::VISIBILITY_PRIVATE.to_string();
    let mut category_id: Option<Uuid> = None;
    let mut media: Vec<PendingMedia> = Vec::new();
    let mut captions: HashMap<usize, String> = HashMap::new();
    let media_cap = media_files_max_for_kind(kind);

    while let Some(field) = next_field(multipart).await? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "body" | "description" => {
                body = Some(read_text(field, &field_name).await?);
            }
            "name" => {
                name = Some(read_text(field, "name").await?);
            }
            "slug" => {
                let trimmed = read_text(field, "slug").await?.trim().to_string();
                if !trimmed.is_empty() {
                    slug = Some(trimmed);
                }
            }
            "visibility" => {
                visibility = read_text(field, "visibility").await?;
            }
            "category_id" => {
                let trimmed = read_text(field, "category_id").await?.trim().to_string();
                if !trimmed.is_empty() {
                    category_id = Some(Uuid::parse_str(&trimmed).map_err(|e| {
                        AppError::ValidationError(format!("category_id must be a UUID: {}", e))
                    })?);
                }
            }
            "media" => {
                consume_media_field(field, &mut media, media_cap).await?;
            }
            other if other.starts_with("caption_") => {
                consume_caption_field(other, field, &mut captions).await;
            }
            _ => {
                // Forward-compat: ignore unknown fields rather than rejecting.
            }
        }
    }

    apply_captions(&mut media, captions);

    let (resolved_body, resolved_slug) = if kind == post::KIND_ALBUM {
        let n = name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::ValidationError("Missing required field: name".to_string()))?;
        (body.unwrap_or_default(), Some(n))
    } else {
        let b = body
            .ok_or_else(|| AppError::ValidationError("Missing required field: body".to_string()))?;
        if b.trim().is_empty() {
            return Err(AppError::ValidationError(
                "Body cannot be empty".to_string(),
            ));
        }
        (b, slug)
    };

    if let Some(ref s) = resolved_slug {
        if s.chars().count() > post::SLUG_MAX_LEN {
            return Err(AppError::ValidationError(format!(
                "slug exceeds {}-character limit",
                post::SLUG_MAX_LEN
            )));
        }
    }
    if !post::is_valid_visibility(&visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            visibility
        )));
    }

    Ok(ComposeInput {
        kind: kind.to_string(),
        body: resolved_body,
        slug: resolved_slug,
        visibility,
        category_id,
        media,
    })
}

/// Drain the remaining multipart fields, accumulating `media` (with the
/// per-kind count cap) and `caption_<index>` captions. Used by
/// `append_media` so the parsing logic for media-only requests doesn't
/// have to be repeated.
pub(crate) async fn parse_media_only_multipart(
    multipart: &mut Multipart,
    cap: usize,
) -> AppResult<Vec<PendingMedia>> {
    let mut media: Vec<PendingMedia> = Vec::new();
    let mut captions: HashMap<usize, String> = HashMap::new();
    while let Some(field) = next_field(multipart).await? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "media" {
            consume_media_field(field, &mut media, cap).await?;
        } else if field_name.starts_with("caption_") {
            consume_caption_field(&field_name, field, &mut captions).await;
        }
    }
    apply_captions(&mut media, captions);
    Ok(media)
}

async fn next_field<'a>(multipart: &'a mut Multipart) -> AppResult<Option<Field<'a>>> {
    multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to parse multipart form: {}", e)))
}

async fn read_text(field: Field<'_>, label: &str) -> AppResult<String> {
    field
        .text()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to read {}: {}", label, e)))
}

async fn consume_media_field(
    field: Field<'_>,
    media: &mut Vec<PendingMedia>,
    cap: usize,
) -> AppResult<()> {
    if media.len() >= cap {
        return Err(AppError::ValidationError(format!(
            "At most {} media files per request",
            cap
        )));
    }
    let mime = field.content_type().map(|s| s.to_string()).ok_or_else(|| {
        AppError::ValidationError("Media field must include a Content-Type".to_string())
    })?;
    if !is_allowed_media_mime(&mime) {
        return Err(AppError::ValidationError(format!(
            "Unsupported media type '{}'",
            mime
        )));
    }
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to read media bytes: {}", e)))?;
    let cap_bytes = max_bytes_for_mime(&mime);
    if bytes.len() > cap_bytes {
        return Err(AppError::ValidationError(format!(
            "Media file ({}) exceeds {} byte limit",
            bytes.len(),
            cap_bytes
        )));
    }
    media.push(PendingMedia {
        bytes: bytes.to_vec(),
        mime_type: mime,
        caption: None,
    });
    Ok(())
}

async fn consume_caption_field(
    name: &str,
    field: Field<'_>,
    captions: &mut HashMap<usize, String>,
) {
    if let Ok(idx) = name.trim_start_matches("caption_").parse::<usize>() {
        let text = field.text().await.unwrap_or_default();
        if !text.is_empty() {
            captions.insert(idx, text);
        }
    }
}

fn apply_captions(media: &mut [PendingMedia], captions: HashMap<usize, String>) {
    for (idx, caption) in captions {
        if let Some(m) = media.get_mut(idx) {
            m.caption = Some(caption);
        }
    }
}
