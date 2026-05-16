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
//! HTTP handlers for post CRUD and feed queries.
//!
//! Route map:
//! - `POST   /api/posts`           gated by `require_admin_or_poster`
//! - `GET    /api/posts/{id}`      public; visibility-filtered per caller tier
//! - `PATCH  /api/posts/{id}`      author or admin
//! - `DELETE /api/posts/{id}`      author or admin (soft delete)
//! - `GET    /api/feed`            public; cursor-paginated, visibility-filtered

use crate::engagement::{fetch_engagement_for_posts, PostEngagement};
use crate::entities::{post, post_media, user};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::media::{self, COMPOSE_BODY_MAX_BYTES};
use crate::posts::queries::{self, FeedPageFilters, PostCategoryFilter};
use crate::posts::types::{
    build_post_response, FeedQuery, FeedResponse, MediaOrderItem, PostMediaResponse, PostResponse,
    UpdatePostMediaRequest, UpdatePostRequest,
};
use crate::posts::{FeedTier, PostsState};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Extension, Router,
};
use axum_login::AuthSession;
use chrono::Utc;
use sea_orm::Set;
use std::collections::HashMap;
use uuid::Uuid;

pub fn post_write_routes() -> Router<PostsState> {
    Router::new()
        .route(
            "/api/posts",
            post(create_post).layer(DefaultBodyLimit::max(COMPOSE_BODY_MAX_BYTES)),
        )
        .route(
            "/api/posts/{id}",
            axum::routing::patch(update_post).delete(delete_post),
        )
        .route(
            "/api/posts/{id}/media",
            post(append_post_media).layer(DefaultBodyLimit::max(COMPOSE_BODY_MAX_BYTES)),
        )
        .route(
            "/api/posts/{id}/media/order",
            axum::routing::put(reorder_post_media),
        )
        .route(
            "/api/posts/{id}/media/{media_id}",
            axum::routing::patch(update_post_media).delete(delete_post_media),
        )
}

pub fn post_read_routes() -> Router<PostsState> {
    Router::new()
        .route("/api/posts/{id}", get(get_post))
        .route("/api/feed", get(feed))
        .route("/media/{media_id}", get(serve_media))
}

async fn create_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<PostResponse>)> {
    let input = media::parse_compose_multipart(&mut multipart, post::KIND_POST).await?;
    let (post_row, media_rows) =
        media::commit_compose(&state.db, &state.s3, &state.settings, &user, input).await?;

    crate::metrics::POSTS_CREATED_TOTAL.inc();
    let author = queries::find_user(&state.db, post_row.author_id).await?;
    let cat = media::load_category(&state.db, post_row.category_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(build_post_response(
            post_row,
            author.as_ref(),
            media_rows,
            PostEngagement::default(),
            cat.as_ref(),
            &HashMap::new(),
        )),
    ))
}

async fn append_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<Vec<PostMediaResponse>>> {
    let rows = media::append_media(
        &state.db,
        &state.s3,
        &user,
        id,
        post::KIND_POST,
        &mut multipart,
    )
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(PostMediaResponse::from_model)
            .collect(),
    ))
}

async fn update_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdatePostMediaRequest>,
) -> AppResult<Json<PostMediaResponse>> {
    let row = media::edit_media_caption(
        &state.db,
        &user,
        post_id,
        media_id,
        post::KIND_POST,
        req.caption,
    )
    .await?;
    Ok(Json(PostMediaResponse::from_model(row)))
}

