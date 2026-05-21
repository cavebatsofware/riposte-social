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
//! DB insertion for the Facebook importer. Per-post and per-album
//! transactions stage media off the local ZIP, upload to S3, then write
//! the post + post_media rows. On any failure after S3 upload, uploaded
//! objects are best-effort deleted so a failed run doesn't leave orphans.

use crate::entities::{post, post_media};
use crate::imports::facebook::parser::{FacebookAlbum, FacebookPost};
use crate::imports::facebook::upload::{
    is_supported_media_mime, mime_for_filename, read_archive_entries,
};
use crate::posts::media::is_video_mime;
use crate::posts::media::variants::{generate_variants_blocking, ImageVariants};
use crate::s3::S3Service;
use chrono::TimeZone;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set, TransactionTrait};
use std::path::Path;
use uuid::Uuid;

/// One uploaded media row pending DB insert. Carries everything the
/// `post_media` row needs: identity (`media_id`, `s3_key`, `mime`), the
/// album-only caption, and the derived image variants (None for videos).
struct UploadedMedia {
    media_id: Uuid,
    s3_key: String,
    mime: String,
    caption: Option<String>,
    variants: Option<ImageVariants>,
}

/// Compute thumbnail + icon + dimensions for an imported image. Takes
/// the bytes by value, moves them into the blocking worker, and returns
/// them back alongside the variants so the caller can still upload to
/// S3 without holding a parallel copy in memory during the decode.
/// Returns `(bytes, None)` for video items, bubbling up decode errors
/// so the importer can roll back the post.
async fn variants_if_image(
    mime: &str,
    bytes: Vec<u8>,
    max_input_dimension: u32,
) -> Result<(Vec<u8>, Option<ImageVariants>), anyhow::Error> {
    if is_video_mime(mime) {
        return Ok((bytes, None));
    }
    let (bytes, variants) = tokio::task::spawn_blocking(move || {
        let res = generate_variants_blocking(&bytes, max_input_dimension);
        (bytes, res)
    })
    .await
    .map_err(|e| anyhow::anyhow!("variant worker join failed: {}", e))?;
    let variants = variants.map_err(|e| anyhow::anyhow!("variant generation failed: {}", e))?;
    Ok((bytes, Some(variants)))
}

/// Process one post: read each attachment from the local archive, upload
/// to S3, and write the post + post_media rows in a single transaction.
pub(crate) async fn import_one_post(
    db: &DatabaseConnection,
    s3: &S3Service,
    archive_path: &Path,
    fb_post: &FacebookPost,
    visibility: &str,
    created_by: Uuid,
    max_input_dimension: u32,
) -> Result<(), anyhow::Error> {
    let post_id = Uuid::new_v4();

    // Pre-stage media: read all bytes off the local zip on a blocking
    // thread, then upload to S3 in series. Sequential within a post is
    // fine; per-post fan-out via `buffer_unordered` upstream provides
    // the overall concurrency.
    let staged_media = {
        let archive_path = archive_path.to_path_buf();
        let uris = fb_post.attachment_uris.clone();
        tokio::task::spawn_blocking(move || read_archive_entries(&archive_path, &uris))
            .await
            .map_err(|e| anyhow::anyhow!("media read task join failed: {}", e))??
    };

    // Defense against importing empty posts: if every staged item has
    // an unsupported mime AND the body is empty, there is nothing to
    // render. Skip cleanly rather than insert a content-empty row.
    let any_supported = staged_media
        .iter()
        .any(|(filename, _)| is_supported_media_mime(&mime_for_filename(filename)));
    if fb_post.body.is_empty() && !any_supported {
        return Ok(());
    }

    let mut uploaded: Vec<UploadedMedia> = Vec::with_capacity(staged_media.len());
    for (i, (filename, bytes)) in staged_media.into_iter().enumerate() {
        let media_id = Uuid::new_v4();
        let mime = mime_for_filename(&filename);
        // images and videos are both supported now. Other
        // types (`.mov` / unknown) are skipped rather than failing the
        // whole post.
        if !is_supported_media_mime(&mime) {
            continue;
        }
        let (bytes, variants) = variants_if_image(&mime, bytes, max_input_dimension).await?;
        let key = format!("posts/{}/{}", post_id, media_id);
        if let Err(e) = s3.put_object_at(&key, bytes, &mime).await {
            // Roll back any uploads already committed for this post so
            // we don't leave orphan objects when the DB transaction
            // below never runs.
            for prior in &uploaded {
                let _ = s3.delete_object_at(&prior.s3_key).await;
            }
            return Err(anyhow::anyhow!("failed to upload media #{}: {}", i, e));
        }
        uploaded.push(UploadedMedia {
            media_id,
            s3_key: key,
            mime,
            caption: None,
            variants,
        });
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
            kind: Set(post::KIND_POST.to_string()),
            slug: Set(None),
        }
        .insert(&txn)
        .await?;

        insert_media_rows(&txn, post_id, &uploaded, now).await?;

        txn.commit().await
    }
    .await;

    if let Err(e) = txn_result {
        for prior in &uploaded {
            let _ = s3.delete_object_at(&prior.s3_key).await;
        }
        return Err(anyhow::anyhow!("DB write failed: {}", e));
    }

    Ok(())
}

