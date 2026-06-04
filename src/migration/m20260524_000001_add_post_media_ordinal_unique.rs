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
//! Promote the existing `(post_id, ordinal)` index on `post_media` to a
//! UNIQUE constraint. Defense-in-depth: append-media derives the next
//! ordinal from `max(ordinal) + 1`, and a concurrent append (same
//! author, two tabs, or a double-clicked Upload button) could otherwise
//! produce two rows with the same ordinal. The reorder path already uses
//! a +1_000_000 offset trick to avoid transient collisions, so it stays
//! correct under the new constraint without modification.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_post_media_post_id_ordinal")
                    .table(PostMedia::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_post_media_post_id_ordinal")
                    .table(PostMedia::Table)
                    .col(PostMedia::PostId)
                    .col(PostMedia::Ordinal)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uq_post_media_post_id_ordinal")
                    .table(PostMedia::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_post_media_post_id_ordinal")
                    .table(PostMedia::Table)
                    .col(PostMedia::PostId)
                    .col(PostMedia::Ordinal)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum PostMedia {
    Table,
    PostId,
    Ordinal,
}
