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
                    .table(ImportJobs::Table)
                    .if_not_exists()
                    .col(uuid(ImportJobs::Id).primary_key())
                    // Job kind: "facebook" today. String (not enum) so adding
                    // future imports is a no-op DDL change.
                    .col(string(ImportJobs::Kind))
                    // Lifecycle status: queued | running | succeeded | failed
                    // | cancelled. Validated at the app layer.
                    .col(string(ImportJobs::Status))
                    // Operator who triggered the job. ON DELETE RESTRICT so a
                    // user can't be removed while their jobs still exist;
                    // the admin UI shows "by <email>" for audit.
                    .col(uuid(ImportJobs::CreatedBy))
                    // Future-proofing: jobs that decompose into sub-jobs
                    // point at their parent here so the admin UI
                    // can group them and resume cleanly.
                    .col(uuid_null(ImportJobs::ParentJobId))
                    // Caller-supplied parameters: source S3 key, visibility
                    // default, etc. Shape varies by `kind`, so jsonb.
                    .col(json_binary(ImportJobs::Params))
                    // Progress shape: { total, processed, succeeded,
                    // skipped, failed }. The worker increments these as it
                    // runs so the admin UI can poll without recomputing.
                    .col(json_binary(ImportJobs::Progress))
                    // Last error message when status = failed. Free-form;
                    // not for end-user display, used by admin UI.
                    .col(text_null(ImportJobs::Error))
                    // Heartbeat written periodically by the worker. The
                    // boot-time sweep does not currently use this (it
                    // blankets all `running` rows), but it is here for
                    // future stuck-job detection without another migration.
                    .col(timestamp_with_time_zone_null(ImportJobs::LastHeartbeatAt))
                    .col(timestamp_with_time_zone_null(ImportJobs::StartedAt))
                    .col(timestamp_with_time_zone_null(ImportJobs::FinishedAt))
                    .col(timestamp_with_time_zone(ImportJobs::CreatedAt))
                    .col(timestamp_with_time_zone(ImportJobs::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_import_jobs_created_by")
                            .from(ImportJobs::Table, ImportJobs::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_import_jobs_parent_job_id")
                            .from(ImportJobs::Table, ImportJobs::ParentJobId)
                            .to(ImportJobs::Table, ImportJobs::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Admin "list active / recent jobs" query is filtered by status and
        // ordered by created_at desc. Index covers it.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_import_jobs_status_created_at")
                    .table(ImportJobs::Table)
                    .col(ImportJobs::Status)
                    .col(ImportJobs::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Boot-time sweep does `WHERE status = 'running'`; covered by the
        // composite above but a dedicated single-column scan helps when the
        // table grows.
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_import_jobs_parent_job_id")
                    .table(ImportJobs::Table)
                    .col(ImportJobs::ParentJobId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ImportJobs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ImportJobs {
    Table,
    Id,
    Kind,
    Status,
    CreatedBy,
    ParentJobId,
    Params,
    Progress,
    Error,
    LastHeartbeatAt,
    StartedAt,
    FinishedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
