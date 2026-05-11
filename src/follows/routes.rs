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
//! Follower-graph HTTP handlers.
//!
//! - `POST   /api/users/{user_id}/follow`        create the edge (idempotent)
//! - `DELETE /api/users/{user_id}/follow`        remove the edge (idempotent)
//! - `GET    /api/users/{user_id}/followers`     paginated list of followers
//! - `GET    /api/users/{user_id}/following`     paginated list of followees
//! - `GET    /api/me/follows/state?user_ids=...` bulk follow-state lookup
//!
//! Writes require `require_authenticated`. Reads require auth too: the
//! follower graph isn't part of the public-feed surface today.

use crate::admin::UserAuth;
use crate::entities::{follow, user, Follow, User};
use crate::errors::{AppError, AppResult};
use crate::follows::fetch_follow_states;
use crate::profile::avatar_url_for;
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, JoinType, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct FollowsState {
    pub db: DatabaseConnection,
}

const LIST_LIMIT_DEFAULT: u64 = 30;
const LIST_LIMIT_MAX: u64 = 100;

/// Cap on `?user_ids=` parameter length for the bulk state endpoint. The
/// rail and feed render at most a few dozen visible avatars at once; 200
/// leaves headroom without inviting an arbitrary scan.
const BULK_STATE_MAX: usize = 200;

pub fn follows_write_routes() -> Router<FollowsState> {
    Router::new().route(
        "/api/users/{user_id}/follow",
        post(create_follow).delete(delete_follow),
    )
}

pub fn follows_read_routes() -> Router<FollowsState> {
    Router::new()
        .route("/api/users/{user_id}/followers", get(list_followers))
        .route("/api/users/{user_id}/following", get(list_following))
        .route("/api/me/follows/state", get(get_follow_state))
}

// ==================== wire format ====================

#[derive(Serialize)]
pub struct FollowStateResponse {
    pub you_follow: bool,
    pub follows_you: bool,
}

#[derive(Serialize)]
pub struct UserSummary {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
}

#[derive(Serialize)]
pub struct FollowsListResponse {
    pub users: Vec<UserSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Deserialize)]
pub struct BulkStateQuery {
    /// Comma-separated UUIDs.
    pub user_ids: String,
}

#[derive(Serialize)]
pub struct BulkStateEntry {
    pub you_follow: bool,
    pub follows_you: bool,
}

#[derive(Serialize)]
pub struct BulkStateResponse {
    pub states: HashMap<Uuid, BulkStateEntry>,
}

// ==================== handlers ====================

async fn create_follow(
    State(state): State<FollowsState>,
    Extension(user): Extension<UserAuth>,
    Path(target_id): Path<Uuid>,
) -> AppResult<Json<FollowStateResponse>> {
    if target_id == user.id {
        return Err(AppError::ValidationError(
            "You cannot follow yourself".to_string(),
        ));
    }
    ensure_target_visible(&state.db, target_id).await?;

    // OnConflict::DoNothing bypasses ActiveModelBehavior::before_save,
    // which is why created_at is set inline.
    let am = follow::ActiveModel {
        follower_id: Set(user.id),
        followed_id: Set(target_id),
        created_at: Set(Utc::now().into()),
    };
    let conflict = OnConflict::columns([follow::Column::FollowerId, follow::Column::FollowedId])
        .do_nothing()
        .to_owned();
    let outcome = Follow::insert(am)
        .on_conflict(conflict)
        .exec(&state.db)
        .await;
    match outcome {
        Ok(_) => {
            crate::metrics::FOLLOWS_TOTAL
                .with_label_values(&["add"])
                .inc();
        }
        Err(DbErr::RecordNotInserted) => {}
        Err(e) => return Err(e.into()),
    }

    Ok(Json(load_pair_state(&state.db, user.id, target_id).await?))
}

async fn delete_follow(
    State(state): State<FollowsState>,
    Extension(user): Extension<UserAuth>,
    Path(target_id): Path<Uuid>,
) -> AppResult<Json<FollowStateResponse>> {
    if target_id == user.id {
        return Err(AppError::ValidationError(
            "You cannot unfollow yourself".to_string(),
        ));
    }
    ensure_target_visible(&state.db, target_id).await?;

    let result = Follow::delete_many()
        .filter(follow::Column::FollowerId.eq(user.id))
        .filter(follow::Column::FollowedId.eq(target_id))
        .exec(&state.db)
        .await?;
    if result.rows_affected > 0 {
        crate::metrics::FOLLOWS_TOTAL
            .with_label_values(&["remove"])
            .inc();
    }

    Ok(Json(load_pair_state(&state.db, user.id, target_id).await?))
}

/// Cursor format `{created_at_rfc3339}_{follower_id}`, sorted descending
/// by `(created_at, follower_id)` so newest follower appears first and
/// ties don't lose rows. The active-user gate is pushed into the SQL
/// query via an INNER JOIN so paginated counts stay consistent across
/// pages; post-filtering after LIMIT would let cursors terminate early
/// when soft-deleted rows fall inside the fetched window.
async fn list_followers(
    State(state): State<FollowsState>,
    Extension(_user): Extension<UserAuth>,
    Path(target_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<FollowsListResponse>> {
    ensure_target_visible(&state.db, target_id).await?;
    let limit = query
        .limit
        .unwrap_or(LIST_LIMIT_DEFAULT)
        .clamp(1, LIST_LIMIT_MAX);

    let mut q = Follow::find()
        .filter(follow::Column::FollowedId.eq(target_id))
        .join(JoinType::InnerJoin, follow::Relation::Follower.def())
        .filter(user::Column::Active.eq(true))
        .order_by_desc(follow::Column::CreatedAt)
        .order_by_desc(follow::Column::FollowerId);

    if let Some((c_at, c_id)) = query.cursor.as_deref().and_then(parse_cursor) {
        q = q.filter(
            sea_orm::Condition::any()
                .add(follow::Column::CreatedAt.lt(c_at))
                .add(
                    sea_orm::Condition::all()
                        .add(follow::Column::CreatedAt.eq(c_at))
                        .add(follow::Column::FollowerId.lt(c_id)),
                ),
        );
    }

    // Fetch limit+1 to detect a next page without a count query.
    let rows = q.limit(limit + 1).all(&state.db).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<&follow::Model> = rows.iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| format_cursor(r.created_at, r.follower_id))
    } else {
        None
    };
    let kept_ids: Vec<Uuid> = page.iter().map(|r| r.follower_id).collect();
    let users = fetch_user_summaries(&state.db, &kept_ids).await?;
    Ok(Json(FollowsListResponse { users, next_cursor }))
}

