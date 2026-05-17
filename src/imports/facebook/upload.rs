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
//! Archive staging from S3 plus the blocking-thread reader that pulls
//! requested entries out of the local archive. Mime sniffing for FB
//! exports lives alongside because both the staging code and the
//! insertion code need the supported-mime decision.

use crate::s3::S3Service;
use std::io::{Read, Write};
use std::path::Path;

/// Stream the archive bytes from S3 to a local tempfile. The tempfile is
/// auto-deleted when the returned handle is dropped.
pub(crate) async fn stage_archive_to_tempfile(
    s3: &S3Service,
    s3_key: &str,
) -> Result<tempfile::NamedTempFile, anyhow::Error> {
    let (bytes, _content_type) = s3.get_object_at(s3_key).await?;
    let mut file = tempfile::Builder::new()
        .prefix("riposte-fb-import-")
        .suffix(".zip")
        .tempfile()?;
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(file)
}

/// Read the listed entries out of the archive on a blocking thread.
/// Returns `(filename, bytes)` pairs preserving order.
pub(crate) fn read_archive_entries(
    archive_path: &Path,
    uris: &[String],
) -> Result<Vec<(String, Vec<u8>)>, anyhow::Error> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(uris.len());
    for uri in uris {
        // Try the URI verbatim, then fall back to a leading-slash variant.
        // ZIPs may or may not include the leading "your_facebook_activity/"
        // depending on export version.
        let candidates = [uri.clone(), uri.trim_start_matches('/').to_string()];
        let mut bytes: Option<Vec<u8>> = None;
        for candidate in &candidates {
            if let Ok(mut entry) = archive.by_name(candidate) {
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf)?;
                bytes = Some(buf);
                break;
            }
        }
        if let Some(bytes) = bytes {
            let basename = uri.rsplit('/').next().unwrap_or(uri.as_str()).to_string();
            out.push((basename, bytes));
        } else {
            // Missing media is logged but not fatal: the post still imports
            // without that attachment.
            tracing::warn!("facebook import: media not found in archive: {}", uri);
        }
    }
    Ok(out)
}

pub(crate) fn mime_for_filename(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if lower.ends_with(".png") {
        "image/png".to_string()
    } else if lower.ends_with(".gif") {
        "image/gif".to_string()
    } else if lower.ends_with(".webp") {
        "image/webp".to_string()
    } else if lower.ends_with(".mp4") {
        "video/mp4".to_string()
    } else if lower.ends_with(".mov") {
        "video/quicktime".to_string()
    } else {
        "application/octet-stream".to_string()
    }
}

fn is_supported_image_mime(mime: &str) -> bool {
    matches!(
        mime,
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    )
}

/// video mimes the importer is allowed to bring across into S3
/// and post_media. Matches the live upload allowlist in
/// [`crate::posts::media::is_video_mime`]. `.mov` files are rejected
/// because they don't play inline in browsers without transcoding 
/// out-of-scope for the importer.
fn is_supported_video_mime(mime: &str) -> bool {
    matches!(mime, "video/mp4" | "video/webm")
}

pub(crate) fn is_supported_media_mime(mime: &str) -> bool {
    is_supported_image_mime(mime) || is_supported_video_mime(mime)
}
