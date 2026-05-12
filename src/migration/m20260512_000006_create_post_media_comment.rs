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
//! Per-media-row comments, mirroring `comment` for posts' media items.
//!
//! Soft-delete via `deleted_at`. `edited_at` is set on first edit and
//! kept stable thereafter. Author FK is RESTRICT (rather than CASCADE)
//! so that user-deletion has to deal with comments explicitly — the
//! same rule the post-level comment table follows.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PostMediaComments::Table)
                    .if_not_exists()
                    .col(uuid(PostMediaComments::Id).primary_key())
                    .col(uuid(PostMediaComments::PostMediaId))
                    .col(uuid(PostMediaComments::UserId))
                    .col(text(PostMediaComments::Body))
                    .col(timestamp_with_time_zone_null(PostMediaComments::DeletedAt))
                    .col(timestamp_with_time_zone_null(PostMediaComments::EditedAt))
                    .col(timestamp_with_time_zone(PostMediaComments::CreatedAt))
                    .col(timestamp_with_time_zone(PostMediaComments::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_post_media_comments_post_media_id")
                            .from(
                                PostMediaComments::Table,
                                PostMediaComments::PostMediaId,
                            )
                            .to(PostMedia::Table, PostMedia::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_post_media_comments_user_id")
                            .from(PostMediaComments::Table, PostMediaComments::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        // Per-media listing in chronological order, filtered to live rows.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_post_media_comments_media_thread")
                    .table(PostMediaComments::Table)
                    .col(PostMediaComments::PostMediaId)
                    .col(PostMediaComments::DeletedAt)
                    .col(PostMediaComments::CreatedAt)
                    .col(PostMediaComments::Id)
                    .to_owned(),
            )
            .await?;

        // Author lookups for moderation views.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_post_media_comments_user_id")
                    .table(PostMediaComments::Table)
                    .col(PostMediaComments::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PostMediaComments::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PostMediaComments {
    Table,
    Id,
    PostMediaId,
    UserId,
    Body,
    DeletedAt,
    EditedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum PostMedia {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