/// Shared helper: write a `post_media` row per uploaded item, threading
/// the optional caption and derived image variants from `UploadedMedia`
/// into the row.
async fn insert_media_rows<C: sea_orm::ConnectionTrait>(
    txn: &C,
    post_id: Uuid,
    uploaded: &[UploadedMedia],
    now: sea_orm::prelude::DateTimeWithTimeZone,
) -> Result<(), sea_orm::DbErr> {
    for (i, item) in uploaded.iter().enumerate() {
        post_media::ActiveModel {
            id: Set(item.media_id),
            post_id: Set(post_id),
            s3_key: Set(item.s3_key.clone()),
            mime_type: Set(item.mime.clone()),
            width: Set(item.variants.as_ref().map(|v| v.width)),
            height: Set(item.variants.as_ref().map(|v| v.height)),
            ordinal: Set(i as i32),
            caption: Set(item.caption.clone()),
            created_at: Set(now),
            thumbnail_data: Set(item.variants.as_ref().map(|v| v.thumbnail.clone())),
            icon_data: Set(item.variants.as_ref().map(|v| v.icon.clone())),
        }
        .insert(txn)
        .await?;
    }
    Ok(())
}

/// Import one FB album as a `post` row with `kind='album'` plus its
/// ordered `post_media` rows. The album name lives in `slug`, the
/// description in `body`, and the implicit cover (`min(ordinal)`) lands
/// at ordinal 0 by virtue of insertion order. Mirrors `import_one_post`
/// structurally: stage media off the zip, upload to S3, then insert in a
/// transaction.
pub(crate) async fn import_one_album(
    db: &DatabaseConnection,
    s3: &S3Service,
    archive_path: &Path,
    fb_album: &FacebookAlbum,
    visibility: &str,
    created_by: Uuid,
    max_input_dimension: u32,
) -> Result<(), anyhow::Error> {
    let post_id = Uuid::new_v4();

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
    let any_supported = staged_media
        .iter()
        .any(|(filename, _)| is_supported_media_mime(&mime_for_filename(filename)));
    if !any_supported {
        return Ok(());
    }

    let captions: Vec<Option<String>> = fb_album
        .attachments
        .iter()
        .map(|(_, c)| c.clone())
        .collect();

    let mut uploaded: Vec<UploadedMedia> = Vec::with_capacity(staged_media.len());
    for (src_idx, (filename, bytes)) in staged_media.into_iter().enumerate() {
        let mime = mime_for_filename(&filename);
        if !is_supported_media_mime(&mime) {
            continue;
        }
        let media_id = Uuid::new_v4();
        let (bytes, variants) = variants_if_image(&mime, bytes, max_input_dimension).await?;
        let key = format!("posts/{}/{}", post_id, media_id);
        if let Err(e) = s3.put_object_at(&key, bytes, &mime).await {
            for prior in &uploaded {
                let _ = s3.delete_object_at(&prior.s3_key).await;
            }
            return Err(anyhow::anyhow!(
                "failed to upload media #{}: {}",
                uploaded.len(),
                e
            ));
        }
        uploaded.push(UploadedMedia {
            media_id,
            s3_key: key,
            mime,
            caption: captions.get(src_idx).cloned().flatten(),
            variants,
        });
    }

    let txn_result: Result<(), sea_orm::DbErr> = async {
        let txn = db.begin().await?;
        let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
        let published = chrono::Utc
            .timestamp_opt(fb_album.timestamp, 0)
            .single()
            .unwrap_or_else(chrono::Utc::now);

        post::ActiveModel {
            id: Set(post_id),
            author_id: Set(created_by),
            body: Set(fb_album.description.clone().unwrap_or_default()),
            visibility: Set(visibility.to_string()),
            published_at: Set(published.into()),
            import_source: Set(Some("facebook".to_string())),
            import_external_id: Set(Some(fb_album.external_id.clone())),
            deleted_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            category_id: Set(None),
            kind: Set(post::KIND_ALBUM.to_string()),
            slug: Set(Some(fb_album.name.clone())),
        }
        .insert(&txn)
        .await?;

        insert_media_rows(&txn, post_id, &uploaded, now).await?;

        txn.commit().await
    }
    .await;

    if let Err(e) = txn_result {
        for prior in &uploaded {
            let _ = s3.delete_object_at(&prior.s3_key).await;
        }
        return Err(anyhow::anyhow!("DB write failed: {}", e));
    }

    Ok(())
}
