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
//! HTTP handlers for albums.
//!
//! Route map:
//! - `POST   /api/albums`                       create + first batch of media (multipart)
//! - `GET    /api/albums`                       visibility-filtered list
//! - `GET    /api/albums/{id}`                  single album with full media list
//! - `PATCH  /api/albums/{id}`                  edit name / description / visibility / cover / category
//! - `DELETE /api/albums/{id}`                  soft delete (author or admin)
//! - `POST   /api/albums/{id}/media`            append media to an existing album
//! - `PATCH  /api/albums/{id}/media/{media_id}` edit caption / ordinal
//! - `DELETE /api/albums/{id}/media/{media_id}` remove one item

use crate::albums::queries::{self, AlbumCategoryFilter, AlbumPageFilters};
use crate::albums::types::{
    build_album_response, AlbumMediaResponse, AlbumResponse, AlbumSummary, ListAlbumsQuery,
    ListAlbumsResponse, UpdateAlbumMediaRequest, UpdateAlbumRequest,
};
use crate::albums::AlbumsState;
use crate::entities::post;
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::{media as posts_media, FeedTier};
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
use uuid::Uuid;

pub fn album_write_routes() -> Router<AlbumsState> {
    Router::new()
        .route(
            "/api/albums",
            post(create_album).layer(DefaultBodyLimit::max(posts_media::COMPOSE_BODY_MAX_BYTES)),
        )
        .route(
            "/api/albums/{id}",
            axum::routing::patch(update_album).delete(delete_album),
        )
        .route(
            "/api/albums/{id}/media",
            post(append_album_media)
                .layer(DefaultBodyLimit::max(posts_media::COMPOSE_BODY_MAX_BYTES)),
        )
        .route(
            "/api/albums/{id}/media/{media_id}",
            axum::routing::patch(update_album_media).delete(delete_album_media),
        )
}

pub fn album_read_routes() -> Router<AlbumsState> {
    Router::new()
        .route("/api/albums", get(list_albums))
        .route("/api/albums/{id}", get(get_album))
        .route("/album-media/{media_id}", get(serve_album_media))
}

async fn create_album(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<AlbumResponse>)> {
    let input = posts_media::parse_compose_multipart(&mut multipart, post::KIND_ALBUM).await?;
    let (post_row, media_rows) =
        posts_media::commit_compose(&state.db, &state.s3, &state.settings, &user, input).await?;

    let author = queries::find_user(&state.db, post_row.author_id).await?;
    let cat = posts_media::load_category(&state.db, post_row.category_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(build_album_response(
            post_row,
            author.as_ref(),
            media_rows,
            cat.as_ref(),
        )),
    ))
}

const ALBUMS_LIMIT_DEFAULT: u64 = 20;
const ALBUMS_LIMIT_MAX: u64 = 100;

async fn list_albums(
    State(state): State<AlbumsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Query(query): Query<ListAlbumsQuery>,
) -> AppResult<Json<ListAlbumsResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;
    let limit = query
        .limit
        .unwrap_or(ALBUMS_LIMIT_DEFAULT)
        .clamp(1, ALBUMS_LIMIT_MAX);

    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;

    let category_filter = match query.category.as_deref() {
        None => AlbumCategoryFilter::Any,
        Some(slug) => {
            let slug = slug.trim();
            if slug == "uncategorized" {
                AlbumCategoryFilter::Uncategorized
            } else {
                match queries::find_category_by_slug(&state.db, slug).await? {
                    Some(c) => AlbumCategoryFilter::InCategory(c.id),
                    None => {
                        return Ok(Json(ListAlbumsResponse {
                            albums: vec![],
                            next_cursor: None,
                        }));
                    }
                }
            }
        }
    };

    let cursor = query.cursor.as_deref().and_then(parse_cursor);
    let filters = AlbumPageFilters {
        feed_condition: ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ),
        author: query.author,
        category_filter,
        cursor,
        fetch: limit + 1,
    };
    let rows = queries::list_album_posts(&state.db, filters).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<post::Model> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = if has_more {
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

    let author_ids: Vec<Uuid> = page.iter().map(|a| a.author_id).collect();
    let authors_by_id = queries::load_authors_by_ids(&state.db, author_ids).await?;
    let album_ids: Vec<Uuid> = page.iter().map(|a| a.id).collect();
    let stats_by_album = queries::aggregate_album_stats(&state.db, album_ids).await?;

    let albums: Vec<AlbumSummary> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            let stat = stats_by_album.get(&row.id);
            let cover_url = stat
                .and_then(|s| s.cover_id)
                .map(|id| format!("/album-media/{}", id));
            let cover_icon_data = stat
                .and_then(|s| s.cover_icon_data.as_deref())
                .map(crate::posts::types::encode_webp_data_uri);
            let photo_count = stat.map(|s| s.photo_count).unwrap_or(0);
            let description = if row.body.is_empty() {
                None
            } else {
                Some(row.body)
            };
            AlbumSummary {
                id: row.id,
                author_id: row.author_id,
                author_display: author.and_then(|u| u.display_name.clone()),
                author_handle: author.map(|u| u.handle.clone()),
                cover_url,
                cover_icon_data,
                name: row.slug.unwrap_or_default(),
                description,
                visibility: row.visibility,
                published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
                photo_count,
            }
        })
        .collect();

    Ok(Json(ListAlbumsResponse {
        albums,
        next_cursor,
    }))
}

