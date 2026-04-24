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
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Settings::Table)
                    .if_not_exists()
                    .col(uuid(Settings::Id).primary_key())
                    .col(string(Settings::Key))
                    .col(string(Settings::Value))
                    .col(string_null(Settings::Category))
                    .col(uuid_null(Settings::EntityId))
                    .col(
                        timestamp_with_time_zone(Settings::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(Settings::UpdatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on key + category + entity_id to prevent duplicates
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_settings_key_category_entity")
                    .table(Settings::Table)
                    .col(Settings::Key)
                    .col(Settings::Category)
                    .col(Settings::EntityId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Insert default admin_registration_enabled setting
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
                        "admin_registration_enabled".into(),
                        "true".into(),
                        "system".into(),
                    ])
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Settings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Settings {
    Table,
    Id,
    Key,
    Value,
    Category,
    EntityId,
    CreatedAt,
    UpdatedAt,
}
