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
//! Facebook archive import worker.
//!
//! The Facebook export ZIP contains a set of `your_posts_*.json` manifests
//! whose entries each describe one post: a unix timestamp, optional body
//! text spread across one or more `data[].post` strings, and 0..N media
//! attachments referenced by ZIP-relative path. We:
//!
//! 1. Stream the uploaded ZIP from S3 to a local tempfile.
//! 2. Walk the archive's central directory, identify post manifests, and
//!    parse them into a flat list of `FacebookPost` items.
//! 3. Dedup against `posts.import_external_id` so re-running an import
//!    skips already-imported rows. The external id is a stable hash over
//!    `(timestamp, body)` plus the first attachment URI when present, so
//!    edits to the FB archive (re-export of the same content) hash
//!    consistently.
//! 4. Process posts with `buffer_unordered` for bounded concurrency. Each
//!    task opens its own `ZipArchive` from the local tempfile (cheap; the
//!    central directory is small) and uploads its media to S3 in series.
//!
//! Mojibake. FB exports double-encode UTF-8 as latin1 for any text field
//! containing non-ASCII characters (curly quotes, em-dashes, accented
//! letters, emoji). `fix_facebook_mojibake` is applied to album name,
//! description, and post body during normalization so the database stores
//! the correctly decoded codepoints.
//!
//! URL formatting. Post bodies arrive as plain text with bare http(s)
//! URLs. `wrap_bare_urls` wraps them in markdown autolink syntax
//! (`<https://...>`) so the social frontend renders them as clickable
//! links rather than literal text.

use crate::entities::{album, album_media, post, post_media};
use crate::imports::{self, JobProgress};
use crate::s3::S3Service;
use blake2::{Blake2b512, Digest};
use chrono::TimeZone;
use futures::stream::{self, StreamExt};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// Bounded post-level concurrency. Each unit does S3 reads + writes plus a
/// short DB transaction; small N keeps us polite to S3 and the DB.
const IMPORT_CONCURRENCY: usize = 4;

/// Heartbeat / progress flush cadence. Low enough that the admin UI sees
/// movement during a long import; high enough that we don't pound the DB
/// after every single item.
const PROGRESS_FLUSH_EVERY: i64 = 5;

/// Per-post body cap. Defends against pathological archives without
/// truncating real posts (FB's own UI limit is ~63K chars).
const BODY_MAX_CHARS: usize = 100_000;

/// Stored verbatim in `import_job.params`. Identifies the source archive
/// and the visibility the operator chose at import time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacebookImportParams {
    /// S3 object key holding the uploaded archive.
    pub s3_key: String,
    /// Visibility tier applied to every imported post. One of
    /// `post::VISIBILITY_PUBLIC` / `_COMMENTERS` / `_POSTERS`.
    pub visibility: String,
    /// Original filename uploaded by the operator (display only).
    pub original_filename: Option<String>,
    /// Byte size of the uploaded archive (display only).
    pub size_bytes: Option<u64>,
}

/// Normalized representation of one FB post extracted from the manifest.
/// `attachment_uris` are paths relative to the ZIP root; the worker
/// resolves them against the archive at import time.
#[derive(Debug, Clone)]
struct FacebookPost {
    /// Stable hash used for dedup against existing imports.
    external_id: String,
    /// Unix seconds. Mapped to `posts.published_at`.
    timestamp: i64,
    /// Markdown body. Empty when the FB post was media-only.
    body: String,
    /// In-archive media references. Each item resolves to a single ZIP
    /// entry the worker will upload to S3.
    attachment_uris: Vec<String>,
}

/// Normalized representation of one FB album manifest extracted from
/// `your_facebook_activity/posts/album/*.json`. Phase 9d: albums are now
/// imported as `albums` entities, NOT synthesized into posts. Each
/// `attachment_uris` resolves to one `album_media` row.
#[derive(Debug, Clone)]
struct FacebookAlbum {
    /// Stable hash used for dedup against existing imports in the
    /// `albums` table.
    external_id: String,
    /// Unix seconds. Mapped to `albums.published_at`.
    timestamp: i64,
    /// Album display name (mapped to `albums.name`). Required: empty-name
    /// FB albums fall back to a synthesized "Untitled album <ts>".
    name: String,
    /// Optional album description (`albums.description`).
    description: Option<String>,
    /// Ordered list of `(uri, optional_caption)` pairs. The current FB
    /// schema has no per-photo caption field, so caption is always None
    /// today; the field is kept for the wire so a future FB export
    /// version with captions can populate it without a re-shape.
    attachments: Vec<(String, Option<String>)>,
}

/// Result of running the worker. Returned to the caller so tests can
/// inspect outcomes without re-querying the DB.
#[derive(Debug, Default)]
pub struct FacebookImportSummary {
    pub total: i64,
    pub succeeded: i64,
    pub skipped: i64,
    pub failed: i64,
}

