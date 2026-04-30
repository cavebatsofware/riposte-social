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
                    .table(Comments::Table)
                    .if_not_exists()
                    .col(uuid(Comments::Id).primary_key())
                    .col(uuid(Comments::PostId))
                    .col(uuid(Comments::UserId))
                    .col(text(Comments::Body))
                    .col(timestamp_with_time_zone_null(Comments::DeletedAt))
                    .col(timestamp_with_time_zone(Comments::CreatedAt))
                    .col(timestamp_with_time_zone(Comments::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_post_id")
                            .from(Comments::Table, Comments::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_user_id")
                            .from(Comments::Table, Comments::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        // Per-post listing in chronological order, filtered to live rows.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_comments_post_thread")
                    .table(Comments::Table)
                    .col(Comments::PostId)
                    .col(Comments::DeletedAt)
                    .col(Comments::CreatedAt)
                    .col(Comments::Id)
                    .to_owned(),
            )
            .await?;

        // Author lookups for moderation views.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_comments_user_id")
                    .table(Comments::Table)
                    .col(Comments::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Comments::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Comments {
    Table,
    Id,
    PostId,
    UserId,
    Body,
    DeletedAt,
    CreatedAt,
    UpdatedAt,
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
