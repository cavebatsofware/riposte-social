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
//! Albums are `posts.kind='album'` rows under the hood; this module is a
//! shape-translation layer that surfaces the album-shaped wire format
//! (`AlbumResponse`, `AlbumSummary`) so the existing frontend keeps
//! working unchanged. All persistence, kind checks, S3 upload, and
//! ownership enforcement live in `posts::shared`.
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

use crate::entities::{category, post, post_media, user, Category, Post, PostMedia, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::{shared, FeedTier};
use crate::s3::S3Service;
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
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Per-album aggregate row used by `list_albums`. Computed in a single
/// GROUP BY against `post_media` so the list endpoint doesn't load every
/// media row just to surface count + cover.
#[derive(FromQueryResult)]
struct AlbumStatsRow {
    post_id: Uuid,
    photo_count: i64,
    cover_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct AlbumsState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: crate::settings::SettingsService,
}

pub fn album_write_routes() -> Router<AlbumsState> {
    Router::new()
        .route(
            "/api/albums",
            post(create_album).layer(DefaultBodyLimit::max(shared::COMPOSE_BODY_MAX_BYTES)),
        )
        .route(
            "/api/albums/{id}",
            axum::routing::patch(update_album).delete(delete_album),
        )
        .route(
            "/api/albums/{id}/media",
            post(append_album_media).layer(DefaultBodyLimit::max(shared::COMPOSE_BODY_MAX_BYTES)),
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

// ==================== Wire format ====================

#[derive(Deserialize, Default)]
pub struct UpdateAlbumRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    /// Explicit cover override. Translated under the hood into a reorder
    /// that moves the chosen media to ordinal=0 and shifts the others.
    #[serde(default)]
    pub cover_media_id: Option<Uuid>,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub clear_category: bool,
}

#[derive(Deserialize, Default)]
pub struct UpdateAlbumMediaRequest {
    #[serde(default)]
    pub caption: Option<String>,
    /// Direct ordinal patch. Callers that want a full reorder (cover
    /// promotion, drag-to-reorder UI) should use the album-level
    /// `cover_media_id` PATCH or compose multiple PATCH calls; this field
    /// just writes the chosen value through.
    #[serde(default)]
    pub ordinal: Option<i32>,
}

#[derive(Serialize)]
pub struct AlbumMediaResponse {
    pub id: Uuid,
    pub album_id: Uuid,
    pub url: String,
    pub mime_type: String,
    pub media_kind: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ordinal: i32,
    pub caption: Option<String>,
}

impl AlbumMediaResponse {
    fn from_model(album_id: Uuid, m: post_media::Model) -> Self {
        let media_kind = if crate::posts::routes::is_video_mime(&m.mime_type) {
            "video"
        } else {
            "image"
        };
        Self {
            url: format!("/album-media/{}", m.id),
            id: m.id,
            album_id,
            mime_type: m.mime_type,
            media_kind,
            width: m.width,
            height: m.height,
            ordinal: m.ordinal,
            caption: m.caption,
        }
    }
}

#[derive(Serialize)]
pub struct AlbumResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub author_avatar_url: Option<String>,
    pub name: String,
    pub description: Option<String>,
    /// Implicit cover: the lowest-ordinal media. Null when the album has
    /// no media. Stays in the wire format as a derived value so existing
    /// frontend code that reads it still works.
    pub cover_media_id: Option<Uuid>,
    pub cover_url: Option<String>,
    /// Visibility stored on the album row. For categorized albums this
    /// is preserved-but-ignored — the category drives access; see
    /// `effective_visibility`.
    pub visibility: String,
    /// Visibility actually enforced. Equals the category's visibility
    /// when categorized, else the album's own visibility.
    pub effective_visibility: String,
    pub published_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub media: Vec<AlbumMediaResponse>,
    pub photo_count: i64,
    pub category_id: Option<Uuid>,
}

fn build_album_response(
    row: post::Model,
    author: Option<&user::Model>,
    media: Vec<post_media::Model>,
    category: Option<&category::Model>,
) -> AlbumResponse {
    let cover_media_id = shared::implicit_cover_id(&media);
    let cover_url = cover_media_id.map(|id| format!("/album-media/{}", id));
    let photo_count = media.len() as i64;
    let album_id = row.id;
    let media_responses = media
        .into_iter()
        .map(|m| AlbumMediaResponse::from_model(album_id, m))
        .collect();
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    let description = if row.body.is_empty() {
        None
    } else {
        Some(row.body)
    };
    AlbumResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        name: row.slug.unwrap_or_default(),
        description,
        cover_media_id,
        cover_url,
        visibility: row.visibility,
        effective_visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        media: media_responses,
        photo_count,
        category_id: row.category_id,
    }
}

