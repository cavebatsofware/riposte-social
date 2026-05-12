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
//! Add `kind` and `slug` to `posts` so the same table can host albums.
//!
//! `kind` is an open-ended TEXT discriminator. v1 values are 'post' and
//! 'album'; future kinds (e.g. 'article') slot in without DDL. Existing
//! rows default to 'post' via the column default. An index on `kind`
//! keeps the feed's default `kind='post'` filter cheap.
//!
//! `slug` is a short title-like field. Albums store the album name
//! here; ordinary posts may leave it null. App-layer caps the length.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            "ALTER TABLE posts ADD COLUMN kind TEXT NOT NULL DEFAULT 'post'",
        )
        .await?;

        db.execute_unprepared("ALTER TABLE posts ADD COLUMN slug TEXT")
            .await?;

        db.execute_unprepared("CREATE INDEX idx_posts_kind ON posts(kind)")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_posts_kind")
            .await?;
        db.execute_unprepared("ALTER TABLE posts DROP COLUMN IF EXISTS slug")
            .await?;
        db.execute_unprepared("ALTER TABLE posts DROP COLUMN IF EXISTS kind")
            .await?;
        Ok(())
    }
}
