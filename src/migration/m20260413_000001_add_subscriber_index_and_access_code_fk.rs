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
        // L15: Add index on subscribers.verification_token for faster lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_subscribers_verification_token")
                    .table(Subscribers::Table)
                    .col(Subscribers::VerificationToken)
                    .to_owned(),
            )
            .await?;

        // L16: Add foreign key on access_codes.created_by -> admin_users.id
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_access_codes_created_by")
                    .from(AccessCodes::Table, AccessCodes::CreatedBy)
                    .to(AdminUsers::Table, AdminUsers::Id)
                    .on_delete(ForeignKeyAction::Restrict)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_access_codes_created_by")
                    .table(AccessCodes::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_subscribers_verification_token")
                    .table(Subscribers::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Subscribers {
    Table,
    VerificationToken,
}

#[derive(DeriveIden)]
enum AccessCodes {
    Table,
    CreatedBy,
}

#[derive(DeriveIden)]
enum AdminUsers {
    Table,
    Id,
}
