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
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20251116_000002_create_build_status"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BuildStatus::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BuildStatus::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BuildStatus::Status).string().not_null())
                    .col(ColumnDef::new(BuildStatus::ErrorMessage).string())
                    .col(
                        ColumnDef::new(BuildStatus::StartedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BuildStatus::CompletedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(BuildStatus::TriggeredBy).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_build_status_triggered_by")
                            .from(BuildStatus::Table, BuildStatus::TriggeredBy)
                            .to(AdminUser::Table, AdminUser::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on status for quick lookups
        manager
            .create_index(
                Index::create()
                    .name("idx_build_status_status")
                    .table(BuildStatus::Table)
                    .col(BuildStatus::Status)
                    .to_owned(),
            )
            .await?;

        // Create index on started_at for ordering
        manager
            .create_index(
                Index::create()
                    .name("idx_build_status_started_at")
                    .table(BuildStatus::Table)
                    .col(BuildStatus::StartedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BuildStatus::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum BuildStatus {
    Table,
    Id,
    Status,
    ErrorMessage,
    StartedAt,
    CompletedAt,
    TriggeredBy,
}

#[derive(Iden)]
#[iden = "admin_users"]
enum AdminUser {
    Table,
    Id,
}
