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
//! 1-to-1 detail table for `posts` rows with `kind='article'`.
//!
//! Article title lives on `posts.slug` (shared title-like field).
//! Article body lives on `posts.body` (shared markdown body).
//! Only article-specific fields live here: subtitle, cover_media_id,
//! excerpt, reading_time_minutes, is_draft.
//!
//! The "row exists iff kind='article'" rule is enforced app-side in
//! create/delete transactions; Postgres can't express a conditional FK.
//! `is_draft` has a partial index because the only query that filters
//! on it is "list my drafts" where the result set is intentionally small.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ArticleDetails::Table)
                    .if_not_exists()
                    .col(uuid(ArticleDetails::PostId).primary_key())
                    .col(string_null(ArticleDetails::Subtitle))
                    .col(uuid_null(ArticleDetails::CoverMediaId))
                    .col(text_null(ArticleDetails::Excerpt))
                    .col(
                        integer(ArticleDetails::ReadingTimeMinutes)
                            .default(1),
                    )
                    .col(
                        boolean(ArticleDetails::IsDraft).default(false),
                    )
                    .col(timestamp_with_time_zone(ArticleDetails::CreatedAt))
                    .col(timestamp_with_time_zone(ArticleDetails::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_article_details_post_id")
                            .from(ArticleDetails::Table, ArticleDetails::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_article_details_cover_media_id")
                            .from(ArticleDetails::Table, ArticleDetails::CoverMediaId)
                            .to(PostMedia::Table, PostMedia::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_article_details_is_draft \
                 ON article_details (is_draft) WHERE is_draft = true",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP INDEX IF EXISTS idx_article_details_is_draft")
            .await?;
        manager
            .drop_table(Table::drop().table(ArticleDetails::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ArticleDetails {
    Table,
    PostId,
    Subtitle,
    CoverMediaId,
    Excerpt,
    ReadingTimeMinutes,
    IsDraft,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum PostMedia {
    Table,
    Id,
}
