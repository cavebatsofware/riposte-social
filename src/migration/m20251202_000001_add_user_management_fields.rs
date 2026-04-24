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
        // Add active column (boolean, defaults to true)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(boolean(AdminUsers::Active).default(true).not_null())
                    .to_owned(),
            )
            .await?;

        // Add deactivated_at column (nullable timestamp)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(timestamp_with_time_zone_null(AdminUsers::DeactivatedAt))
                    .to_owned(),
            )
            .await?;

        // Add force_password_change column (boolean, defaults to false)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(
                        boolean(AdminUsers::ForcePasswordChange)
                            .default(false)
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Add password_reset_token column (nullable text)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(string_null(AdminUsers::PasswordResetToken))
                    .to_owned(),
            )
            .await?;

        // Add password_reset_token_expires_at column (nullable timestamp)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(timestamp_with_time_zone_null(
                        AdminUsers::PasswordResetTokenExpiresAt,
                    ))
                    .to_owned(),
            )
            .await?;

        // Create index on password_reset_token for faster lookups
        manager
            .create_index(
                Index::create()
                    .name("idx_admin_users_password_reset_token")
                    .table(AdminUsers::Table)
                    .col(AdminUsers::PasswordResetToken)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop index first
        manager
            .drop_index(
                Index::drop()
                    .name("idx_admin_users_password_reset_token")
                    .table(AdminUsers::Table)
                    .to_owned(),
            )
            .await?;

        // Drop columns in reverse order
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::PasswordResetTokenExpiresAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::PasswordResetToken)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::ForcePasswordChange)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::DeactivatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::Active)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AdminUsers {
    Table,
    Active,
    DeactivatedAt,
    ForcePasswordChange,
    PasswordResetToken,
    PasswordResetTokenExpiresAt,
}
