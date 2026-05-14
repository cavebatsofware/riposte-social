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
//! Copy `albums` rows into `posts` as `kind='album'`.
//!
//! Each album becomes a post with the same UUID. Reusing the id keeps
//! external `/album/{id}` URLs valid against the post row, and lets the
//! album_media → post_media migration work without a remap table.
//!
//! Field mapping:
//!   posts.body          ← COALESCE(albums.description, '') (body is NOT NULL)
//!   posts.kind          ← 'album'
//!   posts.slug          ← albums.name
//!   posts.content_lang  ← 'english' (default; albums have no FTS lang)
//! Other columns copy 1:1 (author, visibility, timestamps, category, import flags).
//!
//! `down()` is intentionally non-reversible; the data lives only in
//! posts after this migration. Recovery is from a DB backup, not by
//! rolling back the migration.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            INSERT INTO posts (
                id, author_id, body, kind, slug, visibility,
                published_at, created_at, updated_at, deleted_at,
                import_source, import_external_id, category_id,
                content_lang
            )
            SELECT
                id, author_id, COALESCE(description, ''), 'album', name, visibility,
                published_at, created_at, updated_at, deleted_at,
                import_source, import_external_id, category_id,
                'english'
            FROM albums
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "albums-to-posts data migration cannot be rolled back; restore from backup".to_string(),
        ))
    }
}
