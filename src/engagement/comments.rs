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
//! Comment endpoints.
//!
//! - `POST   /api/posts/{id}/comments`         authenticated
//! - `GET    /api/posts/{id}/comments`         public; visibility-filtered
//! - `PATCH  /api/posts/{id}/comments/{cid}`   author or administrator
//! - `DELETE /api/posts/{id}/comments/{cid}`   author or administrator
//!
//! Bodies are markdown, rendered server-side by the same `pulldown-cmark`
//! plus `ammonia` pipeline used for post bodies. Soft-deleted comments
//! (`deleted_at IS NOT NULL`) are filtered out of public reads; they survive
//! in the DB so admin moderation can audit. Edits set `edited_at` to a
//! timestamp distinct from `updated_at` (which moves on any DB write) so
//! the UI can show an "(edited)" indicator only for user-driven body
//! changes.

use crate::admin::UserAuth;
use crate::engagement::types::{
    CommentListResponse, CommentResponse, CreateCommentRequest, EditCommentRequest,
};
use crate::engagement::{
    comment_reactions::fetch_comment_engagement, CommentEngagement, EngagementState,
};
use crate::entities::{comment, post, user, Comment, Post, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::posts::{markdown, FeedTier};
use crate::visibility::load_visible_post;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use std::collections::HashMap;
use uuid::Uuid;

/// Body length cap. Comments are short by intent; longer thoughts deserve
/// their own post. Matches the posts-body soft cap so storage is bounded.
const COMMENT_MAX_LEN: usize = 4000;

pub fn comment_write_routes() -> Router<EngagementState> {
    Router::new()
        .route("/api/posts/{id}/comments", post(create_comment))
        .route(
            "/api/posts/{id}/comments/{comment_id}",
            axum::routing::patch(edit_comment).delete(delete_comment),
        )
}

pub fn comment_read_routes() -> Router<EngagementState> {
    Router::new().route("/api/posts/{id}/comments", get(list_comments))
}

pub(crate) fn build_comment_response(
    row: comment::Model,
    author: Option<&user::Model>,
) -> CommentResponse {
    build_comment_response_with_engagement(row, author, CommentEngagement::default())
}

pub(crate) fn build_comment_response_with_engagement(
    row: comment::Model,
    author: Option<&user::Model>,
    engagement: CommentEngagement,
) -> CommentResponse {
    let body_html = markdown::render_to_html(&row.body);
    let edited_at = row.edited_at.map(|t| t.with_timezone(&Utc).to_rfc3339());
    CommentResponse {
        id: row.id,
        post_id: row.post_id,
        user_id: row.user_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        body: row.body,
        body_html,
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        edited_at,
        reaction_counts: engagement.reaction_counts,
        viewer_reaction_kinds: engagement.viewer_reaction_kinds,
    }
}

/// `POST /api/posts/{id}/comments`. Authenticated commenter, poster, or
/// administrator. Caller must be allowed to read the parent post.
async fn create_comment(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path(post_id): Path<Uuid>,
    Json(req): Json<CreateCommentRequest>,
) -> AppResult<(StatusCode, Json<CommentResponse>)> {
    let body = req.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::ValidationError(
            "Comment body cannot be empty".to_string(),
        ));
    }
    if body.chars().count() > COMMENT_MAX_LEN {
        return Err(AppError::ValidationError(format!(
            "Comment exceeds {}-character limit",
            COMMENT_MAX_LEN
        )));
    }

    let parent = load_visible_post(&state.db, post_id, &user).await?;

    let row = comment::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_id: Set(parent.id),
        user_id: Set(user.id),
        body: Set(body),
        deleted_at: Set(None),
        edited_at: Set(None),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    let author = User::find_by_id(row.user_id).one(&state.db).await?;
    crate::metrics::COMMENTS_TOTAL
        .with_label_values(&["create"])
        .inc();
    Ok((
        StatusCode::CREATED,
        Json(build_comment_response(row, author.as_ref())),
    ))
}

