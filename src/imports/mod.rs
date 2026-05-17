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
//! Import job runner.
//!
//! A homegrown alternative to a real queue (apalis, sqlxmq) sized to the
//! workload here: one slow admin operation, restart-resilient, with bounded
//! parallelism inside each job rather than across them.
//!
//! Lifecycle. The HTTP handler inserts an `import_job` row in `queued`,
//! returns the id, then `tokio::spawn`s a worker. The worker flips status
//! to `running`, processes the work, writes progress + heartbeat as it
//! goes, and ends in a terminal state. All state lives on the row so the
//! admin UI just polls `GET /api/admin/imports/{id}`.
//!
//! Restart safety. On boot we sweep any `running` rows from a previous
//! process and flip them to `failed` with an explanatory error. We do not
//! auto-resume yet; the row's progress counters are preserved so a future
//! "Resume" button can pick up where the previous run left off. Resume itself works through the existing
//! `(import_source, import_external_id)` dedup on `posts`, not through
//! per-task durability.
//!
//! Parallelism. Each worker decides its own concurrency via
//! `futures::stream::buffer_unordered` over the per-item work list.
//! Sub-jobs (children with `parent_job_id` set) are supported by the
//! schema but not used yet; introducing them costs no DDL.

pub mod facebook;
pub mod handlers;
pub mod types;

use crate::entities::{import_job, ImportJob};
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical progress shape stored in `import_job.progress` as JSONB. The
/// worker is responsible for incrementing the relevant counters and
/// persisting back to the row. Fields are i64 (not u64) so JSON round-trips
/// cleanly through serde_json.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    /// Total items the worker plans to process. Set once after the
    /// worker scans its input. Zero before the scan completes.
    pub total: i64,
    /// Items the worker has finished considering. Sum of succeeded +
    /// skipped + failed once the run finishes.
    pub processed: i64,
    pub succeeded: i64,
    /// Items the worker decided not to import (e.g. dedup hit on
    /// `(import_source, import_external_id)`).
    pub skipped: i64,
    pub failed: i64,
}

