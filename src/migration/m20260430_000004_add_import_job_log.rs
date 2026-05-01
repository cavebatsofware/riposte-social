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
        // Per-job structured log so the admin UI can show exactly what
        // happened during a run — boot, parse, dedup, per-post failures,
        // termination. The shape is `{ "entries": [...], "dropped": N }`
        // (see `JobLog` in `src/imports/mod.rs`); workers append events
        // and trim oldest entries past a cap, incrementing `dropped` so
        // the UI can render "N earlier entries omitted".
        manager
            .alter_table(
                Table::alter()
                    .table(ImportJobs::Table)
                    .add_column(
                        ColumnDef::new(ImportJobs::Log)
                            .json_binary()
                            .not_null()
                            .default(serde_json::json!({"entries": [], "dropped": 0})),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ImportJobs::Table)
                    .drop_column(ImportJobs::Log)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ImportJobs {
    Table,
    Log,
}
