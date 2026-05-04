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
//! Phase 9e: categories.
//!
//! Flat list of admin-managed categories. One category per post or album
//! (FK columns are added in `m20260503_000003`). The left rail uses these
//! to group content; the feed query accepts `?category=<slug>` to filter.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Categories::Table)
                    .if_not_exists()
                    .col(uuid(Categories::Id).primary_key())
                    .col(string_uniq(Categories::Name))
                    .col(string_uniq(Categories::Slug))
                    .col(integer(Categories::Ordinal).default(0))
                    .col(string_null(Categories::Color))
                    .col(timestamp_with_time_zone(Categories::CreatedAt))
                    .col(timestamp_with_time_zone(Categories::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_categories_ordinal")
                    .table(Categories::Table)
                    .col(Categories::Ordinal)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Categories::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Categories {
    Table,
    Id,
    Name,
    Slug,
    Ordinal,
    Color,
    CreatedAt,
    UpdatedAt,
}
