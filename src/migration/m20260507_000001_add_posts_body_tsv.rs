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
//! Add `content_lang` and `body_tsv` to `posts`, plus a GIN index.
//!
//! `body_tsv` is a STORED generated column built via CASE over literal
//! FTS configuration names — STORED generated columns require the
//! expression to be IMMUTABLE, and `content_lang::regconfig` (dynamic
//! cast) is only STABLE.
//!
//! Built-in stemmers cover english/spanish/french/german. The CASE
//! ELSE branch maps any other value to `'simple'`. Adding `chinese`
//! requires `CREATE EXTENSION pg_jieba` plus an extra CASE branch.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Stored as the literal FTS config name so the CASE below can
        // compare against literals (no regconfig cast — keeps the
        // generated column expression immutable). Existing rows
        // backfill to 'english' via the column default.
        db.execute_unprepared(
            "ALTER TABLE posts ADD COLUMN content_lang TEXT NOT NULL DEFAULT 'english'",
        )
        .await?;

        // ELSE 'simple' is the safety net: any unsupported value
        // (including 'chinese' until pg_jieba is installed, plus any
        // typo) gets whitespace tokenization rather than failing.
        db.execute_unprepared(
            r#"
            ALTER TABLE posts
            ADD COLUMN body_tsv TSVECTOR
            GENERATED ALWAYS AS (
                CASE content_lang
                    WHEN 'english' THEN to_tsvector('english', coalesce(body, ''))
                    WHEN 'spanish' THEN to_tsvector('spanish', coalesce(body, ''))
                    WHEN 'french'  THEN to_tsvector('french',  coalesce(body, ''))
                    WHEN 'german'  THEN to_tsvector('german',  coalesce(body, ''))
                    ELSE to_tsvector('simple', coalesce(body, ''))
                END
            ) STORED
            "#,
        )
        .await?;

        db.execute_unprepared("CREATE INDEX idx_posts_body_tsv ON posts USING GIN (body_tsv)")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_posts_body_tsv")
            .await?;
        db.execute_unprepared("ALTER TABLE posts DROP COLUMN IF EXISTS body_tsv")
            .await?;
        db.execute_unprepared("ALTER TABLE posts DROP COLUMN IF EXISTS content_lang")
            .await?;
        Ok(())
    }
}
