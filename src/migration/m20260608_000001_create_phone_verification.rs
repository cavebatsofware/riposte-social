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
//! Dedupe cache for phone verification results, keyed by an HMAC of the number
//! (no raw PII stored). See `entities::phone_verification`.
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PhoneVerifications::Table)
                    .if_not_exists()
                    .col(uuid(PhoneVerifications::Id).primary_key())
                    .col(string_uniq(PhoneVerifications::PhoneHmac))
                    .col(boolean(PhoneVerifications::Valid))
                    .col(string_null(PhoneVerifications::LineType))
                    .col(timestamp_with_time_zone(PhoneVerifications::VerifiedAt))
                    .col(
                        timestamp_with_time_zone(PhoneVerifications::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(PhoneVerifications::UpdatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PhoneVerifications::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PhoneVerifications {
    Table,
    Id,
    PhoneHmac,
    Valid,
    LineType,
    VerifiedAt,
    CreatedAt,
    UpdatedAt,
}
