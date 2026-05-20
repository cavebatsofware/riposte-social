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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export)]
pub struct FollowStateResponse {
    pub you_follow: bool,
    pub follows_you: bool,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct UserSummary {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub avatar_icon_data: Option<String>,
    pub role: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct FollowsListResponse {
    pub users: Vec<UserSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Deserialize, Default, TS)]
#[serde(default)]
#[ts(export)]
pub struct ListQuery {
    pub cursor: Option<String>,
    #[ts(type = "number | null")]
    pub limit: Option<u64>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct BulkStateQuery {
    /// Comma-separated UUIDs.
    pub user_ids: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct BulkStateEntry {
    pub you_follow: bool,
    pub follows_you: bool,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct BulkStateResponse {
    pub states: HashMap<Uuid, BulkStateEntry>,
}
