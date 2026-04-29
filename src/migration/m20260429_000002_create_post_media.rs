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
                    .table(PostMedia::Table)
                    .if_not_exists()
                    .col(uuid(PostMedia::Id).primary_key())
                    .col(uuid(PostMedia::PostId))
                    .col(string(PostMedia::S3Key))
                    .col(string(PostMedia::MimeType))
                    .col(integer_null(PostMedia::Width))
                    .col(integer_null(PostMedia::Height))
                    .col(integer(PostMedia::Ordinal))
                    .col(string_null(PostMedia::Caption))
                    .col(timestamp_with_time_zone(PostMedia::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_post_media_post_id")
                            .from(PostMedia::Table, PostMedia::PostId)
                            .to(Posts::Table, Posts::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
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

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PostMedia::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PostMedia {
    Table,
    Id,
    PostId,
    S3Key,
    MimeType,
    Width,
    Height,
    Ordinal,
    Caption,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Posts {
    Table,
    Id,
}
