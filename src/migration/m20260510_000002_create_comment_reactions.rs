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
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CommentReactions::Table)
                    .if_not_exists()
                    .col(uuid(CommentReactions::Id).primary_key())
                    .col(uuid(CommentReactions::CommentId))
                    .col(uuid(CommentReactions::UserId))
                    .col(string(CommentReactions::Kind))
                    .col(timestamp_with_time_zone(CommentReactions::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comment_reactions_comment_id")
                            .from(CommentReactions::Table, CommentReactions::CommentId)
                            .to(Comments::Table, Comments::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comment_reactions_user_id")
                            .from(CommentReactions::Table, CommentReactions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // One reaction of a given kind per (comment, user); add/remove is
        // idempotent at the API layer via insert-or-ignore + delete.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_comment_reactions_comment_user_kind")
                    .table(CommentReactions::Table)
                    .col(CommentReactions::CommentId)
                    .col(CommentReactions::UserId)
                    .col(CommentReactions::Kind)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Counts-per-comment + per-kind aggregates scan this index.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_comment_reactions_comment_kind")
                    .table(CommentReactions::Table)
                    .col(CommentReactions::CommentId)
                    .col(CommentReactions::Kind)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CommentReactions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum CommentReactions {
    Table,
    Id,
    CommentId,
    UserId,
    Kind,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Comments {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
