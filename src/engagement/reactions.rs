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
//! Reaction endpoints. Both POST and DELETE require authentication; the
//! visibility-tier check against the parent post happens inline so an
//! under-tier caller gets the same 404 they would for a missing post.

use crate::admin::UserAuth;
use crate::engagement::aggregate::fetch_engagement_for_posts;
use crate::engagement::EngagementState;
use crate::entities::{post, reaction, Reaction};
use crate::errors::{AppError, AppResult};
use axum::{
    extract::{Path, State},
    response::Json,
    routing::{delete, post},
    Extension, Router,
};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub fn reaction_write_routes() -> Router<EngagementState> {
    Router::new()
        .route("/api/posts/{id}/reactions", post(create_reaction))
        .route(
            "/api/posts/{id}/reactions/{kind}",
            delete(delete_reaction),
        )
}

#[derive(Deserialize)]
pub struct CreateReactionRequest {
    pub kind: String,
}

#[derive(Serialize)]
pub struct ReactionStateResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

/// `POST /api/posts/{id}/reactions`. Idempotent: a second call with the
/// same kind is a no-op. Reactions are uniquely keyed by
/// `(post_id, user_id, kind)`, enforced by the DB index.
async fn create_reaction(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path(post_id): Path<Uuid>,
    Json(req): Json<CreateReactionRequest>,
) -> AppResult<Json<ReactionStateResponse>> {
    if !reaction::is_valid_kind(&req.kind) {
        return Err(AppError::ValidationError(format!(
            "Unsupported reaction kind '{}'",
            req.kind
        )));
    }

    let parent = ensure_visible_post(&state.db, post_id, &user).await?;

    // `Insert::on_conflict(...).exec()` bypasses `ActiveModelBehavior::
    // before_save`, so we set `created_at` explicitly here. ActiveModel's
    // `insert()` would call before_save for us, but we need ON CONFLICT
    // semantics for idempotency.
    let am = reaction::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_id: Set(parent.id),
        user_id: Set(user.id),
        kind: Set(req.kind.clone()),
        created_at: Set(chrono::Utc::now().into()),
    };

    // Insert-or-ignore: a duplicate is treated as success so the client
    // never has to track local state to avoid double-add.
    match Reaction::insert(am)
        .on_conflict(
            OnConflict::columns([
                reaction::Column::PostId,
                reaction::Column::UserId,
                reaction::Column::Kind,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(&state.db)
        .await
    {
        Ok(_) => {
            crate::metrics::REACTIONS_TOTAL
                .with_label_values(&["add"])
                .inc();
        }
        Err(DbErr::RecordNotInserted) => {}
        Err(e) => return Err(e.into()),
    }

    Ok(Json(reaction_state(&state, parent.id, user.id).await?))
}

/// `DELETE /api/posts/{id}/reactions/{kind}`. Idempotent: deleting a
/// non-existent reaction returns the current state without error.
async fn delete_reaction(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, kind)): Path<(Uuid, String)>,
) -> AppResult<Json<ReactionStateResponse>> {
    if !reaction::is_valid_kind(&kind) {
        return Err(AppError::ValidationError(format!(
            "Unsupported reaction kind '{}'",
            kind
        )));
    }

    let parent = ensure_visible_post(&state.db, post_id, &user).await?;

    let result = Reaction::delete_many()
        .filter(reaction::Column::PostId.eq(parent.id))
        .filter(reaction::Column::UserId.eq(user.id))
        .filter(reaction::Column::Kind.eq(kind))
        .exec(&state.db)
        .await?;
    if result.rows_affected > 0 {
        crate::metrics::REACTIONS_TOTAL
            .with_label_values(&["remove"])
            .inc();
    }

    Ok(Json(reaction_state(&state, parent.id, user.id).await?))
}

/// Fetch the parent post and verify the caller's tier is permitted to read
/// it. Returns the same `Post not found` error for both missing rows and
/// under-tier calls so existence isn't disclosed.
async fn ensure_visible_post(
    db: &DatabaseConnection,
    post_id: Uuid,
    user: &UserAuth,
) -> AppResult<post::Model> {
    crate::visibility::ensure_visible_post_for_user(db, post_id, user).await
}

/// Re-aggregate counts and viewer reactions for a single post; used as the
/// response body after an add or remove so clients update in one round trip.
async fn reaction_state(
    state: &EngagementState,
    post_id: Uuid,
    viewer_id: Uuid,
) -> Result<ReactionStateResponse, DbErr> {
    let mut map = fetch_engagement_for_posts(&state.db, &[post_id], Some(viewer_id)).await?;
    let entry = map.remove(&post_id).unwrap_or_default();
    Ok(ReactionStateResponse {
        reaction_counts: entry.reaction_counts,
        viewer_reaction_kinds: entry.viewer_reaction_kinds,
    })
}
