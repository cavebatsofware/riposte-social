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
//! S3 batch upload + key allocation. The upload runs before the DB
//! transaction; failures roll back any objects already uploaded so the
//! caller can retry without leaving orphans.

use crate::errors::{AppError, AppResult};
use crate::posts::media::{PendingMedia, PlannedUpload};
use crate::s3::S3Service;
use uuid::Uuid;

/// Total multipart body cap. Tracks the value already used by both route
/// modules so this constant doesn't drift away from the per-file caps.
pub const COMPOSE_BODY_MAX_BYTES: usize = 256 * 1024 * 1024;

pub(crate) fn build_media_plan(
    post_id: Uuid,
    media: Vec<PendingMedia>,
    base_ordinal: i32,
) -> Vec<PlannedUpload> {
    media
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let media_id = Uuid::new_v4();
            PlannedUpload {
                media_id,
                s3_key: format!("posts/{}/{}", post_id, media_id),
                media: m,
                ordinal: base_ordinal + i as i32,
                width: None,
                height: None,
                thumbnail_data: None,
                icon_data: None,
            }
        })
        .collect()
}

/// Push every planned upload into S3, tracking successful keys so a
/// later failure can roll the whole batch back.
pub(crate) async fn upload_media(s3: &S3Service, plan: &[PlannedUpload]) -> AppResult<Vec<String>> {
    let mut uploaded: Vec<String> = Vec::new();
    for item in plan {
        if let Err(e) = s3
            .put_object_at(
                &item.s3_key,
                item.media.bytes.clone(),
                &item.media.mime_type,
            )
            .await
        {
            rollback_uploads(s3, &uploaded).await;
            return Err(AppError::InternalError(format!(
                "Failed to upload media: {}",
                e
            )));
        }
        uploaded.push(item.s3_key.clone());
    }
    Ok(uploaded)
}

/// Best-effort cleanup. Per-key errors are swallowed because the call
/// site already failed on something else; surfacing a secondary delete
/// error would just mask the real cause.
pub(crate) async fn rollback_uploads(s3: &S3Service, keys: &[String]) {
    for k in keys {
        let _ = s3.delete_object_at(k).await;
    }
}
