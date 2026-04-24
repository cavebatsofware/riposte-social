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
        // Composite index for IP + action + created_at queries
        // Used by has_recent_contact_submission and has_recent_subscription
        manager
            .create_index(
                Index::create()
                    .name("idx_access_log_ip_action_created")
                    .table(AccessLog::Table)
                    .col(AccessLog::IpAddress)
                    .col(AccessLog::Action)
                    .col(AccessLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Composite index for success flag queries
        manager
            .create_index(
                Index::create()
                    .name("idx_access_log_ip_action_success_created")
                    .table(AccessLog::Table)
                    .col(AccessLog::IpAddress)
                    .col(AccessLog::Action)
                    .col(AccessLog::Success)
                    .col(AccessLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_access_log_ip_action_success_created")
                    .table(AccessLog::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_access_log_ip_action_created")
                    .table(AccessLog::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AccessLog {
    Table,
    IpAddress,
    Action,
    Success,
    CreatedAt,
}
