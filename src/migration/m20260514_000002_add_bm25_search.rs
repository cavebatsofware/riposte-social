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
//! Enable the paradedb pg_search extension and create a BM25 index on
//! `posts.body`. The index also exposes a `body_ngram` alias built with
//! a 3-4 character ngram tokenizer so the feed search supports
//! partial-word matches (e.g. `phon` finds `phone`). Whole-word typo
//! tolerance is handled at query time via `paradedb.match(..., distance => 1)`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("CREATE EXTENSION IF NOT EXISTS pg_search")
            .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_posts_body_bm25 ON posts \
             USING bm25 (id, body, (body::pdb.ngram(3,4, 'alias=body_ngram'))) \
             WITH (key_field='id')",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_posts_body_bm25")
            .await?;
        Ok(())
    }
}