/// Public worker entry point. Idempotent: re-running against the same
/// archive (or another archive containing the same posts) skips already-
/// imported posts via the `(import_source, import_external_id)` dedup.
///
/// `created_by` is recorded as the post author for every imported row.
/// Per the MVP plan posts are owned by an admin/poster account; importing
/// under another user's id is not supported.
pub async fn run_facebook_import(
    db: DatabaseConnection,
    s3: S3Service,
    job_id: Uuid,
    params: FacebookImportParams,
    created_by: Uuid,
) -> Result<FacebookImportSummary, anyhow::Error> {
    imports::mark_running(&db, job_id).await?;
    let _ = imports::append_log(
        &db,
        job_id,
        imports::LOG_LEVEL_INFO,
        "Worker started",
        Some(serde_json::json!({
            "s3_key": params.s3_key,
            "visibility": params.visibility,
            "original_filename": params.original_filename,
            "size_bytes": params.size_bytes,
        })),
    )
    .await;

    // Pull the archive from S3 to a local tempfile so the zip crate (which
    // expects `Read + Seek`) can walk the central directory cheaply, and
    // so per-post tasks can each open their own `ZipArchive` view of the
    // same file without holding the bytes in memory.
    let staged = match stage_archive_to_tempfile(&s3, &params.s3_key).await {
        Ok(s) => s,
        Err(e) => {
            // `{:#}` walks the anyhow cause chain so the admin UI sees
            // the underlying network/credential reason, not just the
            // SDK's top-level "dispatch failure" wrapper.
            let _ = imports::append_log(
                &db,
                job_id,
                imports::LOG_LEVEL_ERROR,
                format!("Failed to stage archive from S3: {:#}", e),
                None,
            )
            .await;
            return Err(e);
        }
    };
    let archive_path = staged.path().to_path_buf();
    let _ = imports::append_log(
        &db,
        job_id,
        imports::LOG_LEVEL_INFO,
        "Archive staged to local tempfile",
        Some(serde_json::json!({
            "path_bytes": staged.path().to_string_lossy().len(),
        })),
    )
    .await;

    let parse_outcome = match tokio::task::spawn_blocking({
        let path = archive_path.clone();
        move || parse_archive(&path)
    })
    .await
    .map_err(|e| anyhow::anyhow!("zip parse task join failed: {}", e))?
    {
        Ok(p) => p,
        Err(e) => {
            let _ = imports::append_log(
                &db,
                job_id,
                imports::LOG_LEVEL_ERROR,
                format!("Failed to parse archive: {:#}", e),
                None,
            )
            .await;
            return Err(e);
        }
    };

    // Flush the events the blocking parser collected (per-manifest summaries,
    // dedup count, etc.) into the job log in order.
    for ev in parse_outcome.log_events {
        let _ = imports::append_log(&db, job_id, ev.level, ev.msg, ev.ctx).await;
    }
    let posts = parse_outcome.posts;
    let albums = parse_outcome.albums;

    let _ = imports::append_log(
        &db,
        job_id,
        imports::LOG_LEVEL_INFO,
        format!(
            "Parsed {} candidate post(s) and {} album(s) from archive",
            posts.len(),
            albums.len()
        ),
        None,
    )
    .await;

    // Total counts both posts and albums; each contributes one unit of
    // work to the progress meter.
    let mut progress = JobProgress {
        total: (posts.len() + albums.len()) as i64,
        ..Default::default()
    };
    imports::update_progress(&db, job_id, &progress).await?;

    // Posts dedup: query the `posts` table for matching import keys.
    let post_candidate_ids: Vec<String> = posts.iter().map(|p| p.external_id.clone()).collect();
    let posts_already: HashSet<String> = if post_candidate_ids.is_empty() {
        HashSet::new()
    } else {
        post::Entity::find()
            .filter(post::Column::ImportSource.eq("facebook"))
            .filter(post::Column::ImportExternalId.is_in(post_candidate_ids))
            .all(&db)
            .await?
            .into_iter()
            .filter_map(|p| p.import_external_id)
            .collect()
    };
    if !posts_already.is_empty() {
        let _ = imports::append_log(
            &db,
            job_id,
            imports::LOG_LEVEL_INFO,
            format!(
                "{} of {} candidate posts already imported; will be skipped",
                posts_already.len(),
                posts.len()
            ),
            None,
        )
        .await;
    }

    // Albums dedup: separate table, separate import-key query.
    let album_candidate_ids: Vec<String> = albums.iter().map(|a| a.external_id.clone()).collect();
    let albums_already: HashSet<String> = if album_candidate_ids.is_empty() {
        HashSet::new()
    } else {
        album::Entity::find()
            .filter(album::Column::ImportSource.eq("facebook"))
            .filter(album::Column::ImportExternalId.is_in(album_candidate_ids))
            .all(&db)
            .await?
            .into_iter()
            .filter_map(|a| a.import_external_id)
            .collect()
    };
    if !albums_already.is_empty() {
        let _ = imports::append_log(
            &db,
            job_id,
            imports::LOG_LEVEL_INFO,
            format!(
                "{} of {} candidate albums already imported; will be skipped",
                albums_already.len(),
                albums.len()
            ),
            None,
        )
        .await;
    }

    // Counters shared across the parallel stream so each task reports its
    // outcome without coordinating through a Mutex.
    let succeeded = Arc::new(AtomicI64::new(0));
    let skipped = Arc::new(AtomicI64::new(0));
    let failed = Arc::new(AtomicI64::new(0));
    let processed = Arc::new(AtomicI64::new(0));

    let visibility = params.visibility.clone();
    let archive_path_arc = Arc::new(archive_path);
    let posts_already_arc = Arc::new(posts_already);
    let albums_already_arc = Arc::new(albums_already);

    // ----- Posts pass -----
    let posts_stream = stream::iter(posts.into_iter().map(|p| {
        let db = db.clone();
        let s3 = s3.clone();
        let visibility = visibility.clone();
        let archive_path = archive_path_arc.clone();
        let already = posts_already_arc.clone();
        async move {
            if already.contains(&p.external_id) {
                return ItemOutcome::Skipped;
            }
            match import_one_post(
                &db,
                &s3,
                archive_path.as_path(),
                &p,
                &visibility,
                created_by,
            )
            .await
            {
                Ok(()) => ItemOutcome::Succeeded,
                Err(e) => {
                    tracing::warn!("facebook import: post {} failed: {}", &p.external_id, e);
                    let _ = imports::append_log(
                        &db,
                        job_id,
                        imports::LOG_LEVEL_ERROR,
                        format!("Post import failed: {:#}", e),
                        Some(serde_json::json!({
                            "external_id": p.external_id,
                            "timestamp": p.timestamp,
                            "media_count": p.attachment_uris.len(),
                        })),
                    )
                    .await;
                    ItemOutcome::Failed
                }
            }
        }
    }))
    .buffer_unordered(IMPORT_CONCURRENCY);

    tokio::pin!(posts_stream);

    while let Some(outcome) = posts_stream.next().await {
        match outcome {
            ItemOutcome::Succeeded => {
                succeeded.fetch_add(1, Ordering::Relaxed);
            }
            ItemOutcome::Skipped => {
                skipped.fetch_add(1, Ordering::Relaxed);
            }
            ItemOutcome::Failed => {
                failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if done % PROGRESS_FLUSH_EVERY == 0 {
            progress.processed = done;
            progress.succeeded = succeeded.load(Ordering::Relaxed);
            progress.skipped = skipped.load(Ordering::Relaxed);
            progress.failed = failed.load(Ordering::Relaxed);
            // Best-effort progress; don't fail the run because the DB
            // hiccupped on a counter update.
            let _ = imports::update_progress(&db, job_id, &progress).await;
            let _ = imports::touch_heartbeat(&db, job_id).await;
        }
    }

    // ----- Albums pass (Phase 9d) -----
    let albums_stream = stream::iter(albums.into_iter().map(|a| {
        let db = db.clone();
        let s3 = s3.clone();
        let visibility = visibility.clone();
        let archive_path = archive_path_arc.clone();
        let already = albums_already_arc.clone();
        async move {
            if already.contains(&a.external_id) {
                return ItemOutcome::Skipped;
            }
            match import_one_album(
                &db,
                &s3,
                archive_path.as_path(),
                &a,
                &visibility,
                created_by,
            )
            .await
            {
                Ok(()) => ItemOutcome::Succeeded,
                Err(e) => {
                    tracing::warn!("facebook import: album {} failed: {}", &a.external_id, e);
                    let _ = imports::append_log(
                        &db,
                        job_id,
                        imports::LOG_LEVEL_ERROR,
                        format!("Album import failed: {:#}", e),
                        Some(serde_json::json!({
                            "external_id": a.external_id,
                            "timestamp": a.timestamp,
                            "name": a.name,
                            "photo_count": a.attachments.len(),
                        })),
                    )
                    .await;
                    ItemOutcome::Failed
                }
            }
        }
    }))
    .buffer_unordered(IMPORT_CONCURRENCY);

    tokio::pin!(albums_stream);

    while let Some(outcome) = albums_stream.next().await {
        match outcome {
            ItemOutcome::Succeeded => {
                succeeded.fetch_add(1, Ordering::Relaxed);
            }
            ItemOutcome::Skipped => {
                skipped.fetch_add(1, Ordering::Relaxed);
            }
            ItemOutcome::Failed => {
                failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        let done = processed.fetch_add(1, Ordering::Relaxed) + 1;
        if done % PROGRESS_FLUSH_EVERY == 0 {
            progress.processed = done;
            progress.succeeded = succeeded.load(Ordering::Relaxed);
            progress.skipped = skipped.load(Ordering::Relaxed);
            progress.failed = failed.load(Ordering::Relaxed);
            let _ = imports::update_progress(&db, job_id, &progress).await;
            let _ = imports::touch_heartbeat(&db, job_id).await;
        }
    }

    // Final flush so the admin UI sees an accurate count even on a tiny
    // archive that never crossed the every-N flush threshold.
    progress.processed = processed.load(Ordering::Relaxed);
    progress.succeeded = succeeded.load(Ordering::Relaxed);
    progress.skipped = skipped.load(Ordering::Relaxed);
    progress.failed = failed.load(Ordering::Relaxed);
    imports::update_progress(&db, job_id, &progress).await?;

    let _ = imports::append_log(
        &db,
        job_id,
        imports::LOG_LEVEL_INFO,
        format!(
            "Worker finished: {} succeeded, {} skipped, {} failed (of {} total)",
            progress.succeeded, progress.skipped, progress.failed, progress.total
        ),
        None,
    )
    .await;

    Ok(FacebookImportSummary {
        total: progress.total,
        succeeded: progress.succeeded,
        skipped: progress.skipped,
        failed: progress.failed,
    })
}

#[derive(Debug, Clone, Copy)]
enum ItemOutcome {
    Succeeded,
    Skipped,
    Failed,
}

/// Stream the archive bytes from S3 to a local tempfile. The tempfile is
/// auto-deleted when the returned handle is dropped.
async fn stage_archive_to_tempfile(
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

/// Bundled result from `parse_archive`: the flat post list, the album
/// list, plus a vec of log events that the async caller flushes via
/// `append_log` after the blocking thread returns. Avoids passing a DB
/// handle into the blocking closure.
#[derive(Default)]
struct ParseOutcome {
    posts: Vec<FacebookPost>,
    albums: Vec<FacebookAlbum>,
    log_events: Vec<ParseLogEvent>,
}

struct ParseLogEvent {
    level: &'static str,
    msg: String,
    ctx: Option<serde_json::Value>,
}

/// Walk the archive central directory, find every post manifest and album
/// manifest, parse them, and return a single flat list. Manifest decoding
/// errors are recorded as warn events and skipped per-file rather than
/// failing the whole import; this keeps a malformed sibling from blocking
/// the rest. Operates on a blocking-thread file handle; the caller wraps
/// in `spawn_blocking`.
///
/// Returns regular posts first, then album posts. After the two passes we
/// apply an "albums win" rule: regular posts whose body is empty and whose
/// only attachments are URIs already claimed by an album are dropped, so
/// auto-generated "added a new photo." entries don't double up the gallery
/// they came from.
fn parse_archive(path: &Path) -> Result<ParseOutcome, anyhow::Error> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut post_manifest_names: Vec<String> = Vec::new();
    let mut album_manifest_names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if is_post_manifest(&name) {
            post_manifest_names.push(name);
        } else if is_album_manifest(&name) {
            album_manifest_names.push(name);
        }
    }

    let mut log_events: Vec<ParseLogEvent> = Vec::new();
    log_events.push(ParseLogEvent {
        level: imports::LOG_LEVEL_INFO,
        msg: format!(
            "Found {} post manifest(s) and {} album manifest(s) in archive",
            post_manifest_names.len(),
            album_manifest_names.len()
        ),
        ctx: Some(serde_json::json!({
            "post_manifests": post_manifest_names.clone(),
            "album_manifests": album_manifest_names.clone(),
        })),
    });

    let mut regular_posts: Vec<FacebookPost> = Vec::new();
    for name in post_manifest_names {
        let before = regular_posts.len();
        let mut entry = archive.by_name(&name)?;
        let mut json = String::new();
        entry.read_to_string(&mut json)?;
        match serde_json::from_str::<Vec<RawFacebookPost>>(&json) {
            Ok(parsed) => {
                let raw_count = parsed.len();
                for raw in parsed {
                    if let Some(post) = normalize_post(raw) {
                        regular_posts.push(post);
                    }
                }
                log_events.push(ParseLogEvent {
                    level: imports::LOG_LEVEL_INFO,
                    msg: format!(
                        "Parsed post manifest {}: {} of {} entries normalized",
                        name,
                        regular_posts.len() - before,
                        raw_count
                    ),
                    ctx: None,
                });
            }
            Err(e) => {
                log_events.push(ParseLogEvent {
                    level: imports::LOG_LEVEL_WARN,
                    msg: format!(
                        "Skipping {} (matched post-manifest pattern but failed to decode as post list: {})",
                        name, e
                    ),
                    ctx: None,
                });
            }
        }
    }

    let mut albums: Vec<FacebookAlbum> = Vec::new();
    for name in album_manifest_names {
        let mut entry = archive.by_name(&name)?;
        let mut json = String::new();
        entry.read_to_string(&mut json)?;
        match serde_json::from_str::<RawAlbum>(&json) {
            Ok(raw) => {
                let album_name = raw.name.clone();
                let photo_count = raw.photos.len();
                if let Some(parsed_album) = normalize_album(raw) {
                    albums.push(parsed_album);
                    log_events.push(ParseLogEvent {
                        level: imports::LOG_LEVEL_INFO,
                        msg: format!(
                            "Parsed album manifest {}: '{}' with {} photo(s)",
                            name,
                            album_name.as_deref().unwrap_or("(unnamed)"),
                            photo_count
                        ),
                        ctx: None,
                    });
                } else {
                    log_events.push(ParseLogEvent {
                        level: imports::LOG_LEVEL_WARN,
                        msg: format!(
                            "Album {} produced no importable album (no photos or no timestamps)",
                            name
                        ),
                        ctx: None,
                    });
                }
            }
            Err(e) => {
                log_events.push(ParseLogEvent {
                    level: imports::LOG_LEVEL_WARN,
                    msg: format!(
                        "Skipping {} (matched album-manifest pattern but failed to decode as album dict: {})",
                        name, e
                    ),
                    ctx: None,
                });
            }
        }
    }

    // Albums-win dedup: any regular post that contributes no body text and
    // whose attachment URIs are entirely claimed by an album is suppressed
    // so the same image doesn't appear twice on the timeline.
    let album_uris: HashSet<String> = albums
        .iter()
        .flat_map(|a| a.attachments.iter().map(|(u, _)| u.clone()))
        .collect();
    let mut suppressed = 0usize;
    regular_posts.retain(|p| {
        if p.body.is_empty()
            && !p.attachment_uris.is_empty()
            && p.attachment_uris.iter().all(|u| album_uris.contains(u))
        {
            suppressed += 1;
            false
        } else {
            true
        }
    });
    if suppressed > 0 {
        log_events.push(ParseLogEvent {
            level: imports::LOG_LEVEL_INFO,
            msg: format!(
                "Albums-win dedup suppressed {} body-less regular post(s) whose media is owned by an album",
                suppressed
            ),
            ctx: None,
        });
    }

    Ok(ParseOutcome {
        posts: regular_posts,
        albums,
        log_events,
    })
}

/// Accept any post-list manifest under the archive. FB names these
/// inconsistently across export versions:
/// - older exports: `your_posts_1.json`
/// - newer exports: `your_posts__check_ins__photos_and_videos_1.json`
///
/// We require the basename to start with `your_posts_` AND end with
/// `_<digits>.json`. That admits both shapes and rejects siblings like
/// `your_posts_check_ins.json` (no digit suffix), `shared_memories.json`
/// (wrong prefix), `your_uncategorized_photos.json` (wrong prefix). If a
/// future archive surfaces a new shape under the same prefix, the JSON
/// decode fallback in `parse_archive` skips it with a warning rather than
/// failing the whole import.
fn is_post_manifest(name: &str) -> bool {
    let basename = name.rsplit('/').next().unwrap_or(name);
    if !basename.starts_with("your_posts_") || !basename.ends_with(".json") {
        return false;
    }
    let stem = &basename[..basename.len() - ".json".len()];
    // Locate the final `_` and require everything after it to be digits.
    let Some(last_us) = stem.rfind('_') else {
        return false;
    };
    let suffix = &stem[last_us + 1..];
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
}

/// Album manifests live at `posts/album/<basename>.json` (one album per
/// file, dict shape, not a list of posts). FB emits them with numeric
/// basenames in this archive (`0.json`, `1.json`, ...) but we accept any
/// `.json` under that directory and let the decode fallback skip
/// non-conforming files.
fn is_album_manifest(name: &str) -> bool {
    let lower = name; // paths are already lowercase by FB convention
    if !lower.ends_with(".json") {
        return false;
    }
    // Match `<...>/posts/album/<file>.json` at any depth.
    if let Some(idx) = lower.find("posts/album/") {
        let rest = &lower[idx + "posts/album/".len()..];
        return !rest.is_empty() && !rest.contains('/');
    }
    false
}

#[derive(Debug, Deserialize)]
struct RawFacebookPost {
    #[serde(default)]
    timestamp: Option<i64>,
    #[serde(default)]
    data: Vec<RawDataEntry>,
    #[serde(default)]
    attachments: Vec<RawAttachmentBlock>,
    // FB's `title` field carries auto-generated activity descriptions like
    // "X updated his status." which are noise rather than content. We
    // intentionally do not deserialize it; the parser ignores any "title"
    // key in the input JSON.
}

#[derive(Debug, Deserialize)]
struct RawDataEntry {
    #[serde(default)]
    post: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAttachmentBlock {
    #[serde(default)]
    data: Vec<RawAttachmentItem>,
}

#[derive(Debug, Deserialize)]
struct RawAttachmentItem {
    #[serde(default)]
    media: Option<RawMedia>,
}

#[derive(Debug, Deserialize)]
struct RawMedia {
    #[serde(default)]
    uri: Option<String>,
}

/// FB album manifest shape (`posts/album/<n>.json`). One album per file,
/// dict-shaped, not a list of posts.
#[derive(Debug, Deserialize)]
struct RawAlbum {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    photos: Vec<RawAlbumPhoto>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    last_modified_timestamp: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawAlbumPhoto {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    creation_timestamp: Option<i64>,
}

/// Normalize one album manifest into a `FacebookAlbum` (Phase 9d).
/// Albums are now imported as `albums` rows with `album_media` items —
/// NOT synthesized as `posts` with all photos crammed into the body. The
/// timestamp prefers the earliest `creation_timestamp` across the photos
/// so re-exports don't shift the album date when an album is touched in
/// FB; `last_modified_timestamp` is a fallback when no photos carry a
/// creation timestamp.
fn normalize_album(raw: RawAlbum) -> Option<FacebookAlbum> {
    let attachments: Vec<(String, Option<String>)> = raw
        .photos
        .iter()
        .filter_map(|p| p.uri.as_ref().map(|u| (u.clone(), None)))
        .collect();
    if attachments.is_empty() {
        return None;
    }

    let earliest_creation = raw.photos.iter().filter_map(|p| p.creation_timestamp).min();
    let timestamp = earliest_creation
        .or(raw.last_modified_timestamp)
        // No timestamps anywhere — drop. The album entity requires a
        // published_at and we have no defensible value to set.
        ?;

    let raw_name = raw
        .name
        .map(|s| fix_facebook_mojibake(s.trim()).into_owned())
        .filter(|s| !s.is_empty());
    // FB albums often lack a name; synthesize a stable placeholder so we
    // never fall back on storing an empty string in `albums.name`.
    let name = raw_name.unwrap_or_else(|| format!("Untitled album {}", timestamp));

    let description = raw
        .description
        .map(|s| fix_facebook_mojibake(s.trim()).into_owned())
        .filter(|s| !s.is_empty());

    // External id is hashed over (timestamp, name, first uri) so the same
    // album re-exported produces the same id even if the description is
    // touched later.
    let first_uri = attachments.first().map(|(u, _)| u.as_str());
    let external_id = compute_external_id(timestamp, &name, first_uri);
    Some(FacebookAlbum {
        external_id,
        timestamp,
        name,
        description,
        attachments,
    })
}

fn normalize_post(raw: RawFacebookPost) -> Option<FacebookPost> {
    let timestamp = raw.timestamp?;
    // Concatenate body text from each data entry; FB sometimes splits a
    // post across multiple entries (e.g. text + a "feeling" annotation).
    // We deliberately do NOT fall back to `raw.title`: FB's auto-generated
    // titles ("X updated his status.", "X added a new photo.") are activity
    // descriptions, not content. A post with no `data[].post` text and no
    // attachments is dropped further down.
    let body_parts: Vec<String> = raw
        .data
        .into_iter()
        .filter_map(|d| d.post)
        .map(|s| {
            let fixed = fix_facebook_mojibake(s.trim());
            match wrap_bare_urls(fixed.as_ref()) {
                Cow::Owned(wrapped) => wrapped,
                Cow::Borrowed(_) => fixed.into_owned(),
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    let mut body = body_parts.join("\n\n");
    if body.chars().count() > BODY_MAX_CHARS {
        body = body.chars().take(BODY_MAX_CHARS).collect::<String>() + "\n\n[truncated]";
    }

    let attachment_uris: Vec<String> = raw
        .attachments
        .into_iter()
        .flat_map(|b| b.data.into_iter())
        .filter_map(|a| a.media.and_then(|m| m.uri))
        .collect();

    // A media-only post with no body should still import (the media is the
    // content). A post with neither body nor media is dropped.
    if body.is_empty() && attachment_uris.is_empty() {
        return None;
    }

    let external_id = compute_external_id(
        timestamp,
        &body,
        attachment_uris.first().map(|s| s.as_str()),
    );
    Some(FacebookPost {
        external_id,
        timestamp,
        body,
        attachment_uris,
    })
}

/// Best-effort fixup for Facebook's latin1-as-UTF8 double-encoding.
///
/// FB's exporter JSON-stringifies UTF-8 byte sequences as latin1
/// codepoints, so each byte of a multi-byte UTF-8 sequence shows up as
/// its own `\u00XX` escape. Examples:
/// - `«` (U+00AB, UTF-8 `C2 AB`) lands as `Â«` (U+00C2 U+00AB).
/// - `'` (U+2019, UTF-8 `E2 80 99`) lands as three chars `â\u{0080}\u{0099}`.
/// - 🦢 (U+1F9A2, UTF-8 `F0 9F A6 A2`) lands as four chars.
///
/// Fix:
/// 1. Quick-reject strings with no chars in the latin-1 high-byte range
///    (U+0080..=U+00FF). Those can't be hiding misinterpreted UTF-8 since
///    every multi-byte UTF-8 leader (C2-DF, E0-EF, F0-F4) and every
///    continuation byte (80-BF) lands in that range when reinterpreted
///    as a codepoint.
/// 2. Re-collect `s.chars()` as latin1 bytes; if any char is ≥ 256 the
///    string contains genuine multi-codepoint UTF-8 mixed in, so we
///    leave it alone.
/// 3. Decode those bytes as UTF-8. Accept the decoded result only if it
///    has strictly fewer chars than the input (the fixup collapsed at
///    least one multi-byte sequence into one codepoint). Otherwise
///    return the input untouched.
///
/// The strict "fewer chars" gate prevents false positives: a clean ASCII
/// string with a stray `Â` (someone literally typed it) re-decodes to
/// the same length and is left alone.
pub(crate) fn fix_facebook_mojibake(s: &str) -> Cow<'_, str> {
    if !s.chars().any(|c| ('\u{0080}'..='\u{00FF}').contains(&c)) {
        return Cow::Borrowed(s);
    }
    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let code = ch as u32;
        if code >= 256 {
            return Cow::Borrowed(s);
        }
        bytes.push(code as u8);
    }
    let original_chars = s.chars().count();
    match std::str::from_utf8(&bytes) {
        Ok(decoded) if decoded.chars().count() < original_chars => Cow::Owned(decoded.to_string()),
        _ => Cow::Borrowed(s),
    }
}

/// Wraps bare http(s) URLs in markdown autolink syntax `<...>`. FB
/// exports store URLs as plain text; without the wrap a markdown
/// renderer either silently shows them as text or relies on a
/// non-standard autolink extension. The autolink form is canonical
/// markdown and works in every renderer.
///
/// Sentence punctuation (`.,;!?`) at the tail of a captured URL is
/// always peeled off so "visit https://example.com." becomes
/// "visit <https://example.com>.". Closing brackets (`)` `]`) are
/// peeled only when their tally exceeds the matching opener inside
/// the captured URL, so "(see https://example.com)" trims the trailing
/// `)` while a Wikipedia-style permalink such as
/// "https://en.wikipedia.org/wiki/Foo_(mathematics)" keeps it.
pub(crate) fn wrap_bare_urls(s: &str) -> Cow<'_, str> {
    use std::sync::OnceLock;
    static URL_RE: OnceLock<regex::Regex> = OnceLock::new();
    let re =
        URL_RE.get_or_init(|| regex::Regex::new(r"https?://[^\s<>]+").expect("valid url regex"));
    if !re.is_match(s) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        re.replace_all(s, |caps: &regex::Captures<'_>| {
            let raw = &caps[0];
            let trimmed = trim_url_punctuation(raw);
            let trail = &raw[trimmed.len()..];
            format!("<{trimmed}>{trail}")
        })
        .into_owned(),
    )
}

/// Walks back from the end of `raw` peeling off characters that are
/// almost certainly surrounding-text punctuation rather than URL
/// content. Sentence punctuation is unconditional. A trailing `)` or
/// `]` is peeled only when the remaining URL has more closers than
/// matching openers, so URLs whose path legitimately ends in a
/// bracket (Wikipedia, MDN's `Array.prototype[Symbol.iterator]`, etc.)
/// keep that final character.
fn trim_url_punctuation(raw: &str) -> &str {
    // Pre-count brackets in one pass so the trimming loop stays linear:
    // each peel updates the running counter for the closer it removes
    // instead of re-scanning the prefix.
    let (mut opens_p, mut closes_p, mut opens_b, mut closes_b) = (0usize, 0usize, 0usize, 0usize);
    for c in raw.chars() {
        match c {
            '(' => opens_p += 1,
            ')' => closes_p += 1,
            '[' => opens_b += 1,
            ']' => closes_b += 1,
            _ => {}
        }
    }
    let mut end = raw.len();
    while let Some(last) = raw[..end].chars().next_back() {
        let strip = match last {
            '.' | ',' | ';' | '!' | '?' => true,
            ')' => closes_p > opens_p,
            ']' => closes_b > opens_b,
            _ => false,
        };
        if !strip {
            break;
        }
        match last {
            ')' => closes_p -= 1,
            ']' => closes_b -= 1,
            _ => {}
        }
        end -= last.len_utf8();
    }
    &raw[..end]
}

/// Stable dedup key. We can't use FB's internal post ids (the export does
/// not include them), so we hash the content. Including the first
/// attachment URI keeps two same-day posts with different media from
/// colliding on identical text bodies.
fn compute_external_id(timestamp: i64, body: &str, first_uri: Option<&str>) -> String {
    let mut hasher = Blake2b512::new();
    hasher.update(timestamp.to_le_bytes());
    hasher.update(b"\x00");
    hasher.update(body.as_bytes());
    hasher.update(b"\x00");
    if let Some(uri) = first_uri {
        hasher.update(uri.as_bytes());
    }
    let digest = hasher.finalize();
    // Truncate to 32 hex chars; collision probability remains negligible
    // and keeps the column compact.
    hex::encode(&digest[..16])
}

/// Process one post: read each attachment from the local archive, upload
/// to S3, and write the post + post_media rows in a single transaction.
async fn import_one_post(
    db: &DatabaseConnection,
    s3: &S3Service,
    archive_path: &Path,
    fb_post: &FacebookPost,
    visibility: &str,
    created_by: Uuid,
) -> Result<(), anyhow::Error> {
    let post_id = Uuid::new_v4();

    // Pre-stage media: read all bytes off the local zip on a blocking
    // thread, then upload to S3 in series. Sequential within a post is
    // fine; per-post fan-out via `buffer_unordered` upstream provides the
    // overall concurrency.
    let staged_media = {
        let archive_path = archive_path.to_path_buf();
        let uris = fb_post.attachment_uris.clone();
        tokio::task::spawn_blocking(move || read_archive_entries(&archive_path, &uris))
            .await
            .map_err(|e| anyhow::anyhow!("media read task join failed: {}", e))??
    };

    // Defense against importing empty posts: if every staged item has an
    // unsupported mime (e.g. archive contains only mp4 attachments) AND
    // the body is empty, there is nothing to render. Skip cleanly rather
    // than insert a content-empty row. The caller counts this as
    // succeeded (we processed it) — surfacing a separate "dropped"
    // counter is a future polish.
    let any_supported = staged_media
        .iter()
        .any(|(filename, _)| is_supported_media_mime(&mime_for_filename(filename)));
    if fb_post.body.is_empty() && !any_supported {
        return Ok(());
    }

    let mut uploaded: Vec<(Uuid, String, String)> = Vec::with_capacity(staged_media.len());
    for (i, (filename, bytes)) in staged_media.into_iter().enumerate() {
        let media_id = Uuid::new_v4();
        let mime = mime_for_filename(&filename);
        // Phase 9c: images and videos are both supported now. Other types
        // (`.mov` / unknown) are skipped rather than failing the whole post.
        if !is_supported_media_mime(&mime) {
            continue;
        }
        let key = format!("posts/{}/{}", post_id, media_id);
        if let Err(e) = s3.put_object_at(&key, bytes, &mime).await {
            // Roll back any uploads already committed for this post so we
            // don't leave orphan objects when the DB transaction below
            // never runs.
            for (_, prior_key, _) in &uploaded {
                let _ = s3.delete_object_at(prior_key).await;
            }
            return Err(anyhow::anyhow!("failed to upload media #{}: {}", i, e));
        }
        uploaded.push((media_id, key, mime));
    }

    let txn_result: Result<(), sea_orm::DbErr> = async {
        let txn = db.begin().await?;
        let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
        let published = chrono::Utc
            .timestamp_opt(fb_post.timestamp, 0)
            .single()
            .unwrap_or_else(chrono::Utc::now);
        post::ActiveModel {
            id: Set(post_id),
            author_id: Set(created_by),
            body: Set(fb_post.body.clone()),
            visibility: Set(visibility.to_string()),
            published_at: Set(published.into()),
            import_source: Set(Some("facebook".to_string())),
            import_external_id: Set(Some(fb_post.external_id.clone())),
            deleted_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            category_id: Set(None),
            content_lang: Set(post::CONTENT_LANG_ENGLISH.to_string()),
            kind: Set(post::KIND_POST.to_string()),
            slug: Set(None),
        }
        .insert(&txn)
        .await?;

        for (i, (media_id, key, mime)) in uploaded.iter().enumerate() {
            post_media::ActiveModel {
                id: Set(*media_id),
                post_id: Set(post_id),
                s3_key: Set(key.clone()),
                mime_type: Set(mime.clone()),
                width: Set(None),
                height: Set(None),
                ordinal: Set(i as i32),
                caption: Set(None),
                created_at: Set(now),
            }
            .insert(&txn)
            .await?;
        }

        txn.commit().await
    }
    .await;

    if let Err(e) = txn_result {
        for (_, key, _) in &uploaded {
            let _ = s3.delete_object_at(key).await;
        }
        return Err(anyhow::anyhow!("DB write failed: {}", e));
    }

    Ok(())
}

/// Phase 9d: import one FB album as an `albums` row plus an ordered
/// set of `album_media` rows. Mirrors `import_one_post` structurally —
/// stage media off the zip, upload to S3, then insert in a transaction.
/// On any failure after S3 upload, all uploaded objects are best-effort
/// deleted so a failed run doesn't leave orphans.
async fn import_one_album(
    db: &DatabaseConnection,
    s3: &S3Service,
    archive_path: &Path,
    fb_album: &FacebookAlbum,
    visibility: &str,
    created_by: Uuid,
) -> Result<(), anyhow::Error> {
    let album_id = Uuid::new_v4();

    let uris: Vec<String> = fb_album
        .attachments
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    let staged_media = {
        let archive_path = archive_path.to_path_buf();
        let uris = uris.clone();
        tokio::task::spawn_blocking(move || read_archive_entries(&archive_path, &uris))
            .await
            .map_err(|e| anyhow::anyhow!("media read task join failed: {}", e))??
    };

    // Drop unsupported mimes (e.g. .mov). If nothing supported survives,
    // there's nothing to import. Album-with-only-unsupported-media is a
    // legitimate skip-without-failure.
    let supported_indices: Vec<usize> = staged_media
        .iter()
        .enumerate()
        .filter_map(|(i, (filename, _))| {
            if is_supported_media_mime(&mime_for_filename(filename)) {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    if supported_indices.is_empty() {
        return Ok(());
    }

    let captions: Vec<Option<String>> = fb_album
        .attachments
        .iter()
        .map(|(_, c)| c.clone())
        .collect();

    // Upload supported items to S3 in series. Each (media_id, s3_key,
    // mime, original_index, caption) entry is queued for the DB insert.
    let mut uploaded: Vec<(Uuid, String, String, Option<String>)> =
        Vec::with_capacity(supported_indices.len());
    for (out_idx, src_idx) in supported_indices.iter().enumerate() {
        let (filename, bytes) = &staged_media[*src_idx];
        let media_id = Uuid::new_v4();
        let mime = mime_for_filename(filename);
        let key = format!("albums/{}/{}", album_id, media_id);
        if let Err(e) = s3.put_object_at(&key, bytes.clone(), &mime).await {
            for (_, prior_key, _, _) in &uploaded {
                let _ = s3.delete_object_at(prior_key).await;
            }
            return Err(anyhow::anyhow!(
                "failed to upload media #{}: {}",
                out_idx,
                e
            ));
        }
        uploaded.push((
            media_id,
            key,
            mime,
            captions.get(*src_idx).cloned().flatten(),
        ));
    }

    let cover_media_id = uploaded.first().map(|(id, _, _, _)| *id);

    let txn_result: Result<(), sea_orm::DbErr> = async {
        let txn = db.begin().await?;
        let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
        let published = chrono::Utc
            .timestamp_opt(fb_album.timestamp, 0)
            .single()
            .unwrap_or_else(chrono::Utc::now);

        album::ActiveModel {
            id: Set(album_id),
            author_id: Set(created_by),
            name: Set(fb_album.name.clone()),
            description: Set(fb_album.description.clone()),
            cover_media_id: Set(cover_media_id),
            visibility: Set(visibility.to_string()),
            published_at: Set(published.into()),
            import_source: Set(Some("facebook".to_string())),
            import_external_id: Set(Some(fb_album.external_id.clone())),
            deleted_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            category_id: Set(None),
        }
        .insert(&txn)
        .await?;

        for (i, (media_id, key, mime, caption)) in uploaded.iter().enumerate() {
            album_media::ActiveModel {
                id: Set(*media_id),
                album_id: Set(album_id),
                s3_key: Set(key.clone()),
                mime_type: Set(mime.clone()),
                width: Set(None),
                height: Set(None),
                ordinal: Set(i as i32),
                caption: Set(caption.clone()),
                created_at: Set(now),
            }
            .insert(&txn)
            .await?;
        }

        txn.commit().await
    }
    .await;

    if let Err(e) = txn_result {
        for (_, key, _, _) in &uploaded {
            let _ = s3.delete_object_at(key).await;
        }
        return Err(anyhow::anyhow!("DB write failed: {}", e));
    }

    Ok(())
}

/// Read the listed entries out of the archive on a blocking thread.
/// Returns `(filename, bytes)` pairs preserving order.
fn read_archive_entries(
    archive_path: &Path,
    uris: &[String],
) -> Result<Vec<(String, Vec<u8>)>, anyhow::Error> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(uris.len());
    for uri in uris {
        // Try the URI verbatim, then fall back to a leading-slash
        // variant. ZIPs may or may not include the leading "your_facebook_
        // activity/" depending on export version.
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
            // Missing media is logged but not fatal: the post still
            // imports without that attachment.
            tracing::warn!("facebook import: media not found in archive: {}", uri);
        }
    }
    Ok(out)
}

fn mime_for_filename(name: &str) -> String {
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

/// Phase 9c: video mimes the importer is allowed to bring across into S3
/// and post_media. Matches the live upload allowlist in
/// [`crate::posts::routes::is_video_mime`]. `.mov` files are rejected
/// because they don't play inline in browsers without transcoding —
/// out-of-scope for the importer.
fn is_supported_video_mime(mime: &str) -> bool {
    matches!(mime, "video/mp4" | "video/webm")
}

fn is_supported_media_mime(mime: &str) -> bool {
    is_supported_image_mime(mime) || is_supported_video_mime(mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- manifest matchers -----

    #[test]
    fn test_post_manifest_matcher_accepts_real_archive_filename() {
        // The archive that motivated this refinement.
        assert!(is_post_manifest(
            "your_facebook_activity/posts/your_posts__check_ins__photos_and_videos_1.json"
        ));
        // Older, simpler shape.
        assert!(is_post_manifest(
            "your_facebook_activity/posts/your_posts_1.json"
        ));
    }

    #[test]
    fn test_post_manifest_matcher_rejects_siblings() {
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/shared_memories.json"
        ));
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/places_you_have_been_tagged_in.json"
        ));
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/your_uncategorized_photos.json"
        ));
        // Has the prefix but no `_<digits>.json` suffix.
        assert!(!is_post_manifest(
            "your_facebook_activity/posts/your_posts_check_ins.json"
        ));
        // Wrong extension.
        assert!(!is_post_manifest("your_posts_1.txt"));
    }

    #[test]
    fn test_album_manifest_matcher() {
        assert!(is_album_manifest(
            "your_facebook_activity/posts/album/0.json"
        ));
        assert!(is_album_manifest(
            "your_facebook_activity/posts/album/profile_pictures.json"
        ));
        // Files outside the album directory are rejected even if they live
        // under `posts/`.
        assert!(!is_album_manifest(
            "your_facebook_activity/posts/your_posts_1.json"
        ));
        // Subdirectories under `album/` are not treated as album manifests.
        assert!(!is_album_manifest(
            "your_facebook_activity/posts/album/sub/0.json"
        ));
        // Wrong extension.
        assert!(!is_album_manifest(
            "your_facebook_activity/posts/album/0.txt"
        ));
    }

    // ----- mojibake fixup -----

    #[test]
    fn test_mojibake_fixup_repairs_known_pattern() {
        // The «hunger» sample observed in the user's archive.
        let mangled = "got a baby \u{00C2}\u{00AB}hunger\u{00C2}\u{00BB} two weeks ago";
        let fixed = fix_facebook_mojibake(mangled);
        assert_eq!(fixed, "got a baby «hunger» two weeks ago");
    }

    #[test]
    fn test_mojibake_fixup_repairs_three_byte_utf8() {
        // U+2019 right single quote: UTF-8 E2 80 99, latin1-mangled to
        // three chars (â + U+0080 + U+0099).
        let mangled = "you\u{00e2}\u{0080}\u{0099}re here";
        assert_eq!(fix_facebook_mojibake(mangled), "you\u{2019}re here");

        // U+201C left double quote: UTF-8 E2 80 9C.
        let mangled2 = "say \u{00e2}\u{0080}\u{009c}hi\u{00e2}\u{0080}\u{009d}";
        assert_eq!(fix_facebook_mojibake(mangled2), "say \u{201c}hi\u{201d}");
    }

    #[test]
    fn test_mojibake_fixup_repairs_four_byte_utf8() {
        // U+1F9A2 swan emoji: UTF-8 F0 9F A6 A2, latin1-mangled to four
        // chars (ð + U+009F + U+00A6 + U+00A2).
        let mangled = "two \u{00f0}\u{009f}\u{00a6}\u{00a2} watch";
        assert_eq!(fix_facebook_mojibake(mangled), "two \u{1f9a2} watch");
    }

    #[test]
    fn test_mojibake_fixup_leaves_clean_strings_alone() {
        // Plain ASCII.
        assert_eq!(
            fix_facebook_mojibake("hello world"),
            std::borrow::Cow::Borrowed("hello world")
        );
        // Pre-existing valid UTF-8 with non-ASCII codepoints (no C2/C3 prefix).
        let already_clean = "hola — té"; // em-dash + accented chars but no C2/C3 leaders
        assert_eq!(fix_facebook_mojibake(already_clean).as_ref(), already_clean);
        // String that contains C2 but does not benefit from re-decoding.
        // U+00C2 alone (not followed by another high-bit char) decodes to
        // the same length, so we leave it.
        let stray_c2 = "stray Â at end";
        // The decoded result is the same length so the strict-fewer-chars
        // gate keeps the original.
        assert_eq!(fix_facebook_mojibake(stray_c2).as_ref(), stray_c2);
    }

    // ----- url auto-link -----

    #[test]
    fn test_wrap_bare_urls_basic() {
        let input = "see https://example.com for more";
        assert_eq!(wrap_bare_urls(input), "see <https://example.com> for more");
    }

    #[test]
    fn test_wrap_bare_urls_strips_trailing_punctuation() {
        let input = "visit https://example.com.";
        assert_eq!(wrap_bare_urls(input), "visit <https://example.com>.");
    }

    #[test]
    fn test_wrap_bare_urls_preserves_query_params() {
        let url = "https://twitter.com/u/status/1?ref_src=twsrc%5Etfw&ref_url=foo";
        let input = format!("see {url}");
        let want = format!("see <{url}>");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_leaves_text_without_urls_alone() {
        // No URL anywhere in the string; the function should short-circuit
        // on the regex pre-check and return Borrowed unchanged.
        let input = "no urls here";
        assert_eq!(wrap_bare_urls(input), input);
    }

    #[test]
    fn test_wrap_bare_urls_keeps_balanced_parens_in_url() {
        // Wikipedia-style URL whose path legitimately ends in `)`.
        // Closing parens inside the URL are balanced, so the trailing
        // `)` is part of the URL and must be preserved.
        let url = "https://en.wikipedia.org/wiki/Pi_(letter)";
        let input = format!("see {url}");
        let want = format!("see <{url}>");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_strips_unbalanced_trailing_paren() {
        // Sentence-wrapping paren around a URL that itself has none.
        let input = "(see https://example.com)";
        assert_eq!(wrap_bare_urls(input), "(see <https://example.com>)");
    }

    #[test]
    fn test_wrap_bare_urls_strips_extra_paren_after_balanced_url() {
        // Sentence wraps a Wikipedia URL: balanced parens stay, the
        // extra closing paren that wraps the sentence is peeled off.
        let url = "https://en.wikipedia.org/wiki/Pi_(letter)";
        let input = format!("(see {url})");
        let want = format!("(see <{url}>)");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_keeps_balanced_brackets_in_url() {
        // Mirror of the parens case for square brackets.
        let url = "https://example.com/Array.prototype[Symbol.iterator]";
        let input = format!("see {url} now");
        let want = format!("see <{url}> now");
        assert_eq!(wrap_bare_urls(&input), want);
    }

    #[test]
    fn test_wrap_bare_urls_multiple() {
        let input = "first https://a.example then https://b.example end";
        assert_eq!(
            wrap_bare_urls(input),
            "first <https://a.example> then <https://b.example> end"
        );
    }

    // ----- album normalization (Phase 9d: FacebookAlbum, not FacebookPost) -----

    #[test]
    fn test_album_normalizes_with_description() {
        let raw = RawAlbum {
            name: Some("Phone Pics".to_string()),
            photos: vec![RawAlbumPhoto {
                uri: Some("posts/media/PhonePics/a.jpg".to_string()),
                creation_timestamp: Some(1262498419),
            }],
            description: Some("Captured on the go.".to_string()),
            last_modified_timestamp: Some(1274792706),
        };
        let album = normalize_album(raw).expect("album should normalize");
        assert_eq!(album.name, "Phone Pics");
        assert_eq!(album.description.as_deref(), Some("Captured on the go."));
        // Earliest creation_timestamp wins over last_modified_timestamp.
        assert_eq!(album.timestamp, 1262498419);
        assert_eq!(album.attachments.len(), 1);
        assert_eq!(album.attachments[0].0, "posts/media/PhonePics/a.jpg");
    }

    #[test]
    fn test_album_normalizes_without_description() {
        let raw = RawAlbum {
            name: Some("Profile pictures".to_string()),
            photos: vec![RawAlbumPhoto {
                uri: Some("posts/media/p/a.jpg".to_string()),
                creation_timestamp: Some(1256877040),
            }],
            description: None,
            last_modified_timestamp: Some(1732132304),
        };
        let album = normalize_album(raw).expect("album should normalize");
        assert_eq!(album.name, "Profile pictures");
        assert!(album.description.is_none());
    }

    #[test]
    fn test_album_synthesizes_name_when_missing() {
        let raw = RawAlbum {
            name: None,
            photos: vec![RawAlbumPhoto {
                uri: Some("posts/media/x/a.jpg".to_string()),
                creation_timestamp: Some(123456),
            }],
            description: None,
            last_modified_timestamp: None,
        };
        let album = normalize_album(raw).expect("album should normalize");
        assert!(album.name.starts_with("Untitled album"));
    }

    #[test]
    fn test_album_drops_when_no_photos() {
        let raw = RawAlbum {
            name: Some("Empty".to_string()),
            photos: vec![],
            description: None,
            last_modified_timestamp: Some(123),
        };
        assert!(normalize_album(raw).is_none());
    }

    // ----- regular post normalization -----

    #[test]
    fn test_normalize_post_drops_auto_title_only_entries() {
        // A post with the canned FB activity title and no body content
        // and no attachments should be dropped.
        let raw = RawFacebookPost {
            timestamp: Some(1257025936),
            data: vec![RawDataEntry { post: None }, RawDataEntry { post: None }],
            attachments: vec![],
        };
        assert!(normalize_post(raw).is_none());
    }

    #[test]
    fn test_normalize_post_keeps_real_body_text() {
        let raw = RawFacebookPost {
            timestamp: Some(1257025936),
            data: vec![RawDataEntry {
                post: Some("Hello world".to_string()),
            }],
            attachments: vec![],
        };
        let post = normalize_post(raw).expect("should retain real body");
        assert_eq!(post.body, "Hello world");
    }

    #[test]
    fn test_normalize_post_applies_mojibake_fixup() {
        let raw = RawFacebookPost {
            timestamp: Some(1257025936),
            data: vec![RawDataEntry {
                post: Some("baby \u{00C2}\u{00AB}hunger\u{00C2}\u{00BB}".to_string()),
            }],
            attachments: vec![],
        };
        let post = normalize_post(raw).expect("should retain");
        assert_eq!(post.body, "baby «hunger»");
    }
}
