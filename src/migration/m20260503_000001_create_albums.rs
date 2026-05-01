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
//! Phase 9d: albums as a first-class entity.
//!
//! Albums are media-only collections — name, description, ordered set of
//! photo/video items with per-item captions. They're explicitly NOT posts:
//! the FB importer's prior "synthesize a post body listing every photo
//! URI" approach was the source of the long-feed-card pollution flagged
//! in user review.
//!
//! Albums never appear in `/api/feed` (posts only). They're discovered via
//! the left-rail Albums group and rendered at `/album/:id`.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // albums
        manager
            .create_table(
                Table::create()
                    .table(Albums::Table)
                    .if_not_exists()
                    .col(uuid(Albums::Id).primary_key())
                    .col(uuid(Albums::AuthorId))
                    .col(string(Albums::Name))
                    .col(text_null(Albums::Description))
                    // cover_media_id can be NULL while the album is being
                    // populated. Once at least one media item exists, the
                    // app sets it (defaulting to ordinal=0).
                    .col(uuid_null(Albums::CoverMediaId))
                    // Same four-value visibility enum as posts. Defaults
                    // to private (Phase 9a).
                    .col(string(Albums::Visibility).default("private"))
                    .col(timestamp_with_time_zone(Albums::PublishedAt))
                    .col(string_null(Albums::ImportSource))
                    .col(string_null(Albums::ImportExternalId))
                    .col(timestamp_with_time_zone_null(Albums::DeletedAt))
                    .col(timestamp_with_time_zone(Albums::CreatedAt))
                    .col(timestamp_with_time_zone(Albums::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_albums_author_id")
                            .from(Albums::Table, Albums::AuthorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_albums_listing")
                    .table(Albums::Table)
                    .col(Albums::DeletedAt)
                    .col(Albums::PublishedAt)
                    .col(Albums::Id)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_albums_author_id")
                    .table(Albums::Table)
                    .col(Albums::AuthorId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_albums_import_key")
                    .table(Albums::Table)
                    .col(Albums::ImportSource)
                    .col(Albums::ImportExternalId)
                    .to_owned(),
            )
            .await?;

        // album_media
        manager
            .create_table(
                Table::create()
                    .table(AlbumMedia::Table)
                    .if_not_exists()
                    .col(uuid(AlbumMedia::Id).primary_key())
                    .col(uuid(AlbumMedia::AlbumId))
                    .col(string(AlbumMedia::S3Key))
                    .col(string(AlbumMedia::MimeType))
                    .col(integer_null(AlbumMedia::Width))
                    .col(integer_null(AlbumMedia::Height))
                    .col(integer(AlbumMedia::Ordinal))
                    .col(text_null(AlbumMedia::Caption))
                    .col(timestamp_with_time_zone(AlbumMedia::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_album_media_album_id")
                            .from(AlbumMedia::Table, AlbumMedia::AlbumId)
                            .to(Albums::Table, Albums::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_album_media_album_id_ordinal")
                    .table(AlbumMedia::Table)
                    .col(AlbumMedia::AlbumId)
                    .col(AlbumMedia::Ordinal)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AlbumMedia::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Albums::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Albums {
    Table,
    Id,
    AuthorId,
    Name,
    Description,
    CoverMediaId,
    Visibility,
    PublishedAt,
    ImportSource,
    ImportExternalId,
    DeletedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AlbumMedia {
    Table,
    Id,
    AlbumId,
    S3Key,
    MimeType,
    Width,
    Height,
    Ordinal,
    Caption,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
