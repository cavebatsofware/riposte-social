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

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AccessLog::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AccessLog::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AccessLog::AccessCode).string().not_null())
                    .col(ColumnDef::new(AccessLog::IpAddress).string())
                    .col(ColumnDef::new(AccessLog::UserAgent).string())
                    .col(ColumnDef::new(AccessLog::MacAddress).string())
                    .col(ColumnDef::new(AccessLog::Count).integer())
                    .col(ColumnDef::new(AccessLog::LastAccessTime).timestamp_with_time_zone())
                    .col(ColumnDef::new(AccessLog::LastDeltaAccess).big_integer())
                    .col(ColumnDef::new(AccessLog::Action).string().not_null())
                    .col(ColumnDef::new(AccessLog::Success).boolean().not_null())
                    .col(
                        ColumnDef::new(AccessLog::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Add indexes for performance
        manager
            .create_index(
                Index::create()
                    .name("idx_access_log_access_code")
                    .table(AccessLog::Table)
                    .col(AccessLog::AccessCode)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_access_log_ip_address")
                    .table(AccessLog::Table)
                    .col(AccessLog::IpAddress)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_access_log_created_at")
                    .table(AccessLog::Table)
                    .col(AccessLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_access_log_last_delta_access")
                    .table(AccessLog::Table)
                    .col(AccessLog::LastDeltaAccess)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AccessLog::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum AccessLog {
    Table,
    Id,
    AccessCode,
    IpAddress,
    UserAgent,
    MacAddress,
    Count,
    LastAccessTime,
    LastDeltaAccess,
    Action,
    Success,
    CreatedAt,
}
