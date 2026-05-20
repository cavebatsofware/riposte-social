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
//! Pre-generated thumbnail and icon bytes on post_media.
//!
//! Image uploads produce two derived WebP sizes at compose time: a ~400px
//! thumbnail for grid / card display and a ~64px icon for compact rail and
//! list contexts. Both are embedded as base64 data URIs in API responses so
//! feed and album loads don't fan out one HTTP request per attachment.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PostMedia::Table)
                    .add_column(binary_null(PostMedia::ThumbnailData))
                    .add_column(binary_null(PostMedia::IconData))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PostMedia::Table)
                    .drop_column(PostMedia::ThumbnailData)
                    .drop_column(PostMedia::IconData)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PostMedia {
    Table,
    ThumbnailData,
    IconData,
}