// ==================== Handlers ====================

async fn create_album(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<AlbumResponse>)> {
    let input = shared::parse_compose_multipart(&mut multipart, post::KIND_ALBUM).await?;
    let (post_row, media_rows) =
        shared::commit_compose(&state.db, &state.s3, &state.settings, &user, input).await?;

    let author = User::find_by_id(post_row.author_id).one(&state.db).await?;
    let cat = shared::load_category(&state.db, post_row.category_id).await?;
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

#[derive(Deserialize)]
pub struct ListAlbumsQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub author: Option<Uuid>,
    /// Optional category filter, by slug. `uncategorized` matches albums
    /// with `category_id IS NULL`.
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Serialize)]
pub struct ListAlbumsResponse {
    pub albums: Vec<AlbumSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct AlbumSummary {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_handle: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub visibility: String,
    pub published_at: String,
    pub photo_count: i64,
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

    let mut q = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(post::KIND_ALBUM))
        .filter(ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ))
        .order_by_desc(post::Column::PublishedAt)
        .order_by_desc(post::Column::Id);

    if let Some(author_id) = query.author {
        q = q.filter(post::Column::AuthorId.eq(author_id));
    }
    if let Some(slug) = query.category.as_deref() {
        let slug = slug.trim();
        if slug == "uncategorized" {
            q = q.filter(post::Column::CategoryId.is_null());
        } else {
            let cat = Category::find()
                .filter(category::Column::Slug.eq(slug))
                .one(&state.db)
                .await?;
            match cat {
                Some(c) => q = q.filter(post::Column::CategoryId.eq(c.id)),
                None => {
                    return Ok(Json(ListAlbumsResponse {
                        albums: vec![],
                        next_cursor: None,
                    }));
                }
            }
        }
    }
    if let Some(cursor) = query.cursor.as_deref().and_then(parse_cursor) {
        let (cursor_at, cursor_id) = cursor;
        q = q.filter(
            sea_orm::Condition::any()
                .add(post::Column::PublishedAt.lt(cursor_at))
                .add(
                    sea_orm::Condition::all()
                        .add(post::Column::PublishedAt.eq(cursor_at))
                        .add(post::Column::Id.lt(cursor_id)),
                ),
        );
    }

    let rows = q.limit(limit + 1).all(&state.db).await?;
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
    let authors_by_id: HashMap<Uuid, user::Model> = User::find()
        .filter(user::Column::Id.is_in(author_ids))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|a| (a.id, a))
        .collect();

    // Aggregate per album in one round trip: photo_count via COUNT(*)
    // and the implicit cover (lowest-ordinal media id) via Postgres'
    // ARRAY_AGG ordered subscript. Avoids loading every post_media row
    // for every album in the page just to read count + cover.
    let album_ids: Vec<Uuid> = page.iter().map(|a| a.id).collect();
    let stats_by_album: HashMap<Uuid, AlbumStatsRow> = PostMedia::find()
        .select_only()
        .column(post_media::Column::PostId)
        .column_as(post_media::Column::Id.count(), "photo_count")
        .column_as(
            Expr::cust("(ARRAY_AGG(id ORDER BY ordinal ASC))[1]"),
            "cover_id",
        )
        .filter(post_media::Column::PostId.is_in(album_ids))
        .group_by(post_media::Column::PostId)
        .into_model::<AlbumStatsRow>()
        .all(&state.db)
        .await?
        .into_iter()
        .map(|s| (s.post_id, s))
        .collect();

    let albums: Vec<AlbumSummary> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            let stat = stats_by_album.get(&row.id);
            let cover_url = stat
                .and_then(|s| s.cover_id)
                .map(|id| format!("/album-media/{}", id));
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

    let row = shared::load_post(&state.db, id, post::KIND_ALBUM).await?;

    let cat = shared::load_category(&state.db, row.category_id).await?;
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(row.author_id, &row.visibility, cat.as_ref()) {
        return Err(AppError::NotFound("Album not found".to_string()));
    }

    let media = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(&state.db)
        .await?;
    let author = User::find_by_id(row.author_id).one(&state.db).await?;
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
    let row = shared::load_owned_post(&state.db, id, post::KIND_ALBUM, &user).await?;

    if let Some(ref v) = req.visibility {
        if !post::is_valid_visibility(v) {
            return Err(AppError::ValidationError(format!(
                "Invalid visibility '{}'",
                v
            )));
        }
    }
    if let Some(cover_id) = req.cover_media_id {
        let exists = PostMedia::find_by_id(cover_id)
            .filter(post_media::Column::PostId.eq(id))
            .one(&state.db)
            .await?;
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
        let cat = Category::find_by_id(cid)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
        crate::visibility::ensure_can_compose_into_category(&state.db, &user, &cat).await?;
        active.category_id = Set(Some(cid));
    }
    let updated = active.update(&state.db).await?;

    // Cover override: derived from min(ordinal), so promoting a different
    // photo to cover means reordering the album. Done after the metadata
    // update so a reorder failure doesn't roll back the description / name
    // / visibility edits.
    if let Some(cover_id) = req.cover_media_id {
        let mut media: Vec<post_media::Model> = PostMedia::find()
            .filter(post_media::Column::PostId.eq(id))
            .order_by_asc(post_media::Column::Ordinal)
            .all(&state.db)
            .await?;
        media.retain(|m| m.id != cover_id);
        let mut ordinals: Vec<(Uuid, i32)> = Vec::with_capacity(media.len() + 1);
        ordinals.push((cover_id, 0));
        for (i, m) in media.into_iter().enumerate() {
            ordinals.push((m.id, (i + 1) as i32));
        }
        shared::reorder_media(&state.db, &user, id, post::KIND_ALBUM, ordinals).await?;
    }

    let media = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(&state.db)
        .await?;
    let author = User::find_by_id(updated.author_id).one(&state.db).await?;
    let cat = shared::load_category(&state.db, updated.category_id).await?;
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
    let row = shared::load_owned_post(&state.db, id, post::KIND_ALBUM, &user).await?;
    let mut active: post::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn append_album_media(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<AlbumResponse>> {
    shared::append_media(
        &state.db,
        &state.s3,
        &user,
        id,
        post::KIND_ALBUM,
        &mut multipart,
    )
    .await?;

    // Re-fetch the full album so the response carries the updated
    // photo_count + cover (if the album was empty before, the appended
    // first item becomes the implicit cover).
    let row = shared::load_post(&state.db, id, post::KIND_ALBUM).await?;
    let media = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(&state.db)
        .await?;
    let author = User::find_by_id(row.author_id).one(&state.db).await?;
    let cat = shared::load_category(&state.db, row.category_id).await?;
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
    shared::load_owned_post(&state.db, album_id, post::KIND_ALBUM, &user).await?;

    let media = PostMedia::find_by_id(media_id)
        .filter(post_media::Column::PostId.eq(album_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let mut active: post_media::ActiveModel = media.into();
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
    let updated = active.update(&state.db).await?;
    Ok(Json(AlbumMediaResponse::from_model(album_id, updated)))
}

async fn delete_album_media(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((album_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    shared::delete_media(
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

    let media = PostMedia::find_by_id(media_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;
    let parent = Post::find_by_id(media.post_id)
        .filter(post::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    // /album-media/{id} only serves media owned by an album. Post media
    // is reachable via /media/{id} on the posts router; rejecting here
    // keeps the URL prefix meaningful.
    if parent.kind != post::KIND_ALBUM {
        return Err(AppError::NotFound("Media not found".to_string()));
    }

    let parent_cat = shared::load_category(&state.db, parent.category_id).await?;
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
