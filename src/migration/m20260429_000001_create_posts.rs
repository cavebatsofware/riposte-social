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
                    .table(Posts::Table)
                    .if_not_exists()
                    .col(uuid(Posts::Id).primary_key())
                    .col(uuid(Posts::AuthorId))
                    .col(text(Posts::Body))
                    // Visibility: "public" | "commenters" | "posters". Stored
                    // as a string with an app-side check rather than a
                    // Postgres enum so adding tiers later is a no-op DDL.
                    .col(string(Posts::Visibility))
                    .col(timestamp_with_time_zone(Posts::PublishedAt))
                    .col(string_null(Posts::ImportSource))
                    .col(string_null(Posts::ImportExternalId))
                    .col(timestamp_with_time_zone_null(Posts::DeletedAt))
                    .col(timestamp_with_time_zone(Posts::CreatedAt))
                    .col(timestamp_with_time_zone(Posts::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_posts_author_id")
                            .from(Posts::Table, Posts::AuthorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        // Feed query is "live posts in published_at-desc order, filtered by
        // visibility tier". Index covers it.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_posts_feed")
                    .table(Posts::Table)
                    .col(Posts::DeletedAt)
                    .col(Posts::PublishedAt)
                    .col(Posts::Id)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_posts_author_id")
                    .table(Posts::Table)
                    .col(Posts::AuthorId)
                    .to_owned(),
            )
            .await?;

        // Dedup key for Facebook (and future) imports. Partial-unique would
        // be ideal but sea-orm-migration doesn't expose that; the app layer
        // checks before insert.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_posts_import_key")
                    .table(Posts::Table)
                    .col(Posts::ImportSource)
                    .col(Posts::ImportExternalId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Posts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Id,
    AuthorId,
    Body,
    Visibility,
    PublishedAt,
    ImportSource,
    ImportExternalId,
    DeletedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