async fn delete_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    media::delete_media(
        &state.db,
        &state.s3,
        &user,
        post_id,
        media_id,
        post::KIND_POST,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reorder_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(post_id): Path<Uuid>,
    Json(items): Json<Vec<MediaOrderItem>>,
) -> AppResult<StatusCode> {
    let ordinals: Vec<(Uuid, i32)> = items.into_iter().map(|i| (i.id, i.ordinal)).collect();
    media::reorder_media(&state.db, &user, post_id, post::KIND_POST, ordinals).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn serve_media(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(media_id): Path<Uuid>,
) -> AppResult<Response> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier)
        .await
        .map_err(|_| AppError::NotFound("Media not found".to_string()))?;

    let media = queries::find_post_media(&state.db, media_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let parent = queries::find_post_active(&state.db, media.post_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let parent_cat = match parent.category_id {
        Some(cid) => queries::find_category(&state.db, cid).await?,
        None => None,
    };
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(parent.author_id, &parent.visibility, parent_cat.as_ref()) {
        return Err(AppError::NotFound("Media not found".to_string()));
    }

    let (bytes, stored_type) = state
        .s3
        .get_object_at(&media.s3_key)
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to load media: {}", e)))?;

    // Effective visibility (category-driven if categorized) decides cache
    // policy. Public goes to a shared cache; everything else stays private.
    let effective_vis = parent_cat
        .as_ref()
        .map(|c| c.visibility.as_str())
        .unwrap_or(parent.visibility.as_str());
    let cache_control = if effective_vis == post::VISIBILITY_PUBLIC {
        "public, max-age=86400"
    } else {
        "private, max-age=300"
    };

    let content_type = stored_type
        .or(Some(media.mime_type.clone()))
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, cache_control.to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

async fn get_post(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;

    let row = media::load_post(&state.db, id, post::KIND_POST).await?;

    let parent_cat = match row.category_id {
        Some(cid) => queries::find_category(&state.db, cid).await?,
        None => None,
    };
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    let viewer_id = ctx.viewer_id;
    if !ctx.permits_read(row.author_id, &row.visibility, parent_cat.as_ref()) {
        return Err(AppError::NotFound("Post not found".to_string()));
    }

    let media_rows = queries::list_post_media_ordered(&state.db, id).await?;
    let author = queries::find_user(&state.db, row.author_id).await?;
    let mut engagement_map = fetch_engagement_for_posts(&state.db, &[row.id], viewer_id).await?;
    let engagement = engagement_map.remove(&row.id).unwrap_or_default();
    let comment_authors = load_top_comment_authors(&state.db, std::iter::once(&engagement)).await?;
    Ok(Json(build_post_response(
        row,
        author.as_ref(),
        media_rows,
        engagement,
        parent_cat.as_ref(),
        &comment_authors,
    )))
}

async fn update_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePostRequest>,
) -> AppResult<Json<PostResponse>> {
    let row = media::load_post(&state.db, id, post::KIND_POST).await?;

    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::Forbidden(
            "Only the author or an administrator can edit this post".to_string(),
        ));
    }

    if let Some(ref v) = req.visibility {
        if !post::is_valid_visibility(v) {
            return Err(AppError::ValidationError(format!(
                "Invalid visibility '{}'",
                v
            )));
        }
    }
    if let Some(body) = req.body.as_deref() {
        if body.trim().is_empty() {
            return Err(AppError::ValidationError(
                "Body cannot be empty".to_string(),
            ));
        }
    }
    if let Some(cid) = req.category_id {
        if queries::find_category(&state.db, cid).await?.is_none() {
            return Err(AppError::ValidationError("Category not found".to_string()));
        }
    }
    // Compose-time member check: if the post is moving INTO a category,
    // confirm the author may compose there. Admins always pass.
    if !req.clear_category {
        if let Some(cid) = req.category_id {
            let cat = queries::find_category(&state.db, cid)
                .await?
                .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
            crate::visibility::ensure_can_compose_into_category(&state.db, &user, &cat).await?;
        }
    }

    let mut active: post::ActiveModel = row.into();
    if req.clear_category {
        active.category_id = Set(None);
    } else if let Some(cid) = req.category_id {
        active.category_id = Set(Some(cid));
    }
    if let Some(body) = req.body {
        active.body = Set(body);
    }
    if let Some(v) = req.visibility {
        active.visibility = Set(v);
    }
    let updated = queries::save_post(&state.db, active).await?;

    let media_rows = queries::list_post_media_ordered(&state.db, id).await?;
    let author = queries::find_user(&state.db, updated.author_id).await?;
    let mut engagement_map =
        fetch_engagement_for_posts(&state.db, &[updated.id], Some(user.id)).await?;
    let engagement = engagement_map.remove(&updated.id).unwrap_or_default();
    let cat = match updated.category_id {
        Some(cid) => queries::find_category(&state.db, cid).await?,
        None => None,
    };
    let comment_authors = load_top_comment_authors(&state.db, std::iter::once(&engagement)).await?;
    Ok(Json(build_post_response(
        updated,
        author.as_ref(),
        media_rows,
        engagement,
        cat.as_ref(),
        &comment_authors,
    )))
}

async fn delete_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = media::load_post(&state.db, id, post::KIND_POST).await?;

    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::Forbidden(
            "Only the author or an administrator can delete this post".to_string(),
        ));
    }

    let mut active: post::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    queries::save_post(&state.db, active).await?;
    Ok(StatusCode::NO_CONTENT)
}

const FEED_LIMIT_DEFAULT: u64 = 20;
const FEED_LIMIT_MAX: u64 = 100;

