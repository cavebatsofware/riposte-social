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
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(InviteCodes::Table)
                    .if_not_exists()
                    .col(uuid(InviteCodes::Id).primary_key())
                    .col(string_uniq(InviteCodes::Code))
                    .col(string_null(InviteCodes::EmailHint))
                    .col(uuid(InviteCodes::CreatedBy))
                    .col(timestamp_with_time_zone(InviteCodes::ExpiresAt))
                    .col(timestamp_with_time_zone_null(InviteCodes::UsedAt))
                    .col(uuid_null(InviteCodes::UsedByUserId))
                    .col(timestamp_with_time_zone_null(InviteCodes::RevokedAt))
                    .col(
                        timestamp_with_time_zone(InviteCodes::CreatedAt)
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_invite_codes_created_by")
                            .from(InviteCodes::Table, InviteCodes::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_invite_codes_used_by_user_id")
                            .from(InviteCodes::Table, InviteCodes::UsedByUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_invite_codes_code")
                    .table(InviteCodes::Table)
                    .col(InviteCodes::Code)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_invite_codes_expires_at")
                    .table(InviteCodes::Table)
                    .col(InviteCodes::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        // FK from users.invite_code_id (added in m20260424) → invite_codes.id.
        // ON DELETE SET NULL: revoking an invite must not cascade-delete the user
        // it onboarded; the historical link is nice-to-have, not load-bearing.
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_users_invite_code_id")
                    .from(Users::Table, Users::InviteCodeId)
                    .to(InviteCodes::Table, InviteCodes::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_users_invite_code_id")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(InviteCodes::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum InviteCodes {
    Table,
    Id,
    Code,
    EmailHint,
    CreatedBy,
    ExpiresAt,
    UsedAt,
    UsedByUserId,
    RevokedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    InviteCodeId,
}
