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
//! Per-media-item reaction endpoints. Keyed by `media_id` under a post.
//! The visibility check happens against the parent post: seeing the post
//! implies seeing its media, so the gate is post-level.

use crate::admin::UserAuth;
use crate::engagement::EngagementState;
use crate::entities::{
    post, post_media, post_media_reaction, reaction, PostMedia, PostMediaReaction,
};
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::posts::FeedTier;
use axum::{
    extract::{Path, State},
    response::Json,
    routing::{delete, get, post},
    Extension, Router,
};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub fn media_reaction_routes() -> Router<EngagementState> {
    Router::new()
        .route(
            "/api/posts/{post_id}/media/{media_id}/reactions",
            post(create_media_reaction),
        )
        .route(
            "/api/posts/{post_id}/media/{media_id}/reactions/{kind}",
            delete(delete_media_reaction),
        )
}

pub fn media_engagement_read_routes() -> Router<EngagementState> {
    Router::new().route(
        "/api/posts/{post_id}/media/{media_id}/engagement",
        get(get_media_engagement),
    )
}

#[derive(Deserialize)]
pub struct CreateMediaReactionRequest {
    pub kind: String,
}

#[derive(Serialize)]
pub struct MediaReactionStateResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
}

#[derive(Serialize)]
pub struct MediaEngagementResponse {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
    pub comment_count: i64,
}

/// `GET /api/posts/{post_id}/media/{media_id}/engagement`. Lazy-loaded by
/// the lightbox on open so the engagement panel can render reaction
/// counts and comment count without that data riding the post / album
/// payload (which would balloon proportional to media count).
async fn get_media_engagement(
    State(state): State<EngagementState>,
    auth_session: UserAuthSession,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<MediaEngagementResponse>> {
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

    let viewer_id = viewer.as_ref().map(|u| u.id);
    let mut map =
        crate::engagement::aggregate::fetch_engagement_for_media(&state.db, &[media.id], viewer_id)
            .await?;
    let entry = map.remove(&media.id).unwrap_or_default();
    Ok(Json(MediaEngagementResponse {
        reaction_counts: entry.reaction_counts,
        viewer_reaction_kinds: entry.viewer_reaction_kinds,
        comment_count: entry.comment_count,
    }))
}

async fn create_media_reaction(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<CreateMediaReactionRequest>,
) -> AppResult<Json<MediaReactionStateResponse>> {
    if !reaction::is_valid_kind(&req.kind) {
        return Err(AppError::ValidationError(format!(
            "Unsupported reaction kind '{}'",
            req.kind
        )));
    }

    let media = load_visible_media(&state.db, post_id, media_id, &user).await?;

    let am = post_media_reaction::ActiveModel {
        id: Set(Uuid::new_v4()),
        post_media_id: Set(media.id),
        user_id: Set(user.id),
        kind: Set(req.kind.clone()),
        created_at: Set(chrono::Utc::now().into()),
    };

    let insert_result = PostMediaReaction::insert(am)
        .on_conflict(
            OnConflict::columns([
                post_media_reaction::Column::PostMediaId,
                post_media_reaction::Column::UserId,
                post_media_reaction::Column::Kind,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(&state.db)
        .await;

    match insert_result {
        Ok(_) => {
            crate::metrics::REACTIONS_TOTAL
                .with_label_values(&["media_add"])
                .inc();
        }
        Err(DbErr::RecordNotInserted) => {}
        Err(e) => return Err(e.into()),
    }

    Ok(Json(
        media_reaction_state(&state.db, media.id, user.id).await?,
    ))
}

async fn delete_media_reaction(
    State(state): State<EngagementState>,
    Extension(user): Extension<UserAuth>,
    Path((post_id, media_id, kind)): Path<(Uuid, Uuid, String)>,
) -> AppResult<Json<MediaReactionStateResponse>> {
    if !reaction::is_valid_kind(&kind) {
        return Err(AppError::ValidationError(format!(
            "Unsupported reaction kind '{}'",
            kind
        )));
    }

    let media = load_visible_media(&state.db, post_id, media_id, &user).await?;

    let result = PostMediaReaction::delete_many()
        .filter(post_media_reaction::Column::PostMediaId.eq(media.id))
        .filter(post_media_reaction::Column::UserId.eq(user.id))
        .filter(post_media_reaction::Column::Kind.eq(kind))
        .exec(&state.db)
        .await?;
    if result.rows_affected > 0 {
        crate::metrics::REACTIONS_TOTAL
            .with_label_values(&["media_remove"])
            .inc();
    }

    Ok(Json(
        media_reaction_state(&state.db, media.id, user.id).await?,
    ))
}

/// Load the media row keyed by `(post_id, media_id)` after confirming
/// the caller can read the parent post. Returns a uniform `Media not
/// found` for missing rows, mismatched ids, and under-tier callers so
/// the endpoint can't be probed for existence or visibility.
pub(crate) async fn load_visible_media(
    db: &DatabaseConnection,
    post_id: Uuid,
    media_id: Uuid,
    user: &UserAuth,
) -> AppResult<post_media::Model> {
    let media = PostMedia::find_by_id(media_id)
        .filter(post_media::Column::PostId.eq(post_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    crate::visibility::load_visible_post(db, post_id, user)
        .await
        .map_err(|_| AppError::NotFound("Media not found".to_string()))?;

    Ok(media)
}

/// Look up media without requiring a `UserAuth`. Used by the read path
/// in `media_comments` so anonymous callers can fetch comments on a
/// publicly-visible media item. The visibility gate is applied at the
/// caller using `ViewerCtx::can_view_post`; this helper only asserts the
/// `(post_id, media_id)` pair exists and the parent post isn't soft-
/// deleted.
pub(crate) async fn lookup_media(
    db: &DatabaseConnection,
    post_id: Uuid,
    media_id: Uuid,
) -> AppResult<(post_media::Model, post::Model)> {
    let media = PostMedia::find_by_id(media_id)
        .filter(post_media::Column::PostId.eq(post_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;
    let parent = crate::entities::Post::find_by_id(post_id)
        .filter(post::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;
    Ok((media, parent))
}

async fn media_reaction_state(
    db: &DatabaseConnection,
    media_id: Uuid,
    viewer_id: Uuid,
) -> Result<MediaReactionStateResponse, DbErr> {
    let mut map =
        crate::engagement::aggregate::fetch_engagement_for_media(db, &[media_id], Some(viewer_id))
            .await?;
    let entry = map.remove(&media_id).unwrap_or_default();
    Ok(MediaReactionStateResponse {
        reaction_counts: entry.reaction_counts,
        viewer_reaction_kinds: entry.viewer_reaction_kinds,
    })
}
