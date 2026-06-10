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
//! Seed the business-module secret settings as empty, encrypted placeholders
//! so they appear in the admin settings UI for an admin to fill. The `secret_`
//! key prefix makes the admin settings API store updates via `set_encrypted`;
//! an empty value reads as "unset" (and disables the related check). Real
//! values are entered through the settings UI, never seeded.
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

const SECRETS: &[&str] = &["secret_turnstile", "secret_order_sms_to"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for key in SECRETS {
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
                            (*key).into(),
                            "".into(),
                            "business".into(),
                            true.into(),
                        ])
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for key in SECRETS {
            manager
                .exec_stmt(
                    Query::delete()
                        .from_table(Settings::Table)
                        .and_where(Expr::col(Settings::Key).eq(*key))
                        .and_where(Expr::col(Settings::Category).eq("business"))
                        .to_owned(),
                )
                .await?;
        }
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
