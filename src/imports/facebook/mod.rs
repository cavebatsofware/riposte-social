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
//! 1. Stream the uploaded ZIP from S3 to a local tempfile (`upload.rs`).
//! 2. Walk the archive central directory and parse the manifests
//!    (`parser.rs`).
//! 3. Dedup against `posts.import_external_id` so re-running an import
//!    skips already-imported rows. The external id is a stable hash over
//!    `(timestamp, body)` plus the first attachment URI when present.
//! 4. Process posts and albums with `buffer_unordered` for bounded
//!    concurrency. Each task opens its own `ZipArchive` from the local
//!    tempfile and uploads its media to S3 in series (`insert.rs`).
//!
//! Mojibake: FB exports double-encode UTF-8 as latin1 for any text field
//! with non-ASCII characters. `parser::fix_facebook_mojibake` is applied
//! to album name, description, and post body during normalization so the
//! DB stores the correctly decoded codepoints.
//!
//! URL formatting: post bodies arrive with bare http(s) URLs.
//! `parser::wrap_bare_urls` rewrites them as markdown autolinks so the
//! social frontend renders them as clickable links.

pub mod insert;
pub mod parser;
pub mod upload;

use crate::entities::post;
use crate::imports::{self, JobProgress};
use crate::s3::S3Service;
use futures::stream::{self, StreamExt};
use insert::{import_one_album, import_one_post};
use parser::parse_archive;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use upload::stage_archive_to_tempfile;
use uuid::Uuid;

/// Bounded post-level concurrency. Each unit does S3 reads + writes plus
/// a short DB transaction; small N keeps us polite to S3 and the DB.
const IMPORT_CONCURRENCY: usize = 4;

/// Heartbeat / progress flush cadence. Low enough that the admin UI sees
/// movement during a long import; high enough that we don't pound the DB
/// after every single item.
const PROGRESS_FLUSH_EVERY: i64 = 5;

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

/// Result of running the worker. Returned to the caller so tests can
/// inspect outcomes without re-querying the DB.
#[derive(Debug, Default)]
pub struct FacebookImportSummary {
    pub total: i64,
    pub succeeded: i64,
    pub skipped: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Copy)]
enum ItemOutcome {
    Succeeded,
    Skipped,
    Failed,
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

    // Albums dedup: scope the lookup to `kind='album'` so it rides the
    // `idx_posts_kind` index.
    let album_candidate_ids: Vec<String> = albums.iter().map(|a| a.external_id.clone()).collect();
    let albums_already: HashSet<String> = if album_candidate_ids.is_empty() {
        HashSet::new()
    } else {
        post::Entity::find()
            .filter(post::Column::Kind.eq(post::KIND_ALBUM))
            .filter(post::Column::ImportSource.eq("facebook"))
            .filter(post::Column::ImportExternalId.is_in(album_candidate_ids))
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
