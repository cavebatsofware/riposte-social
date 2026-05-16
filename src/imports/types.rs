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
//! Wire-format responses for the admin import endpoints.

use crate::entities::import_job;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ImportJobResponse {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub created_by: Uuid,
    pub parent_job_id: Option<Uuid>,
    pub params: serde_json::Value,
    pub progress: serde_json::Value,
    /// Structured per-job log. Shape: `{ entries: [{ts, level, msg, ctx?}],
    /// dropped: N }`. The admin UI renders this as a chronological event
    /// list so an operator can see exactly where an import went wrong.
    pub log: serde_json::Value,
    pub error: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<import_job::Model> for ImportJobResponse {
    fn from(m: import_job::Model) -> Self {
        let utc = Utc;
        Self {
            id: m.id,
            kind: m.kind,
            status: m.status,
            created_by: m.created_by,
            parent_job_id: m.parent_job_id,
            params: m.params,
            progress: m.progress,
            log: m.log,
            error: m.error,
            last_heartbeat_at: m
                .last_heartbeat_at
                .map(|t| t.with_timezone(&utc).to_rfc3339()),
            started_at: m.started_at.map(|t| t.with_timezone(&utc).to_rfc3339()),
            finished_at: m.finished_at.map(|t| t.with_timezone(&utc).to_rfc3339()),
            created_at: m.created_at.with_timezone(&utc).to_rfc3339(),
            updated_at: m.updated_at.with_timezone(&utc).to_rfc3339(),
        }
    }
}
