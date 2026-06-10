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
//! Generic customer-order intake table for the business module. The customer
//! identity columns hold ciphertext (see `entities::order`); `details` is an
//! opaque JSON string of form-specific fields. `estimate_total` is captured at
//! intake; `total` and `deposit` are set later when the order is confirmed.
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Orders::Table)
                    .if_not_exists()
                    .col(uuid(Orders::Id).primary_key())
                    .col(string(Orders::Kind))
                    .col(string(Orders::Title))
                    .col(text(Orders::Summary))
                    .col(string(Orders::CustomerName))
                    .col(string(Orders::CustomerPhone))
                    .col(string_null(Orders::CustomerEmail))
                    .col(double_null(Orders::EstimateTotal))
                    .col(double_null(Orders::Total))
                    .col(double_null(Orders::Deposit))
                    .col(text(Orders::Details))
                    .col(string(Orders::Status).default("placed"))
                    .col(
                        timestamp_with_time_zone(Orders::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        timestamp_with_time_zone(Orders::UpdatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on created_at for recent-first listing.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_orders_created_at")
                    .table(Orders::Table)
                    .col(Orders::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Index on kind for filtering by intake form.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_orders_kind")
                    .table(Orders::Table)
                    .col(Orders::Kind)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Orders::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Orders {
    Table,
    Id,
    Kind,
    Title,
    Summary,
    CustomerName,
    CustomerPhone,
    CustomerEmail,
    EstimateTotal,
    Total,
    Deposit,
    Details,
    Status,
    CreatedAt,
    UpdatedAt,
}
