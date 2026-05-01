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
//! Seed Phase 6 feature gates. Each gate is a `(category=features)` row in
//! the `settings` table; the application reads them via
//! `SettingsService::get_*_enabled()` helpers and admins flip them from
//! the admin Settings page.
//!
//! All gates default to `true` so a fresh install is fully functional.
//! Operators flip individual gates off when they want to mute a feature
//! (e.g. `fb_import_enabled = false` to lock the import endpoint while
//! they audit a partial run).
use sea_orm_migration::prelude::*;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

const PHASE6_GATES: &[&str] = &[
    "poster_posting_enabled",
    "commenter_invites_enabled",
    "public_feed_enabled",
    "fb_import_enabled",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for key in PHASE6_GATES {
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
                            (*key).into(),
                            "true".into(),
                            "features".into(),
                        ])
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for key in PHASE6_GATES {
            manager
                .exec_stmt(
                    Query::delete()
                        .from_table(Settings::Table)
                        .and_where(Expr::col(Settings::Key).eq(*key))
                        .and_where(Expr::col(Settings::Category).eq("features"))
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
}
