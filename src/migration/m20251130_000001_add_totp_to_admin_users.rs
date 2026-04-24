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
        // Add totp_secret column (nullable, stores base32-encoded secret)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(string_null(AdminUsers::TotpSecret))
                    .to_owned(),
            )
            .await?;

        // Add totp_enabled column (boolean, defaults to false)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(boolean_null(AdminUsers::TotpEnabled).default(false))
                    .to_owned(),
            )
            .await?;

        // Add totp_enabled_at column (nullable timestamp)
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(timestamp_with_time_zone_null(AdminUsers::TotpEnabledAt))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::TotpSecret)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::TotpEnabled)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::TotpEnabledAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AdminUsers {
    Table,
    TotpSecret,
    TotpEnabled,
    TotpEnabledAt,
}
