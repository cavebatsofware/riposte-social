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
//! `follows` directed-edge table for the social graph.
//!
//! One row per (follower, followed) pair. Composite PK enforces "you can
//! only follow someone once". A CHECK constraint blocks self-follow rows
//! at the storage layer in addition to the route-handler 400 — defense in
//! depth so a future code path that bypasses the handler can't corrupt
//! the graph.
//!
//! `idx_follows_followed_id` keeps the "who follows me" lookup O(log n)
//! since the composite PK leads with `follower_id` and so doesn't help
//! the inverse direction.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Follows::Table)
                    .if_not_exists()
                    .col(uuid(Follows::FollowerId))
                    .col(uuid(Follows::FollowedId))
                    .col(timestamp_with_time_zone(Follows::CreatedAt))
                    .primary_key(
                        Index::create()
                            .col(Follows::FollowerId)
                            .col(Follows::FollowedId),
                    )
                    .check(
                        Expr::col(Follows::FollowerId).ne(Expr::col(Follows::FollowedId)),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_follows_follower_id")
                            .from(Follows::Table, Follows::FollowerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_follows_followed_id")
                            .from(Follows::Table, Follows::FollowedId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_follows_followed_id")
                    .table(Follows::Table)
                    .col(Follows::FollowedId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Follows::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Follows {
    Table,
    FollowerId,
    FollowedId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
