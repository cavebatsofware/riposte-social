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
//! profile fields on users.
//!
//! Adds `handle`, `bio`, `pronouns`, `avatar_s3_key`. Backfills `handle` from
//! the email local-part with collision-safe suffixing, then enforces NOT NULL
//! and UNIQUE.
//!
//! Length / charset rules (3-30 chars, lowercase ASCII / digits / `_` / `-`)
//! are validated at the application layer; the DB just guards uniqueness.
//! `bio` (500 char) and `pronouns` (30 char) caps are also app-layer.

use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add nullable columns first so the table modification doesn't trip
        // on existing rows. Handle is filled in a follow-up step then made
        // NOT NULL.
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(string_null(Users::Handle))
                    .add_column(text_null(Users::Bio))
                    .add_column(string_null(Users::Pronouns))
                    .add_column(string_null(Users::AvatarS3Key))
                    .to_owned(),
            )
            .await?;

        // Backfill handle from email local-part. Strip non-allowed chars,
        // truncate to 30, then append a numeric suffix on collision.
        // The bootstrap admin is the only row in a fresh deployment, but
        // this loop handles N rows safely if the migration runs against
        // a populated DB.
        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            DO $$
            DECLARE
                u RECORD;
                base TEXT;
                candidate TEXT;
                suffix INT;
            BEGIN
                FOR u IN SELECT id, email FROM users WHERE handle IS NULL LOOP
                    base := lower(split_part(u.email, '@', 1));
                    -- replace any char that isn't [a-z0-9_-] with '-'
                    base := regexp_replace(base, '[^a-z0-9_\-]', '-', 'g');
                    -- collapse leading/trailing dashes
                    base := regexp_replace(base, '^-+|-+$', '', 'g');
                    IF length(base) < 3 THEN
                        base := base || repeat('x', 3 - length(base));
                    END IF;
                    base := substring(base FROM 1 FOR 30);
                    candidate := base;
                    suffix := 1;
                    WHILE EXISTS(SELECT 1 FROM users WHERE handle = candidate) LOOP
                        candidate := substring(base FROM 1 FOR (30 - length(suffix::text) - 1)) || '-' || suffix::text;
                        suffix := suffix + 1;
                    END LOOP;
                    UPDATE users SET handle = candidate WHERE id = u.id;
                END LOOP;
            END $$;
            "#,
        )
        .await?;

        // Enforce NOT NULL on handle now that every row has a value.
        db.execute_unprepared("ALTER TABLE users ALTER COLUMN handle SET NOT NULL")
            .await?;

        // Unique index on handle. Equivalent to a UNIQUE constraint; an index
        // is what we'd add anyway for the by-handle lookup.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_users_handle")
                    .table(Users::Table)
                    .col(Users::Handle)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_users_handle")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::Handle)
                    .drop_column(Users::Bio)
                    .drop_column(Users::Pronouns)
                    .drop_column(Users::AvatarS3Key)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Handle,
    Bio,
    Pronouns,
    AvatarS3Key,
}
