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

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20251122_000001_drop_articles_and_build_status"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop build_status first (no foreign key dependencies)
        manager
            .drop_table(
                Table::drop()
                    .table(BuildStatus::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        // Drop articles table
        manager
            .drop_table(Table::drop().table(Articles::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // This migration is intentionally not reversible.
        // The articles and build_status tables are being permanently removed
        // as part of switching to static Astro content collections.
        Err(DbErr::Migration(
            "This migration cannot be reversed. Articles are now managed as static files.".into(),
        ))
    }
}

#[derive(Iden)]
enum BuildStatus {
    Table,
}

#[derive(Iden)]
enum Articles {
    Table,
}
