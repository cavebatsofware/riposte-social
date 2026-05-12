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
//! Drop `albums` and `album_media` after the data has been copied into
//! `posts` / `post_media` by the prior two migrations.
//!
//! `down()` is intentionally non-reversible: re-creating the tables
//! without their data would leave the running app pointing at empty
//! shells. Recovery is from a DB backup.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // CASCADE drops the FK from album_media into albums and any
        // dependent indexes in one shot.
        db.execute_unprepared("DROP TABLE IF EXISTS album_media CASCADE")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS albums CASCADE")
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "albums tables drop cannot be rolled back; restore from backup".to_string(),
        ))
    }
}
