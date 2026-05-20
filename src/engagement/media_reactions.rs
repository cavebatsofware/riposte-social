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
use crate::engagement::types::{
    CreateMediaReactionRequest, MediaEngagementResponse, MediaReactionStateResponse,
};
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
use sea_orm::{
    prelude::DateTimeWithTimeZone, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    FromQueryResult, JoinType, QueryFilter, QuerySelect, RelationTrait, Set,
};
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
    if !ctx.permits_read(parent.author_id, &parent.visibility, parent_cat.as_ref()) {
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

/// Single INNER JOIN that fetches the media row alongside the few
/// parent-post columns the visibility check actually reads. The post
/// `body` is intentionally not selected: it's large (potentially KB
/// per row) and irrelevant to every caller of this helper. Returns
/// `Media not found` for missing rows, mismatched ids, or a
/// soft-deleted parent so the endpoint surface stays uniform.
pub(crate) async fn lookup_media(
    db: &DatabaseConnection,
    post_id: Uuid,
    media_id: Uuid,
) -> AppResult<(post_media::Model, crate::visibility::PostVisibilityCols)> {
    #[derive(FromQueryResult)]
    struct Row {
        // post_media columns (full row).
        pm_id: Uuid,
        pm_post_id: Uuid,
        pm_s3_key: String,
        pm_mime_type: String,
        pm_width: Option<i32>,
        pm_height: Option<i32>,
        pm_ordinal: i32,
        pm_caption: Option<String>,
        pm_created_at: DateTimeWithTimeZone,
        // post columns (slim, id is implied by pm_post_id under the
        // JOIN condition, so we don't reselect it).
        p_author_id: Uuid,
        p_visibility: String,
        p_category_id: Option<Uuid>,
    }

    let row: Option<Row> = PostMedia::find_by_id(media_id)
        .select_only()
        .column_as(post_media::Column::Id, "pm_id")
        .column_as(post_media::Column::PostId, "pm_post_id")
        .column_as(post_media::Column::S3Key, "pm_s3_key")
        .column_as(post_media::Column::MimeType, "pm_mime_type")
        .column_as(post_media::Column::Width, "pm_width")
        .column_as(post_media::Column::Height, "pm_height")
        .column_as(post_media::Column::Ordinal, "pm_ordinal")
        .column_as(post_media::Column::Caption, "pm_caption")
        .column_as(post_media::Column::CreatedAt, "pm_created_at")
        .column_as(post::Column::AuthorId, "p_author_id")
        .column_as(post::Column::Visibility, "p_visibility")
        .column_as(post::Column::CategoryId, "p_category_id")
        .filter(post_media::Column::PostId.eq(post_id))
        .filter(post::Column::DeletedAt.is_null())
        .join(JoinType::InnerJoin, post_media::Relation::Post.def())
        .into_model::<Row>()
        .one(db)
        .await?;
    let row = row.ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let media = post_media::Model {
        id: row.pm_id,
        post_id: row.pm_post_id,
        s3_key: row.pm_s3_key,
        mime_type: row.pm_mime_type,
        width: row.pm_width,
        height: row.pm_height,
        ordinal: row.pm_ordinal,
        caption: row.pm_caption,
        created_at: row.pm_created_at,
        thumbnail_data: None,
        icon_data: None,
    };
    let parent = crate::visibility::PostVisibilityCols {
        id: row.pm_post_id,
        author_id: row.p_author_id,
        visibility: row.p_visibility,
        category_id: row.p_category_id,
    };
    Ok((media, parent))
}

/// Load the media + slim parent visibility columns in one query, then
/// apply the visibility gate. Returns a uniform `Media not found` for
/// missing rows, mismatched ids, soft-deleted parents, and under-tier
/// callers so the endpoint can't be probed for existence or visibility.
pub(crate) async fn load_visible_media(
    db: &DatabaseConnection,
    post_id: Uuid,
    media_id: Uuid,
    user: &UserAuth,
) -> AppResult<post_media::Model> {
    let (media, parent) = lookup_media(db, post_id, media_id).await?;

    let parent_cat = match parent.category_id {
        Some(cid) => crate::entities::Category::find_by_id(cid).one(db).await?,
        None => None,
    };
    let ctx = crate::visibility::ViewerCtx::from_user_auth_async(db, user)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(parent.author_id, &parent.visibility, parent_cat.as_ref()) {
        return Err(AppError::NotFound("Media not found".to_string()));
    }
    Ok(media)
}

/// Reaction-only state used as the response body for add / remove.
/// Specialized rather than calling the full `fetch_engagement_for_media`
/// because the reaction handlers don't read the comment count and have
/// no reason to pay for that aggregation on every interaction.
async fn media_reaction_state(
    db: &DatabaseConnection,
    media_id: Uuid,
    viewer_id: Uuid,
) -> Result<MediaReactionStateResponse, DbErr> {
    use sea_orm::{FromQueryResult, QuerySelect};

    #[derive(FromQueryResult)]
    struct CountRow {
        kind: String,
        count: i64,
    }
    #[derive(FromQueryResult)]
    struct ViewerRow {
        kind: String,
    }

    let counts: Vec<CountRow> = PostMediaReaction::find()
        .select_only()
        .column(post_media_reaction::Column::Kind)
        .column_as(post_media_reaction::Column::Id.count(), "count")
        .filter(post_media_reaction::Column::PostMediaId.eq(media_id))
        .group_by(post_media_reaction::Column::Kind)
        .into_model::<CountRow>()
        .all(db)
        .await?;
    let mine: Vec<ViewerRow> = PostMediaReaction::find()
        .select_only()
        .column(post_media_reaction::Column::Kind)
        .filter(post_media_reaction::Column::PostMediaId.eq(media_id))
        .filter(post_media_reaction::Column::UserId.eq(viewer_id))
        .into_model::<ViewerRow>()
        .all(db)
        .await?;

    Ok(MediaReactionStateResponse {
        reaction_counts: counts.into_iter().map(|r| (r.kind, r.count)).collect(),
        viewer_reaction_kinds: mine.into_iter().map(|r| r.kind).collect(),
    })
}
