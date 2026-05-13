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
//! Comment reaction endpoints. Mirrors the post-reaction surface but
//! keyed by `comment_id` instead of `post_id`. The visibility check still
//! happens against the parent post so an under-tier caller cannot react
//! to a comment on a post they should not be able to read.

use crate::admin::UserAuth;
use crate::engagement::EngagementState;
use crate::entities::reaction;
use crate::entities::{comment, comment_reaction, post, Comment, CommentReaction, Post};
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

pub fn comment_reaction_routes() -> Router<EngagementState> {
    Router::new()
        .route(
            "/api/posts/{post_id}/comments/{comment_id}/reactions",
            post(create_comment_reaction),
        )
        .route(
            "/api/posts/{post_id}/comments/{comment_id}/reactions/{kind}",
            delete(delete_comment_reaction),
        )
}

#[derive(Deserialize)]
pub struct CreateCommentReactionRequest {
    pub kind: String,
}

#[derive(Serialize)]
pub struct CommentReactionStateResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

async fn create_comment_reaction(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<CreateCommentReactionRequest>,
) -> AppResult<Json<CommentReactionStateResponse>> {
    if !reaction::is_valid_kind(&req.kind) {
        return Err(AppError::ValidationError(format!(
            "Unsupported reaction kind '{}'",
            req.kind
        )));
    }

    let parent = load_visible_comment(&state.db, post_id, comment_id, &user).await?;

    let am = comment_reaction::ActiveModel {
        id: Set(Uuid::new_v4()),
        comment_id: Set(parent.id),
        user_id: Set(user.id),
        kind: Set(req.kind.clone()),
        created_at: Set(chrono::Utc::now().into()),
    };

    let insert_result = CommentReaction::insert(am)
        .on_conflict(
            OnConflict::columns([
                comment_reaction::Column::CommentId,
                comment_reaction::Column::UserId,
                comment_reaction::Column::Kind,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(&state.db)
        .await;

    match insert_result {
        Ok(_) => {
            crate::metrics::REACTIONS_TOTAL
                .with_label_values(&["comment_add"])
                .inc();
        }
        Err(DbErr::RecordNotInserted) => {}
        Err(e) => return Err(e.into()),
    }

    Ok(Json(
        comment_reaction_state(&state.db, parent.id, user.id).await?,
    ))
}

async fn delete_comment_reaction(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, comment_id, kind)): Path<(Uuid, Uuid, String)>,
) -> AppResult<Json<CommentReactionStateResponse>> {
    if !reaction::is_valid_kind(&kind) {
        return Err(AppError::ValidationError(format!(
            "Unsupported reaction kind '{}'",
            kind
        )));
    }

    let parent = load_visible_comment(&state.db, post_id, comment_id, &user).await?;

    let result = CommentReaction::delete_many()
        .filter(comment_reaction::Column::CommentId.eq(parent.id))
        .filter(comment_reaction::Column::UserId.eq(user.id))
        .filter(comment_reaction::Column::Kind.eq(kind))
        .exec(&state.db)
        .await?;
    if result.rows_affected > 0 {
        crate::metrics::REACTIONS_TOTAL
            .with_label_values(&["comment_remove"])
            .inc();
    }

    Ok(Json(
        comment_reaction_state(&state.db, parent.id, user.id).await?,
    ))
}

/// Load the comment after confirming it belongs to the path's post and
/// the caller can read the parent post. Returns the comment row when
/// authorized; same `Comment not found` error for any failure mode so
/// callers cannot probe for ownership or visibility.
async fn load_visible_comment(
    db: &DatabaseConnection,
    post_id: Uuid,
    comment_id: Uuid,
    user: &UserAuth,
) -> AppResult<comment::Model> {
    let row = Comment::find_by_id(comment_id)
        .filter(comment::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    if row.post_id != post_id {
        return Err(AppError::NotFound("Comment not found".to_string()));
    }

    let parent = Post::find_by_id(row.post_id)
        .filter(post::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;
    let parent_cat = if let Some(cid) = parent.category_id {
        crate::entities::Category::find_by_id(cid).one(db).await?
    } else {
        None
    };

    let ctx = crate::visibility::ViewerCtx::from_user_auth_async(db, user)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.can_view_post(&parent, parent_cat.as_ref()) {
        return Err(AppError::NotFound("Comment not found".to_string()));
    }
    Ok(row)
}

/// Aggregate counts and viewer reactions for a single comment. Used as
/// the response body after an add or remove so clients update in one
/// round trip.
pub(crate) async fn comment_reaction_state(
    db: &DatabaseConnection,
    comment_id: Uuid,
    viewer_id: Uuid,
) -> Result<CommentReactionStateResponse, DbErr> {
    let mut map = fetch_comment_engagement(db, &[comment_id], Some(viewer_id)).await?;
    let entry = map.remove(&comment_id).unwrap_or_default();
    Ok(CommentReactionStateResponse {
        reaction_counts: entry.reaction_counts,
        viewer_reaction_kinds: entry.viewer_reaction_kinds,
    })
}

/// Per-comment engagement summary. Mirrors `PostEngagement` for posts but
/// only carries the reaction-side fields — comments do not have nested
/// comments, so there is no count to track here.
#[derive(Default, Debug, Clone)]
pub struct CommentEngagement {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

/// Batched lookup of reaction counts and viewer reactions for a list of
/// comment IDs. Used by the comments-list handler to attach engagement
/// fields to each `CommentResponse` without N+1.
pub async fn fetch_comment_engagement(
    db: &DatabaseConnection,
    comment_ids: &[Uuid],
    viewer_id: Option<Uuid>,
) -> Result<HashMap<Uuid, CommentEngagement>, DbErr> {
    if comment_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut out: HashMap<Uuid, CommentEngagement> = HashMap::new();

    use sea_orm::{FromQueryResult, QuerySelect};

    #[derive(FromQueryResult)]
    struct CountRow {
        comment_id: Uuid,
        kind: String,
        count: i64,
    }
    #[derive(FromQueryResult)]
    struct ViewerRow {
        comment_id: Uuid,
        kind: String,
    }

    let counts: Vec<CountRow> = CommentReaction::find()
        .select_only()
        .column(comment_reaction::Column::CommentId)
        .column(comment_reaction::Column::Kind)
        .column_as(comment_reaction::Column::Id.count(), "count")
        .filter(comment_reaction::Column::CommentId.is_in(comment_ids.to_vec()))
        .group_by(comment_reaction::Column::CommentId)
        .group_by(comment_reaction::Column::Kind)
        .into_model::<CountRow>()
        .all(db)
        .await?;

    for row in counts {
        out.entry(row.comment_id)
            .or_default()
            .reaction_counts
            .insert(row.kind, row.count);
    }

    if let Some(viewer) = viewer_id {
        let mine: Vec<ViewerRow> = CommentReaction::find()
            .select_only()
            .column(comment_reaction::Column::CommentId)
            .column(comment_reaction::Column::Kind)
            .filter(comment_reaction::Column::CommentId.is_in(comment_ids.to_vec()))
            .filter(comment_reaction::Column::UserId.eq(viewer))
            .into_model::<ViewerRow>()
            .all(db)
            .await?;
        for row in mine {
            out.entry(row.comment_id)
                .or_default()
                .viewer_reaction_kinds
                .push(row.kind);
        }
    }

    Ok(out)
}
