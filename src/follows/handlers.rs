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

use crate::admin::UserAuth;
use crate::errors::{AppError, AppResult};
use crate::follows::queries;
use crate::follows::types::{
    BulkStateEntry, BulkStateQuery, BulkStateResponse, FollowStateResponse, FollowsListResponse,
    ListQuery,
};
use crate::follows::FollowsState;
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

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

    let inserted = queries::upsert_follow(&state.db, user.id, target_id, Utc::now().into()).await?;
    if inserted {
        crate::metrics::FOLLOWS_TOTAL
            .with_label_values(&["add"])
            .inc();
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

    let removed = queries::delete_follow(&state.db, user.id, target_id).await?;
    if removed > 0 {
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
/// pages.
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
    let cursor = query.cursor.as_deref().and_then(parse_cursor);

    let rows = queries::list_followers_page(&state.db, target_id, cursor, limit + 1).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<&_> = rows.iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| format_cursor(r.created_at, r.follower_id))
    } else {
        None
    };
    let kept_ids: Vec<Uuid> = page.iter().map(|r| r.follower_id).collect();
    let users = queries::fetch_user_summaries(&state.db, &kept_ids).await?;
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
    let cursor = query.cursor.as_deref().and_then(parse_cursor);

    let rows = queries::list_following_page(&state.db, target_id, cursor, limit + 1).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<&_> = rows.iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| format_cursor(r.created_at, r.followed_id))
    } else {
        None
    };
    let kept_ids: Vec<Uuid> = page.iter().map(|r| r.followed_id).collect();
    let users = queries::fetch_user_summaries(&state.db, &kept_ids).await?;
    Ok(Json(FollowsListResponse { users, next_cursor }))
}

async fn get_follow_state(
    State(state): State<FollowsState>,
    Extension(user): Extension<UserAuth>,
    Query(query): Query<BulkStateQuery>,
) -> AppResult<Json<BulkStateResponse>> {
    let segments: Vec<&str> = query
        .user_ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if segments.len() > BULK_STATE_MAX {
        return Err(AppError::ValidationError(format!(
            "At most {} user_ids per request",
            BULK_STATE_MAX
        )));
    }
    let mut ids: Vec<Uuid> = Vec::with_capacity(segments.len());
    for s in segments {
        let parsed = Uuid::parse_str(s).map_err(|_| {
            AppError::ValidationError("user_ids must be comma-separated UUIDs".to_string())
        })?;
        ids.push(parsed);
    }

    let mut deduped = ids;
    deduped.sort();
    deduped.dedup();

    let states = queries::fetch_follow_states(&state.db, user.id, &deduped).await?;
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

/// Same 404 shape for missing rows and soft-deleted users so existence
/// isn't disclosed for inactive accounts. When per-user visibility lands
/// later, this is the single seam that needs to grow.
async fn ensure_target_visible(db: &DatabaseConnection, target_id: Uuid) -> AppResult<()> {
    let row = queries::find_active_user(db, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    if !row.active {
        return Err(AppError::NotFound("User not found".to_string()));
    }
    Ok(())
}

async fn load_pair_state(
    db: &DatabaseConnection,
    viewer_id: Uuid,
    target_id: Uuid,
) -> AppResult<FollowStateResponse> {
    let map = queries::fetch_follow_states(db, viewer_id, &[target_id]).await?;
    let s = map.get(&target_id).copied().unwrap_or_default();
    Ok(FollowStateResponse {
        you_follow: s.you_follow,
        follows_you: s.follows_you,
    })
}

fn parse_cursor(cursor: &str) -> Option<(DateTime<FixedOffset>, Uuid)> {
    let (ts, id) = cursor.rsplit_once('_')?;
    let parsed_ts = DateTime::parse_from_rfc3339(ts).ok()?;
    let parsed_id = Uuid::parse_str(id).ok()?;
    Some((parsed_ts, parsed_id))
}

fn format_cursor(created_at: DateTime<FixedOffset>, id: Uuid) -> String {
    format!("{}_{}", created_at.with_timezone(&Utc).to_rfc3339(), id)
}
