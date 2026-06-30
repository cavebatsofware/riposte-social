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
//! Seed the encrypted `secret_resend_api_key` placeholder (the `secret_` prefix
//! makes the settings API encrypt it) used when `email_provider` is "resend".
//! Set by an admin in the settings UI.
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

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
                        Settings::Encrypted,
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        "secret_resend_api_key".into(),
                        "".into(),
                        "email".into(),
                        true.into(),
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
                    .and_where(Expr::col(Settings::Key).eq("secret_resend_api_key"))
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
    Encrypted,
}
