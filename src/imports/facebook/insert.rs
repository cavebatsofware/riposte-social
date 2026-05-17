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
use crate::s3::S3Service;
use chrono::TimeZone;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set, TransactionTrait};
use std::path::Path;
use uuid::Uuid;

/// Process one post: read each attachment from the local archive, upload
/// to S3, and write the post + post_media rows in a single transaction.
pub(crate) async fn import_one_post(
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

    let mut uploaded: Vec<(Uuid, String, String)> = Vec::with_capacity(staged_media.len());
    for (i, (filename, bytes)) in staged_media.into_iter().enumerate() {
        let media_id = Uuid::new_v4();
        let mime = mime_for_filename(&filename);
        // images and videos are both supported now. Other
        // types (`.mov` / unknown) are skipped rather than failing the
        // whole post.
        if !is_supported_media_mime(&mime) {
            continue;
        }
        let key = format!("posts/{}/{}", post_id, media_id);
        if let Err(e) = s3.put_object_at(&key, bytes, &mime).await {
            // Roll back any uploads already committed for this post so
            // we don't leave orphan objects when the DB transaction
            // below never runs.
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

    let mut uploaded: Vec<(Uuid, String, String, Option<String>)> =
        Vec::with_capacity(supported_indices.len());
    for (out_idx, src_idx) in supported_indices.iter().enumerate() {
        let (filename, bytes) = &staged_media[*src_idx];
        let media_id = Uuid::new_v4();
        let mime = mime_for_filename(filename);
        let key = format!("posts/{}/{}", post_id, media_id);
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

        for (i, (media_id, key, mime, caption)) in uploaded.iter().enumerate() {
            post_media::ActiveModel {
                id: Set(*media_id),
                post_id: Set(post_id),
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