async fn list_following(
    State(state): State<FollowsState>,
    Extension(_user): Extension<UserAuth>,
    Path(target_id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<FollowsListResponse>> {
    ensure_target_visible(&state.db, target_id).await?;
    let limit = query
        .limit
        .unwrap_or(LIST_LIMIT_DEFAULT)
        .clamp(1, LIST_LIMIT_MAX);

    let mut q = Follow::find()
        .filter(follow::Column::FollowerId.eq(target_id))
        .join(JoinType::InnerJoin, follow::Relation::Followed.def())
        .filter(user::Column::Active.eq(true))
        .order_by_desc(follow::Column::CreatedAt)
        .order_by_desc(follow::Column::FollowedId);

    if let Some((c_at, c_id)) = query.cursor.as_deref().and_then(parse_cursor) {
        q = q.filter(
            sea_orm::Condition::any()
                .add(follow::Column::CreatedAt.lt(c_at))
                .add(
                    sea_orm::Condition::all()
                        .add(follow::Column::CreatedAt.eq(c_at))
                        .add(follow::Column::FollowedId.lt(c_id)),
                ),
        );
    }

    let rows = q.limit(limit + 1).all(&state.db).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<&follow::Model> = rows.iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| format_cursor(r.created_at, r.followed_id))
    } else {
        None
    };
    let kept_ids: Vec<Uuid> = page.iter().map(|r| r.followed_id).collect();
    let users = fetch_user_summaries(&state.db, &kept_ids).await?;
    Ok(Json(FollowsListResponse { users, next_cursor }))
}

async fn get_follow_state(
    State(state): State<FollowsState>,
    Extension(user): Extension<UserAuth>,
    Query(query): Query<BulkStateQuery>,
) -> AppResult<Json<BulkStateResponse>> {
    let ids: Vec<Uuid> = query
        .user_ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();
    if ids.len() > BULK_STATE_MAX {
        return Err(AppError::ValidationError(format!(
            "At most {} user_ids per request",
            BULK_STATE_MAX
        )));
    }

    let mut deduped: Vec<Uuid> = ids;
    deduped.sort();
    deduped.dedup();

    let states = fetch_follow_states(&state.db, user.id, &deduped).await?;
    let states = states
        .into_iter()
        .map(|(id, s)| {
            (
                id,
                BulkStateEntry {
                    you_follow: s.you_follow,
                    follows_you: s.follows_you,
                },
            )
        })
        .collect();
    Ok(Json(BulkStateResponse { states }))
}

// ==================== helpers ====================

/// Same 404 shape for missing rows and soft-deleted users so existence
/// isn't disclosed for inactive accounts. When per-user visibility lands
/// later, this is the single seam that needs to grow.
async fn ensure_target_visible(db: &DatabaseConnection, target_id: Uuid) -> AppResult<()> {
    let row = User::find_by_id(target_id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;
    if !row.active {
        return Err(AppError::AuthError("User not found".to_string()));
    }
    Ok(())
}

async fn load_pair_state(
    db: &DatabaseConnection,
    viewer_id: Uuid,
    target_id: Uuid,
) -> AppResult<FollowStateResponse> {
    let map = fetch_follow_states(db, viewer_id, &[target_id]).await?;
    let s = map.get(&target_id).copied().unwrap_or_default();
    Ok(FollowStateResponse {
        you_follow: s.you_follow,
        follows_you: s.follows_you,
    })
}

async fn fetch_user_summaries(
    db: &DatabaseConnection,
    ids: &[Uuid],
) -> Result<Vec<UserSummary>, sea_orm::DbErr> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = User::find()
        .filter(user::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    let by_id: HashMap<Uuid, user::Model> = rows.into_iter().map(|m| (m.id, m)).collect();
    // Preserve input order so cursor pagination reads in the server-
    // determined sort, not the random map iteration order.
    Ok(ids
        .iter()
        .filter_map(|id| {
            by_id.get(id).map(|m| UserSummary {
                user_id: m.id,
                handle: m.handle.clone(),
                display_name: m.display_name.clone(),
                avatar_url: avatar_url_for(m),
                role: m.role.clone(),
            })
        })
        .collect())
}

fn parse_cursor(cursor: &str) -> Option<(chrono::DateTime<chrono::FixedOffset>, Uuid)> {
    let (ts, id) = cursor.rsplit_once('_')?;
    let parsed_ts = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let parsed_id = Uuid::parse_str(id).ok()?;
    Some((parsed_ts, parsed_id))
}

fn format_cursor(created_at: chrono::DateTime<chrono::FixedOffset>, id: Uuid) -> String {
    format!("{}_{}", created_at.with_timezone(&Utc).to_rfc3339(), id)
}
