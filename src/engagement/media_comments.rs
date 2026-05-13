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
//! Per-media-item comment endpoints. Mirror of `comments.rs` keyed by a
//! `(post_id, media_id)` pair. Visibility is gated against the parent
//! post — seeing the post implies seeing its media's conversation.

use crate::admin::UserAuth;
use crate::engagement::media_reactions::{load_visible_media, lookup_media};
use crate::engagement::EngagementState;
use crate::entities::{post_media_comment, user, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::posts::{markdown, FeedTier};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

const COMMENT_MAX_LEN: usize = 4000;

pub fn media_comment_write_routes() -> Router<EngagementState> {
    Router::new()
        .route(
            "/api/posts/{post_id}/media/{media_id}/comments",
            post(create_media_comment),
        )
        .route(
            "/api/posts/{post_id}/media/{media_id}/comments/{comment_id}",
            axum::routing::patch(edit_media_comment).delete(delete_media_comment),
        )
}

pub fn media_comment_read_routes() -> Router<EngagementState> {
    Router::new().route(
        "/api/posts/{post_id}/media/{media_id}/comments",
        get(list_media_comments),
    )
}

#[derive(Deserialize)]
pub struct CreateMediaCommentRequest {
    pub body: String,
}

#[derive(Deserialize)]
pub struct EditMediaCommentRequest {
    pub body: String,
}

#[derive(Serialize)]
pub struct MediaCommentResponse {
    pub id: Uuid,
    pub media_id: Uuid,
    pub user_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub body: String,
    pub body_html: String,
    pub created_at: String,
    pub updated_at: String,
    pub edited_at: Option<String>,
}

#[derive(Serialize)]
pub struct MediaCommentListResponse {
    pub comments: Vec<MediaCommentResponse>,
}

fn build_media_comment_response(
    row: post_media_comment::Model,
    author: Option<&user::Model>,
) -> MediaCommentResponse {
    let body_html = markdown::render_to_html(&row.body);
    let edited_at = row.edited_at.map(|t| t.with_timezone(&Utc).to_rfc3339());
    MediaCommentResponse {
        id: row.id,
        media_id: row.post_media_id,
        user_id: row.user_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        body: row.body,
        body_html,
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        edited_at,
    }
}

async fn create_media_comment(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<CreateMediaCommentRequest>,
) -> AppResult<(StatusCode, Json<MediaCommentResponse>)> {
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

    let media = load_visible_media(&state.db, post_id, media_id, &user).await?;

    let row = post_media_comment::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_media_id: Set(media.id),
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
        .with_label_values(&["media_create"])
        .inc();
    Ok((
        StatusCode::CREATED,
        Json(build_media_comment_response(row, author.as_ref())),
    ))
}

async fn list_media_comments(
    State(state): State<EngagementState>,
    auth_session: UserAuthSession,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<MediaCommentListResponse>> {
    let viewer = auth_session.user().await;
    let tier = FeedTier::from_role(viewer.as_ref().map(|u| u.role.as_str()));

    if matches!(tier, FeedTier::Anonymous) {
        let enabled = state
            .settings
            .get_public_feed_enabled()
            .await
            .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
        if !enabled {
            return Err(AppError::NotFound("Media not found".to_string()));
        }
    }

    let (media, parent) = lookup_media(&state.db, post_id, media_id).await?;
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
    if !ctx.can_view_post(&parent, parent_cat.as_ref()) {
        return Err(AppError::NotFound("Media not found".to_string()));
    }

    let rows = crate::entities::PostMediaComment::find()
        .filter(post_media_comment::Column::PostMediaId.eq(media.id))
        .filter(post_media_comment::Column::DeletedAt.is_null())
        .order_by_asc(post_media_comment::Column::CreatedAt)
        .order_by_asc(post_media_comment::Column::Id)
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

    let comments = rows
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.user_id);
            build_media_comment_response(row, author)
        })
        .collect();

    Ok(Json(MediaCommentListResponse { comments }))
}

async fn edit_media_comment(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, media_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<EditMediaCommentRequest>,
) -> AppResult<Json<MediaCommentResponse>> {
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

    let row = crate::entities::PostMediaComment::find_by_id(comment_id)
        .filter(post_media_comment::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    if row.post_media_id != media_id {
        return Err(AppError::NotFound("Comment not found".to_string()));
    }

    // Confirm the (post_id, media_id) URL pair matches the row's actual
    // parent post so the URL prefix can't be spoofed to a different post.
    let media = crate::entities::PostMedia::find_by_id(row.post_media_id)
        .filter(crate::entities::post_media::Column::PostId.eq(post_id))
        .one(&state.db)
        .await?;
    if media.is_none() {
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
    let mut active: post_media_comment::ActiveModel = row.into();
    active.body = Set(body);
    active.edited_at = Set(Some(Utc::now().into()));
    let updated = active.update(&state.db).await?;

    let author = User::find_by_id(author_id).one(&state.db).await?;
    crate::metrics::COMMENTS_TOTAL
        .with_label_values(&["media_edit"])
        .inc();
    Ok(Json(build_media_comment_response(updated, author.as_ref())))
}

async fn delete_media_comment(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, media_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let row = crate::entities::PostMediaComment::find_by_id(comment_id)
        .filter(post_media_comment::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Comment not found".to_string()))?;

    if row.post_media_id != media_id {
        return Err(AppError::NotFound("Comment not found".to_string()));
    }
    let media = crate::entities::PostMedia::find_by_id(row.post_media_id)
        .filter(crate::entities::post_media::Column::PostId.eq(post_id))
        .one(&state.db)
        .await?;
    if media.is_none() {
        return Err(AppError::NotFound("Comment not found".to_string()));
    }

    let is_author = row.user_id == user.id;
    let is_admin = user.role == user::ROLE_ADMINISTRATOR;
    if !is_author && !is_admin {
        return Err(AppError::Forbidden(
            "Only the comment author or an administrator can delete this comment".to_string(),
        ));
    }

    let mut active: post_media_comment::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(&state.db).await?;
    crate::metrics::COMMENTS_TOTAL
        .with_label_values(&["media_soft_delete"])
        .inc();
    Ok(StatusCode::NO_CONTENT)
}
