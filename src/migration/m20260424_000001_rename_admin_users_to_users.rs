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
        let db = manager.get_connection();

        // 1. Remap legacy `viewer` role to `commenter`. Rows will also pick up
        //    user_type = regular_user below; viewer rows were rare/nonexistent
        //    on fresh installs but this keeps old data coherent.
        db.execute_unprepared("UPDATE admin_users SET role = 'commenter' WHERE role = 'viewer'")
            .await?;

        // 2. Add new columns. user_type defaults to admin_user so every pre-existing
        //    row remains a privileged user; the UPDATE below fixes commenter rows.
        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .add_column(
                        string(AdminUsers::UserType)
                            .default("admin_user")
                            .not_null(),
                    )
                    .add_column(string_null(AdminUsers::OidcSub))
                    .add_column(string_null(AdminUsers::DisplayName))
                    .add_column(string_null(AdminUsers::AvatarUrl))
                    .add_column(timestamp_with_time_zone_null(AdminUsers::LastLoginAt))
                    .add_column(uuid_null(AdminUsers::InviteCodeId))
                    .to_owned(),
            )
            .await?;

        // 3. Any row now-role'd as commenter (originally viewer) is a regular_user.
        db.execute_unprepared(
            "UPDATE admin_users SET user_type = 'regular_user' WHERE role = 'commenter'",
        )
        .await?;

        // 4. Partial unique index on oidc_sub (only enforces uniqueness when set).
        db.execute_unprepared(
            "CREATE UNIQUE INDEX idx_admin_users_oidc_sub ON admin_users (oidc_sub) WHERE oidc_sub IS NOT NULL"
        )
        .await?;

        // 5. Secondary index on user_type for admin-UI filtering.
        manager
            .create_index(
                Index::create()
                    .name("idx_admin_users_user_type")
                    .table(AdminUsers::Table)
                    .col(AdminUsers::UserType)
                    .to_owned(),
            )
            .await?;

        // 6. Rename the table admin_users -> users. Postgres updates dependent
        //    indexes and foreign-key constraints (e.g. access_log.admin_user_id)
        //    automatically; only the table name changes.
        manager
            .rename_table(
                Table::rename()
                    .table(AdminUsers::Table, Users::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        manager
            .rename_table(
                Table::rename()
                    .table(Users::Table, AdminUsers::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_admin_users_user_type")
                    .table(AdminUsers::Table)
                    .to_owned(),
            )
            .await?;

        db.execute_unprepared("DROP INDEX IF EXISTS idx_admin_users_oidc_sub")
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(AdminUsers::Table)
                    .drop_column(AdminUsers::InviteCodeId)
                    .drop_column(AdminUsers::LastLoginAt)
                    .drop_column(AdminUsers::AvatarUrl)
                    .drop_column(AdminUsers::DisplayName)
                    .drop_column(AdminUsers::OidcSub)
                    .drop_column(AdminUsers::UserType)
                    .to_owned(),
            )
            .await?;

        db.execute_unprepared("UPDATE admin_users SET role = 'viewer' WHERE role = 'commenter'")
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AdminUsers {
    Table,
    UserType,
    OidcSub,
    DisplayName,
    AvatarUrl,
    LastLoginAt,
    InviteCodeId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
}
