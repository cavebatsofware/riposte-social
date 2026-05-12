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
//! Copy `album_media` rows into `post_media` and shift ordinals so the
//! pre-migration `albums.cover_media_id` ends up at ordinal 0.
//!
//! Two-step:
//!
//! 1. Cover-ordinal shift (executed in `album_media` BEFORE the copy):
//!    for each album whose cover_media_id is set AND points at a media
//!    row whose ordinal isn't 0, rewrite ordinals so the cover row
//!    becomes ordinal 0 and the rest shift +1 to make room. Albums
//!    whose cover is NULL or already at ordinal 0 need no shift.
//!
//! 2. Copy: `INSERT INTO post_media (...) SELECT ... FROM album_media`.
//!    The album.id was reused as post.id in the previous migration, so
//!    `album_media.album_id` maps directly to `post_media.post_id` with
//!    no remap.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Step 1: shift ordinals so each album's cover row sits at 0.
        // The +1000000 offset is a temporary high-bit value that avoids
        // colliding with the swapped row during the two-statement
        // remap; the final UPDATE pulls everything back into a dense
        // 0..n range.
        //
        // Albums whose cover is NULL or already ordinal 0 are filtered
        // out by the WHERE EXISTS clause; the no-op case writes
        // nothing.
        db.execute_unprepared(
            r#"
            UPDATE album_media SET ordinal = ordinal + 1000000
            WHERE album_id IN (
                SELECT a.id FROM albums a
                JOIN album_media m ON m.id = a.cover_media_id
                WHERE m.ordinal <> 0
            )
            "#,
        )
        .await?;

        // After the high-bit shift every album's rows occupy
        // [1000000..) and there are no row-level conflicts. Now place
        // each cover at 0 and re-densify the rest into [1..n].
        db.execute_unprepared(
            r#"
            WITH covers AS (
                SELECT a.id AS album_id, a.cover_media_id AS cover_id
                FROM albums a
                WHERE a.cover_media_id IS NOT NULL
                  AND EXISTS (
                      SELECT 1 FROM album_media m
                      WHERE m.id = a.cover_media_id AND m.ordinal >= 1000000
                  )
            )
            UPDATE album_media m SET ordinal = 0
            FROM covers c
            WHERE m.id = c.cover_id
            "#,
        )
        .await?;

        // Re-densify non-cover rows in each affected album: assign
        // ordinals [1..n] in original order (the +1000000 offset
        // preserved relative ordering).
        db.execute_unprepared(
            r#"
            WITH ranked AS (
                SELECT
                    m.id,
                    ROW_NUMBER() OVER (PARTITION BY m.album_id ORDER BY m.ordinal) AS rn
                FROM album_media m
                WHERE m.album_id IN (
                    SELECT album_id FROM album_media WHERE ordinal >= 1000000
                )
                  AND m.ordinal >= 1000000
            )
            UPDATE album_media m SET ordinal = r.rn
            FROM ranked r
            WHERE m.id = r.id
            "#,
        )
        .await?;

        // Step 2: copy. album_id maps to post_id directly because of
        // the album.id → post.id reuse in the previous migration.
        db.execute_unprepared(
            r#"
            INSERT INTO post_media (
                id, post_id, s3_key, mime_type, width, height,
                ordinal, caption, created_at
            )
            SELECT
                id, album_id, s3_key, mime_type, width, height,
                ordinal, caption, created_at
            FROM album_media
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "album_media-to-post_media migration cannot be rolled back; restore from backup".to_string(),
        ))
    }
}
