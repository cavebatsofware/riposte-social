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
use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};

/// Job kinds. Adding a new kind is one new constant; the column itself is a
/// string and validation lives in the app layer.
pub const KIND_FACEBOOK: &str = "facebook";

/// Lifecycle status values. Stored as strings rather than a Postgres enum so
/// adding a state (e.g. `paused` for resumable runs) costs no DDL.
pub const STATUS_QUEUED: &str = "queued";
pub const STATUS_RUNNING: &str = "running";
pub const STATUS_SUCCEEDED: &str = "succeeded";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_CANCELLED: &str = "cancelled";

pub fn is_terminal_status(status: &str) -> bool {
    matches!(status, STATUS_SUCCEEDED | STATUS_FAILED | STATUS_CANCELLED)
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "import_jobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub created_by: Uuid,
    pub parent_job_id: Option<Uuid>,
    /// Caller-supplied parameters; shape varies by `kind`. For
    /// `KIND_FACEBOOK` see `crate::imports::facebook::FacebookImportParams`.
    pub params: serde_json::Value,
    /// Progress counters. The worker increments these as it runs so the
    /// admin UI can poll without recomputing aggregates. See
    /// `crate::imports::JobProgress` for the canonical shape.
    pub progress: serde_json::Value,
    pub error: Option<String>,
    pub last_heartbeat_at: Option<DateTimeWithTimeZone>,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// Structured per-job event log. Shape: `{ "entries": [{ts, level,
    /// msg, ctx?}], "dropped": N }`. See `crate::imports::JobLog` for the
    /// canonical type and the bounded-buffer rules workers follow when
    /// appending. Surfaced through the admin UI for diagnosis.
    pub log: serde_json::Value,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedBy",
        to = "super::user::Column::Id",
        on_delete = "Restrict"
    )]
    CreatedBy,
    /// Self-FK for sub-jobs that decompose a parent job. Cascade so deleting
    /// a parent removes its children; we do not currently expose deletion
    /// (jobs are immutable history) but the schema supports it.
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentJobId",
        to = "Column::Id",
        on_delete = "Cascade"
    )]
    Parent,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedBy.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now: DateTimeWithTimeZone = chrono::Utc::now().into();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