async fn get_album(
    State(state): State<AlbumsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AlbumResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;

    let row = posts_media::load_post(&state.db, id, post::KIND_ALBUM).await?;

    let cat = posts_media::load_category(&state.db, row.category_id).await?;
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(row.author_id, &row.visibility, cat.as_ref()) {
        return Err(AppError::NotFound("Album not found".to_string()));
    }

    let media = queries::list_post_media_ordered(&state.db, id).await?;
    let author = queries::find_user(&state.db, row.author_id).await?;
    Ok(Json(build_album_response(
        row,
        author.as_ref(),
        media,
        cat.as_ref(),
    )))
}

async fn update_album(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAlbumRequest>,
) -> AppResult<Json<AlbumResponse>> {
    let row = posts_media::load_owned_post(&state.db, id, post::KIND_ALBUM, &user).await?;

    if let Some(ref v) = req.visibility {
        if !post::is_valid_visibility(v) {
            return Err(AppError::ValidationError(format!(
                "Invalid visibility '{}'",
                v
            )));
        }
    }
    if let Some(cover_id) = req.cover_media_id {
        let exists = queries::find_post_media_in_post(&state.db, cover_id, id).await?;
        if exists.is_none() {
            return Err(AppError::ValidationError(
                "cover_media_id must refer to a media item in this album".to_string(),
            ));
        }
    }

    let mut active: post::ActiveModel = row.into();
    if let Some(name) = req.name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::ValidationError(
                "name cannot be empty".to_string(),
            ));
        }
        if trimmed.chars().count() > post::SLUG_MAX_LEN {
            return Err(AppError::ValidationError(format!(
                "name exceeds {}-character limit",
                post::SLUG_MAX_LEN
            )));
        }
        active.slug = Set(Some(trimmed));
    }
    if let Some(d) = req.description {
        active.body = Set(d.trim().to_string());
    }
    if let Some(v) = req.visibility {
        active.visibility = Set(v);
    }
    if req.clear_category {
        active.category_id = Set(None);
    } else if let Some(cid) = req.category_id {
        let cat = queries::find_category(&state.db, cid)
            .await?
            .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
        crate::visibility::ensure_can_compose_into_category(&state.db, &user, &cat).await?;
        active.category_id = Set(Some(cid));
    }
    let updated = queries::save_album(&state.db, active).await?;

    // Cover override: derived from min(ordinal), so promoting a different
    // photo to cover means reordering the album. Done after the metadata
    // update so a reorder failure doesn't roll back the description / name
    // / visibility edits.
    if let Some(cover_id) = req.cover_media_id {
        let mut media = queries::list_post_media_ordered(&state.db, id).await?;
        media.retain(|m| m.id != cover_id);
        let mut ordinals: Vec<(Uuid, i32)> = Vec::with_capacity(media.len() + 1);
        ordinals.push((cover_id, 0));
        for (i, m) in media.into_iter().enumerate() {
            ordinals.push((m.id, (i + 1) as i32));
        }
        posts_media::reorder_media(&state.db, &user, id, post::KIND_ALBUM, ordinals).await?;
    }

    let media = queries::list_post_media_ordered(&state.db, id).await?;
    let author = queries::find_user(&state.db, updated.author_id).await?;
    let cat = posts_media::load_category(&state.db, updated.category_id).await?;
    Ok(Json(build_album_response(
        updated,
        author.as_ref(),
        media,
        cat.as_ref(),
    )))
}

async fn delete_album(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = posts_media::load_owned_post(&state.db, id, post::KIND_ALBUM, &user).await?;
    let mut active: post::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    queries::save_album(&state.db, active).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn append_album_media(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<AlbumResponse>> {
    posts_media::append_media(
        &state.db,
        &state.s3,
        &user,
        id,
        post::KIND_ALBUM,
        &mut multipart,
    )
    .await?;

    let row = posts_media::load_post(&state.db, id, post::KIND_ALBUM).await?;
    let media = queries::list_post_media_ordered(&state.db, id).await?;
    let author = queries::find_user(&state.db, row.author_id).await?;
    let cat = posts_media::load_category(&state.db, row.category_id).await?;
    Ok(Json(build_album_response(
        row,
        author.as_ref(),
        media,
        cat.as_ref(),
    )))
}

async fn update_album_media(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((album_id, media_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateAlbumMediaRequest>,
) -> AppResult<Json<AlbumMediaResponse>> {
    posts_media::load_owned_post(&state.db, album_id, post::KIND_ALBUM, &user).await?;

    let media = queries::find_post_media_in_post(&state.db, media_id, album_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let mut active: crate::entities::post_media::ActiveModel = media.into();
    if let Some(c) = req.caption {
        let trimmed = c.trim();
        active.caption = Set(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        });
    }
    if let Some(o) = req.ordinal {
        active.ordinal = Set(o);
    }
    let updated = queries::save_album_media(&state.db, active).await?;
    Ok(Json(AlbumMediaResponse::from_model(album_id, updated)))
}

async fn delete_album_media(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((album_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    posts_media::delete_media(
        &state.db,
        &state.s3,
        &user,
        album_id,
        media_id,
        post::KIND_ALBUM,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn serve_album_media(
    State(state): State<AlbumsState>,
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

    // /album-media/{id} only serves media owned by an album. Post media
    // is reachable via /media/{id} on the posts router; rejecting here
    // keeps the URL prefix meaningful.
    if parent.kind != post::KIND_ALBUM {
        return Err(AppError::NotFound("Media not found".to_string()));
    }

    let parent_cat = posts_media::load_category(&state.db, parent.category_id).await?;
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

async fn caller_tier(auth_session: &AuthSession<crate::admin::UserAuthBackend>) -> FeedTier {
    let user = auth_session.user().await;
    FeedTier::from_role(user.as_ref().map(|u| u.role.as_str()))
}

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
