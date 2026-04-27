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
//! Drop the `user_type` column from `users`.
//!
//! `user_type` was added in m20260424 as a coarse admin_user/regular_user
//! partition, but the `role` column (administrator/poster/commenter) already
//! encodes every distinction we need. Two columns expressing the same fact
//! drift; one is enough.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_admin_users_user_type")
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::UserType)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(
                        ColumnDef::new(Users::UserType)
                            .string()
                            .not_null()
                            .default("admin_user"),
                    )
                    .to_owned(),
            )
            .await?;

        // Re-derive user_type from role for any existing rows.
        db.execute_unprepared(
            "UPDATE users SET user_type = CASE WHEN role = 'commenter' THEN 'regular_user' ELSE 'admin_user' END",
        )
        .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_admin_users_user_type")
                    .table(Users::Table)
                    .col(Users::UserType)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    UserType,
}
