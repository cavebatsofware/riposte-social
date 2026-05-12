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
//! Per-media-row reactions, mirroring `comment_reaction` for posts'
//! media items.
//!
//! Both photos in a multi-photo post and photos in an album (which is
//! now a `kind='album'` post) attach reactions through this table. The
//! parent `post_media` row owns the visibility decision via its
//! parent post; this table just carries the (media, user, kind) facts.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PostMediaReactions::Table)
                    .if_not_exists()
                    .col(uuid(PostMediaReactions::Id).primary_key())
                    .col(uuid(PostMediaReactions::PostMediaId))
                    .col(uuid(PostMediaReactions::UserId))
                    .col(string(PostMediaReactions::Kind))
                    .col(timestamp_with_time_zone(PostMediaReactions::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_post_media_reactions_post_media_id")
                            .from(
                                PostMediaReactions::Table,
                                PostMediaReactions::PostMediaId,
                            )
                            .to(PostMedia::Table, PostMedia::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_post_media_reactions_user_id")
                            .from(PostMediaReactions::Table, PostMediaReactions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // One reaction of a given kind per (media, user); add/remove is
        // idempotent at the API layer via insert-or-ignore + delete.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_post_media_reactions_media_user_kind")
                    .table(PostMediaReactions::Table)
                    .col(PostMediaReactions::PostMediaId)
                    .col(PostMediaReactions::UserId)
                    .col(PostMediaReactions::Kind)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Counts-per-media + per-kind aggregates scan this index.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_post_media_reactions_media_kind")
                    .table(PostMediaReactions::Table)
                    .col(PostMediaReactions::PostMediaId)
                    .col(PostMediaReactions::Kind)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PostMediaReactions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PostMediaReactions {
    Table,
    Id,
    PostMediaId,
    UserId,
    Kind,
    CreatedAt,
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