/// `GET /api/feed`. Cursor-paginated feed scoped to the caller's tier.
/// Anonymous visitors get public posts only; commenters get public +
/// commenter-visible; posters and admins see everything.
async fn feed(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Query(query): Query<FeedQuery>,
) -> AppResult<Json<FeedResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;
    let limit = query
        .limit
        .unwrap_or(FEED_LIMIT_DEFAULT)
        .clamp(1, FEED_LIMIT_MAX);

    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;

    let search_term = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let category_filter = match query.category.as_deref() {
        None => PostCategoryFilter::Any,
        Some(slug) => {
            let slug = slug.trim();
            if slug == "uncategorized" {
                PostCategoryFilter::Uncategorized
            } else {
                match queries::find_category_by_slug(&state.db, slug).await? {
                    Some(c) => PostCategoryFilter::InCategory(c.id),
                    None => {
                        return Ok(Json(FeedResponse {
                            posts: vec![],
                            next_cursor: None,
                        }));
                    }
                }
            }
        }
    };

    let cursor = if search_term.is_some() {
        None
    } else {
        query.cursor.as_deref().and_then(parse_cursor)
    };
    // For non-search, fetch one extra row to detect a next page without a count query.
    let fetch_limit = if search_term.is_some() {
        limit
    } else {
        limit + 1
    };

    let feed_filters = FeedPageFilters {
        feed_condition: ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ),
        author: query.author,
        category_filter,
        cursor,
        limit: fetch_limit,
        search_term: search_term.clone(),
    };
    let rows = queries::list_feed_page(&state.db, feed_filters).await?;

    let (page, next_cursor): (Vec<post::Model>, Option<String>) = if search_term.is_some() {
        (rows, None)
    } else {
        let has_more = rows.len() as u64 > limit;
        let page: Vec<post::Model> = rows.into_iter().take(limit as usize).collect();
        let nc = if has_more {
            page.last().map(|last| {
                format!(
                    "{}_{}",
                    last.published_at.with_timezone(&Utc).to_rfc3339(),
                    last.id
                )
            })
        } else {
            None
        };
        (page, nc)
    };

    // Hydrate the page in parallel: authors, media, engagement, categories.
    let author_ids: Vec<Uuid> = page.iter().map(|p| p.author_id).collect();
    let post_ids: Vec<Uuid> = page.iter().map(|p| p.id).collect();
    let category_ids: Vec<Uuid> = page.iter().filter_map(|p| p.category_id).collect();
    let viewer_id = ctx.viewer_id;
    let post_ids_for_engagement = post_ids.clone();

    let (authors, media_rows, engagement_by_post_raw, categories) = tokio::try_join!(
        async { queries::load_users_by_ids(&state.db, author_ids).await },
        async { queries::load_media_for_posts(&state.db, post_ids.clone()).await },
        async {
            fetch_engagement_for_posts(&state.db, &post_ids_for_engagement, viewer_id)
                .await
                .map_err(|e| sea_orm::DbErr::Custom(format!("engagement fetch failed: {:#}", e)))
        },
        async { queries::load_categories_by_ids(&state.db, category_ids).await },
    )?;

    let authors_by_id: HashMap<Uuid, user::Model> =
        authors.into_iter().map(|a| (a.id, a)).collect();
    let mut media_by_post: HashMap<Uuid, Vec<post_media::Model>> = HashMap::new();
    for m in media_rows {
        media_by_post.entry(m.post_id).or_default().push(m);
    }
    let categories_by_id: HashMap<Uuid, crate::entities::category::Model> =
        categories.into_iter().map(|c| (c.id, c)).collect();
    let mut engagement_by_post = engagement_by_post_raw;

    let top_comment_authors =
        load_top_comment_authors(&state.db, engagement_by_post.values()).await?;

    let posts: Vec<PostResponse> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            let media_list = media_by_post.remove(&row.id).unwrap_or_default();
            let engagement = engagement_by_post.remove(&row.id).unwrap_or_default();
            let cat = row.category_id.and_then(|id| categories_by_id.get(&id));
            build_post_response(
                row,
                author,
                media_list,
                engagement,
                cat,
                &top_comment_authors,
            )
        })
        .collect();

    Ok(Json(FeedResponse { posts, next_cursor }))
}

async fn load_top_comment_authors<'a, I>(
    db: &sea_orm::DatabaseConnection,
    engagement_iter: I,
) -> AppResult<HashMap<Uuid, user::Model>>
where
    I: IntoIterator<Item = &'a PostEngagement>,
{
    let mut ids: Vec<Uuid> = engagement_iter
        .into_iter()
        .flat_map(|e| e.top_comments.iter().map(|c| c.user_id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    Ok(queries::load_top_comment_authors(db, ids).await?)
}

async fn caller_tier(auth_session: &AuthSession<crate::admin::UserAuthBackend>) -> FeedTier {
    let user = auth_session.user().await;
    FeedTier::from_role(user.as_ref().map(|u| u.role.as_str()))
}

/// Block anonymous reads when `public_feed_enabled` is off. Authed
/// callers (any tier) bypass. A settings read failure surfaces as a 500
/// (fail-closed for a security gate).
async fn enforce_public_feed_gate(
    settings: &crate::settings::SettingsService,
    tier: FeedTier,
) -> AppResult<()> {
    if !matches!(tier, FeedTier::Anonymous) {
        return Ok(());
    }
    let enabled = settings
        .get_public_feed_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if enabled {
        return Ok(());
    }
    Err(AppError::NotFound("Not found".to_string()))
}

fn parse_cursor(cursor: &str) -> Option<(chrono::DateTime<chrono::FixedOffset>, Uuid)> {
    let (ts, id) = cursor.rsplit_once('_')?;
    let parsed_ts = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let parsed_id = Uuid::parse_str(id).ok()?;
    Some((parsed_ts, parsed_id))
}

#[allow(dead_code)]
type _Marker = AuthenticatedUser;