impl JobProgress {
    pub fn into_value(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

/// Maximum log entries retained per job. When exceeded the oldest entries
/// are dropped (the count is preserved in `JobLog::dropped` so the admin
/// UI can show "N earlier entries omitted").
pub const JOB_LOG_CAP: usize = 500;

/// Severity label on a log entry. Maps to a UI badge color.
pub const LOG_LEVEL_INFO: &str = "info";
pub const LOG_LEVEL_WARN: &str = "warn";
pub const LOG_LEVEL_ERROR: &str = "error";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLogEntry {
    /// RFC3339 timestamp. Set by `append_log` so workers do not need to
    /// pass it.
    pub ts: String,
    /// Severity. One of `LOG_LEVEL_*`.
    pub level: String,
    /// Free-form human-readable message.
    pub msg: String,
    /// Optional structured context (post id, archive size, etc.). Use
    /// sparingly: one log entry per significant event, not per item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobLog {
    #[serde(default)]
    pub entries: Vec<JobLogEntry>,
    /// Count of entries that were dropped from the head when the buffer
    /// hit `JOB_LOG_CAP`. The UI shows this as "N earlier entries omitted"
    /// so admins know they're not seeing the full history.
    #[serde(default)]
    pub dropped: i64,
}

impl JobLog {
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    pub fn into_value(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Append one entry to the job's log column. Performs a select-modify-
/// update round trip; we accept that cost because (a) log writes are
/// infrequent (significant events only, not per-item progress) and (b)
/// the row is owned by the spawned worker so there's no concurrent
/// writer to fight with.
///
/// Trims the oldest entries past `JOB_LOG_CAP`, accumulating the drop
/// count so the UI can render "N earlier entries omitted". Best-effort:
/// log persistence failures are logged via `tracing` and swallowed so a
/// transient DB hiccup does not abort the import.
pub async fn append_log<C>(
    db: &C,
    job_id: Uuid,
    level: &str,
    msg: impl Into<String>,
    ctx: Option<serde_json::Value>,
) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let row = match ImportJob::find_by_id(job_id).one(db).await? {
        Some(r) => r,
        None => return Ok(()),
    };
    let mut log = JobLog::from_value(&row.log);
    log.entries.push(JobLogEntry {
        ts: chrono::Utc::now().to_rfc3339(),
        level: level.to_string(),
        msg: msg.into(),
        ctx,
    });
    if log.entries.len() > JOB_LOG_CAP {
        let overflow = log.entries.len() - JOB_LOG_CAP;
        log.entries.drain(..overflow);
        log.dropped += overflow as i64;
    }

    ImportJob::update_many()
        .col_expr(
            import_job::Column::Log,
            sea_orm::sea_query::Expr::value(log.into_value()),
        )
        .filter(import_job::Column::Id.eq(job_id))
        .exec(db)
        .await?;
    Ok(())
}

/// Boot-time sweep: any rows still marked `running` from a previous process
/// are flipped to `failed` with an explanatory error and `finished_at = now()`.
/// Single-instance deployment assumed. With a future multi-instance setup
/// we would gate this on `last_heartbeat_at` staleness instead of blanket-
/// failing every running row.
pub async fn sweep_stale_running_jobs<C>(db: &C) -> Result<u64, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
    let result = ImportJob::update_many()
        .col_expr(
            import_job::Column::Status,
            sea_orm::sea_query::Expr::value(import_job::STATUS_FAILED),
        )
        .col_expr(
            import_job::Column::Error,
            sea_orm::sea_query::Expr::value("interrupted by server restart"),
        )
        .col_expr(
            import_job::Column::FinishedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(import_job::Column::Status.eq(import_job::STATUS_RUNNING))
        .exec(db)
        .await?;

    if result.rows_affected > 0 {
        tracing::warn!(
            "Boot-time sweep: marked {} interrupted import_job(s) as failed",
            result.rows_affected
        );
    }
    Ok(result.rows_affected)
}

/// Update the heartbeat timestamp on a running job. Cheap one-row write;
/// workers call this periodically (every few seconds) so a future
/// "stuck job" detector can flag rows whose `last_heartbeat_at` is too old.
#[allow(dead_code)]
pub async fn touch_heartbeat<C>(db: &C, job_id: Uuid) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
    ImportJob::update_many()
        .col_expr(
            import_job::Column::LastHeartbeatAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(import_job::Column::Id.eq(job_id))
        .exec(db)
        .await?;
    Ok(())
}

/// Persist updated progress counters on a running job. Workers call this
/// after each item or batch of items so the admin UI can poll progress in
/// near-real-time.
#[allow(dead_code)]
pub async fn update_progress<C>(
    db: &C,
    job_id: Uuid,
    progress: &JobProgress,
) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    ImportJob::update_many()
        .col_expr(
            import_job::Column::Progress,
            sea_orm::sea_query::Expr::value(progress.clone().into_value()),
        )
        .filter(import_job::Column::Id.eq(job_id))
        .exec(db)
        .await?;
    Ok(())
}

/// Mark a job terminal (succeeded / failed / cancelled). Sets `finished_at`
/// and an optional error message. Idempotent: callers can invoke this even
/// if the row is already terminal; the status is overwritten with the new
/// value.
#[allow(dead_code)]
pub async fn finish<C>(
    db: &C,
    job_id: Uuid,
    status: &str,
    error: Option<String>,
) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
    let mut update = ImportJob::update_many()
        .col_expr(
            import_job::Column::Status,
            sea_orm::sea_query::Expr::value(status),
        )
        .col_expr(
            import_job::Column::FinishedAt,
            sea_orm::sea_query::Expr::value(now),
        );
    if let Some(err) = error {
        update = update.col_expr(
            import_job::Column::Error,
            sea_orm::sea_query::Expr::value(err),
        );
    }
    update
        .filter(import_job::Column::Id.eq(job_id))
        .exec(db)
        .await?;
    Ok(())
}

/// Flip a queued job to running, setting `started_at` and the first
/// heartbeat. Called by the spawned worker as its first DB write.
#[allow(dead_code)]
pub async fn mark_running<C>(db: &C, job_id: Uuid) -> Result<(), sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let now: sea_orm::prelude::DateTimeWithTimeZone = chrono::Utc::now().into();
    ImportJob::update_many()
        .col_expr(
            import_job::Column::Status,
            sea_orm::sea_query::Expr::value(import_job::STATUS_RUNNING),
        )
        .col_expr(
            import_job::Column::StartedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .col_expr(
            import_job::Column::LastHeartbeatAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .filter(import_job::Column::Id.eq(job_id))
        .filter(import_job::Column::Status.eq(import_job::STATUS_QUEUED))
        .exec(db)
        .await?;
    Ok(())
}

/// Insert a new job in `queued` state. Returns the inserted row.
#[allow(dead_code)]
pub async fn enqueue<C>(
    db: &C,
    kind: &str,
    created_by: Uuid,
    params: serde_json::Value,
    parent_job_id: Option<Uuid>,
) -> Result<import_job::Model, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let row = import_job::ActiveModel {
        id: Set(Uuid::new_v4()),
        kind: Set(kind.to_string()),
        status: Set(import_job::STATUS_QUEUED.to_string()),
        created_by: Set(created_by),
        parent_job_id: Set(parent_job_id),
        params: Set(params),
        progress: Set(JobProgress::default().into_value()),
        log: Set(JobLog::default().into_value()),
        error: Set(None),
        last_heartbeat_at: Set(None),
        started_at: Set(None),
        finished_at: Set(None),
        ..Default::default()
    };
    row.insert(db).await
}
