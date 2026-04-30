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
                    .table(Reactions::Table)
                    .if_not_exists()
                    .col(uuid(Reactions::Id).primary_key())
                    .col(uuid(Reactions::PostId))
                    .col(uuid(Reactions::UserId))
                    // Kind: "like" today; the column is a string so adding
                    // "love", "laugh", etc. later requires no DDL.
                    .col(string(Reactions::Kind))
                    .col(timestamp_with_time_zone(Reactions::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reactions_post_id")
                            .from(Reactions::Table, Reactions::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reactions_user_id")
                            .from(Reactions::Table, Reactions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // One reaction of a given kind per (post, user). Add/remove is
        // idempotent at the API layer via insert-or-ignore + delete.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_reactions_post_user_kind")
                    .table(Reactions::Table)
                    .col(Reactions::PostId)
                    .col(Reactions::UserId)
                    .col(Reactions::Kind)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Counts-per-post and per-kind queries scan this index.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_reactions_post_kind")
                    .table(Reactions::Table)
                    .col(Reactions::PostId)
                    .col(Reactions::Kind)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Reactions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Reactions {
    Table,
    Id,
    PostId,
    UserId,
    Kind,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
