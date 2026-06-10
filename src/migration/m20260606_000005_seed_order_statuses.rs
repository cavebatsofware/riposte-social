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
//! Seed the configurable order-status workflow as a plaintext, comma-separated
//! setting. Admins edit the list in the settings UI; the orders admin page uses
//! it to populate the status dropdown.
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

const DEFAULT_STATUSES: &str =
    "placed, pending deposit, material prep, construction, finishing, complete, cancelled";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(Settings::Table)
                    .columns([
                        Settings::Id,
                        Settings::Key,
                        Settings::Value,
                        Settings::Category,
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        "order_statuses".into(),
                        DEFAULT_STATUSES.into(),
                        "business".into(),
                    ])
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .exec_stmt(
                Query::delete()
                    .from_table(Settings::Table)
                    .and_where(Expr::col(Settings::Key).eq("order_statuses"))
                    .and_where(Expr::col(Settings::Category).eq("business"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Settings {
    Table,
    Id,
    Key,
    Value,
    Category,
}
