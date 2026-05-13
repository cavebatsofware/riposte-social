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
//! Add `visibility` and `created_by` to `categories`.
//!
//! `visibility` defaults to `public` (existing rows match the previous "no
//! access control" behavior). The application accepts the four post-level
//! tiers plus a fifth `user_list` tier; validation lives in the
//! visibility module.
//!
//! `created_by` is a nullable FK to `users.id` with `ON DELETE SET NULL`.
//! NULL means "system-owned" — only admins can manage those
//! rows. New categories created via the API populate it from the caller.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Categories::Table)
                    .add_column(string(Categories::Visibility).default("public").not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Categories::Table)
                    .add_column(uuid_null(Categories::CreatedBy))
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk_categories_created_by")
                            .from_tbl(Categories::Table)
                            .from_col(Categories::CreatedBy)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_categories_created_by")
                    .table(Categories::Table)
                    .col(Categories::CreatedBy)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_categories_created_by")
                    .table(Categories::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Categories::Table)
                    .drop_foreign_key(Alias::new("fk_categories_created_by"))
                    .drop_column(Categories::CreatedBy)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Categories::Table)
                    .drop_column(Categories::Visibility)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Categories {
    Table,
    Visibility,
    CreatedBy,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
