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
        // Drop mac_address column as it's not available from HTTP requests
        manager
            .alter_table(
                Table::alter()
                    .table(AccessLog::Table)
                    .drop_column(AccessLog::MacAddress)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Re-add mac_address column if rolling back
        manager
            .alter_table(
                Table::alter()
                    .table(AccessLog::Table)
                    .add_column(ColumnDef::new(AccessLog::MacAddress).string())
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum AccessLog {
    Table,
    MacAddress,
}