/// `GET /api/posts/{id}/comments`. Public; the caller's tier filters the
/// parent post's visibility. Returns live comments in chronological order.
async fn list_comments(
    State(state): State<EngagementState>,
    auth_session: UserAuthSession,
    Path(post_id): Path<Uuid>,
) -> AppResult<Json<CommentListResponse>> {
    let viewer = auth_session.user().await;
    let tier = FeedTier::from_role(viewer.as_ref().map(|u| u.role.as_str()));

    // Anonymous reads are blocked when the public feed is muted; authed
    // callers are unaffected. Fail closed: a settings read failure
    // surfaces as a 500 rather than silently exposing comments.
    if matches!(tier, FeedTier::Anonymous) {
        let enabled = state
            .settings
            .get_public_feed_enabled()
            .await
            .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
        if !enabled {
            return Err(AppError::NotFound("Post not found".to_string()));
        }
    }

    let parent = Post::find_by_id(post_id)
        .filter(post::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Post not found".to_string()))?;
    let parent_cat = if let Some(cid) = parent.category_id {
        crate::entities::Category::find_by_id(cid)
            .one(&state.db)
            .await?
    } else {
        None
    };
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(parent.author_id, &parent.visibility, parent_cat.as_ref()) {
        return Err(AppError::NotFound("Post not found".to_string()));
    }

    let rows = Comment::find()
        .filter(comment::Column::PostId.eq(parent.id))
        .filter(comment::Column::DeletedAt.is_null())
        .order_by_asc(comment::Column::CreatedAt)
        .order_by_asc(comment::Column::Id)
        .all(&state.db)
        .await?;

    let author_ids: Vec<Uuid> = rows.iter().map(|r| r.user_id).collect();
    let authors = if author_ids.is_empty() {
        vec![]
    } else {
        User::find()
            .filter(user::Column::Id.is_in(author_ids))
            .all(&state.db)
            .await?
    };
    let authors_by_id: HashMap<Uuid, user::Model> =
        authors.into_iter().map(|a| (a.id, a)).collect();

    // Batch-fetch reaction counts and viewer reactions for every comment
    // in this thread. Anonymous viewers get counts only.
    let comment_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut engagement_by_comment =
        fetch_comment_engagement(&state.db, &comment_ids, viewer.as_ref().map(|u| u.id)).await?;

    let comments = rows
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.user_id);
            let eng = engagement_by_comment.remove(&row.id).unwrap_or_default();
            build_comment_response_with_engagement(row, author, eng)
        })
        .collect();

    Ok(Json(CommentListResponse { comments }))
}

/// `PATCH /api/posts/{id}/comments/{comment_id}`. Edit the body of an
/// existing comment. Allowed for the original author and administrators
/// (admin edit is a moderation affordance  typically used to redact
/// rather than rewrite, but the wider permission keeps the surface
/// uniform with delete). Sets `edited_at` to the current time.
async fn edit_comment(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<EditCommentRequest>,
) -> AppResult<Json<CommentResponse>> {
    let body = req.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::ValidationError(
            "Comment body cannot be empty".to_string(),
        ));
    }
    if body.chars().count() > COMMENT_MAX_LEN {
        return Err(AppError::ValidationError(format!(
            "Comment exceeds {}-character limit",
            COMMENT_MAX_LEN
        )));
    }

    let row = Comment::find_by_id(comment_id)
        .filter(comment::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    if row.post_id != post_id {
        // Mirror the delete-side response: don't leak existence of a
        // comment whose post_id doesn't match the route.
        return Err(AppError::NotFound("Comment not found".to_string()));
    }

    let is_author = row.user_id == user.id;
    let is_admin = user.role == user::ROLE_ADMINISTRATOR;
    if !is_author && !is_admin {
        return Err(AppError::Forbidden(
            "Only the comment author or an administrator can edit this comment".to_string(),
        ));
    }

    let author_id = row.user_id;
    let mut active: comment::ActiveModel = row.into();
    active.body = Set(body);
    active.edited_at = Set(Some(Utc::now().into()));
    let updated = active.update(&state.db).await?;

    let author = User::find_by_id(author_id).one(&state.db).await?;
    crate::metrics::COMMENTS_TOTAL
        .with_label_values(&["edit"])
        .inc();
    Ok(Json(build_comment_response(updated, author.as_ref())))
}

/// `DELETE /api/posts/{id}/comments/{comment_id}`. Soft delete. Allowed
/// when the caller is the comment author or an administrator. Returns 204
/// on success; the row stays in the DB for audit but is hidden from public
/// reads.
async fn delete_comment(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let row = Comment::find_by_id(comment_id)
        .filter(comment::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    if row.post_id != post_id {
        // Comment exists but belongs to a different post; treat as not
        // found so callers can't probe for ownership.
        return Err(AppError::NotFound("Comment not found".to_string()));
    }

    let is_author = row.user_id == user.id;
    let is_admin = user.role == user::ROLE_ADMINISTRATOR;
    if !is_author && !is_admin {
        return Err(AppError::Forbidden(
            "Only the comment author or an administrator can delete this comment".to_string(),
        ));
    }

    let mut active: comment::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(&state.db).await?;
    crate::metrics::COMMENTS_TOTAL
        .with_label_values(&["soft_delete"])
        .inc();
    Ok(StatusCode::NO_CONTENT)
}
