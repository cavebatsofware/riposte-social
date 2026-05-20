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
//! Cross-kind helpers shared by post and album handlers.
//!
//! Both route surfaces stay distinct so wire formats and URL paths remain
//! separate; the work that's identical between kinds (multipart parsing,
//! S3 upload + DB insert with rollback, kind-checked media manipulation,
//! authorship checks) lives here so both modules call the same code.

pub mod insert;
pub mod parse;
pub mod upload;
pub mod variants;

use crate::entities::post;
use uuid::Uuid;

pub use insert::{
    append_media, commit_compose, delete_media, edit_media_caption, implicit_cover_id,
    load_category, load_owned_post, load_post, reorder_media,
};
pub use parse::{parse_compose_multipart, ComposeInput, PendingMedia};
pub use upload::COMPOSE_BODY_MAX_BYTES;

/// Per-kind media count caps. Posts are tight (a card carries up to 8
/// attachments cleanly); albums hold a full upload batch.
pub(crate) const POST_MEDIA_FILES_MAX: usize = 8;
pub(crate) const ALBUM_MEDIA_FILES_MAX: usize = 50;

/// Per-image-file size cap (10 MiB). Phone-shot photos comfortably fit;
/// edited / RAW exports get rejected before hitting S3.
pub(crate) const IMAGE_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Per-video-file size cap (100 MiB). Family video clips are typically
/// short; transcoded HEVC is even smaller.
pub(crate) const VIDEO_FILE_MAX_BYTES: usize = 100 * 1024 * 1024;

/// Allowlisted image mime types. Browsers render these inline; everything
/// else is rejected so uploaded files cannot become a vector for malicious
/// content disguised as media (`text/html` payloads, SVG with embedded
/// scripts, etc.).
pub(crate) const ALLOWED_IMAGE_MIME_TYPES: &[&str] =
    &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Allowlisted video mime types. Constrained to formats every modern
/// browser plays inline with `<video controls>`.
pub(crate) const ALLOWED_VIDEO_MIME_TYPES: &[&str] = &["video/mp4", "video/webm"];

/// Returns true when `mime` is on either the image or the video allowlist.
pub fn is_allowed_media_mime(mime: &str) -> bool {
    ALLOWED_IMAGE_MIME_TYPES.contains(&mime) || ALLOWED_VIDEO_MIME_TYPES.contains(&mime)
}

/// Returns true when `mime` is a video mime  used to pick the per-file
/// size cap and to dispatch the frontend `<video>` vs `<img>` render.
pub fn is_video_mime(mime: &str) -> bool {
    ALLOWED_VIDEO_MIME_TYPES.contains(&mime)
}

/// Per-file cap depends on the mime: images stay at 10 MiB, videos go up
/// to 100 MiB.
pub fn max_bytes_for_mime(mime: &str) -> usize {
    if is_video_mime(mime) {
        VIDEO_FILE_MAX_BYTES
    } else {
        IMAGE_FILE_MAX_BYTES
    }
}

/// Returns the maximum number of media files allowed in a single
/// compose / append request for the given kind.
pub fn media_files_max_for_kind(kind: &str) -> usize {
    if kind == post::KIND_ALBUM {
        ALBUM_MEDIA_FILES_MAX
    } else {
        POST_MEDIA_FILES_MAX
    }
}

/// Lowercase, human-friendly label for a kind. Used in 404 / 403 messages
/// so a kind-mismatch hit looks like a normal "post not found" / "album
/// not found" rather than disclosing the wrong kind.
pub(crate) fn kind_noun(kind: &str) -> &'static str {
    if kind == post::KIND_ALBUM {
        "album"
    } else {
        "post"
    }
}

/// Title-cased version of `kind_noun` for sentence-leading messages.
pub(crate) fn kind_noun_title(kind: &str) -> &'static str {
    if kind == post::KIND_ALBUM {
        "Album"
    } else {
        "Post"
    }
}

/// Upload-ready media with its allocated row id, S3 key, and ordinal slot.
/// Image-only fields (`width`, `height`, `thumbnail_data`, `icon_data`) are
/// populated by `variants::process_image_variants` between plan-build and
/// S3 upload, and copied into the `post_media` row during DB insert.
pub(crate) struct PlannedUpload {
    pub media_id: Uuid,
    pub s3_key: String,
    pub media: PendingMedia,
    pub ordinal: i32,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub thumbnail_data: Option<Vec<u8>>,
    pub icon_data: Option<Vec<u8>>,
}
