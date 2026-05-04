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
//! `category_member` join table for `user_list` category access.
//!
//! Each row grants `user_id` access to category `category_id`. Composite
//! PK on `(category_id, user_id)`. Both FKs cascade on delete so removing
//! a category or deactivating-via-delete a user automatically reaps the
//! membership row.
//!
//! The per-viewer subquery (`SELECT category_id FROM category_member
//! WHERE user_id = $viewer`) drives the feed predicate; the
//! `idx_category_member_user_id` index keeps that lookup fast.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CategoryMember::Table)
                    .if_not_exists()
                    .col(uuid(CategoryMember::CategoryId))
                    .col(uuid(CategoryMember::UserId))
                    .col(timestamp_with_time_zone(CategoryMember::CreatedAt))
                    .primary_key(
                        Index::create()
                            .col(CategoryMember::CategoryId)
                            .col(CategoryMember::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_category_member_category_id")
                            .from(CategoryMember::Table, CategoryMember::CategoryId)
                            .to(Categories::Table, Categories::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_category_member_user_id")
                            .from(CategoryMember::Table, CategoryMember::UserId)
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
                    .name("idx_category_member_user_id")
                    .table(CategoryMember::Table)
                    .col(CategoryMember::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CategoryMember::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum CategoryMember {
    Table,
    CategoryId,
    UserId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Categories {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
