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
//! - `POST   /api/albums`                       create (admin/poster) + first batch of media (multipart)
//! - `GET    /api/albums`                       visibility-filtered list (used by left rail + /albums overflow)
//! - `GET    /api/albums/{id}`                  single album with full media list
//! - `PATCH  /api/albums/{id}`                  edit name/description/visibility/cover
//! - `DELETE /api/albums/{id}`                  soft delete (author or admin)
//! - `POST   /api/albums/{id}/media`            append media to an existing album
//! - `PATCH  /api/albums/{id}/media/{media_id}` edit caption / ordinal
//! - `DELETE /api/albums/{id}/media/{media_id}` remove one item

use crate::entities::{
    album, album_media, category, post, user, Album, AlbumMedia, Category, User,
};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::FeedTier;
use crate::s3::S3Service;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use axum_login::AuthSession;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct AlbumsState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: crate::settings::SettingsService,
}

const POST_BODY_MAX_BYTES: usize = 256 * 1024 * 1024;
const MEDIA_FILES_MAX: usize = 50;

pub fn album_write_routes() -> Router<AlbumsState> {
    Router::new()
        .route(
            "/api/albums",
            post(create_album).layer(DefaultBodyLimit::max(POST_BODY_MAX_BYTES)),
        )
        .route(
            "/api/albums/{id}",
            axum::routing::patch(update_album).delete(delete_album),
        )
        .route(
            "/api/albums/{id}/media",
            post(append_album_media).layer(DefaultBodyLimit::max(POST_BODY_MAX_BYTES)),
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

#[derive(Deserialize, Default)]
pub struct UpdateAlbumRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub cover_media_id: Option<Uuid>,
    /// Phase 9e: assign / reassign a category. Pair with `clear_category=true`
    /// to clear an existing category. Same convention as posts' update.
    #[serde(default)]
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub clear_category: bool,
}

#[derive(Deserialize, Default)]
pub struct UpdateAlbumMediaRequest {
    #[serde(default)]
    pub caption: Option<String>,
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
    fn from_model(m: album_media::Model) -> Self {
        let media_kind = if crate::posts::routes::is_video_mime(&m.mime_type) {
            "video"
        } else {
            "image"
        };
        Self {
            url: format!("/album-media/{}", m.id),
            id: m.id,
            album_id: m.album_id,
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
    /// Phase 9e: nullable category id. The album's category metadata
    /// (name/slug/color) isn't included here in v1; the rail and admin
    /// page use the categories list directly.
    pub category_id: Option<Uuid>,
}

fn build_album_response(
    row: album::Model,
    author: Option<&user::Model>,
    media: Vec<album_media::Model>,
    category: Option<&category::Model>,
) -> AlbumResponse {
    let cover_url = row.cover_media_id.map(|id| format!("/album-media/{}", id));
    let photo_count = media.len() as i64;
    let media_responses = media
        .into_iter()
        .map(AlbumMediaResponse::from_model)
        .collect();
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    AlbumResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        name: row.name,
        description: row.description,
        cover_media_id: row.cover_media_id,
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

struct PendingMedia {
    bytes: Vec<u8>,
    mime_type: String,
    caption: Option<String>,
}

async fn create_album(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<AlbumResponse>)> {
    if user.role == user::ROLE_POSTER {
        let enabled = state
            .settings
            .get_poster_posting_enabled()
            .await
            .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
        if !enabled {
            return Err(AppError::AuthError(
                "Posting is currently disabled by an administrator".to_string(),
            ));
        }
    }

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut visibility = post::VISIBILITY_PRIVATE.to_string();
    let mut published_at: Option<DateTime<Utc>> = None;
    let mut media: Vec<PendingMedia> = Vec::new();
    let mut captions_by_index: HashMap<usize, String> = HashMap::new();
    let mut category_id: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to parse multipart form: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "name" => {
                name = Some(field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read name: {}", e))
                })?);
            }
            "description" => {
                description = Some(field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read description: {}", e))
                })?);
            }
            "visibility" => {
                visibility = field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read visibility: {}", e))
                })?;
            }
            "category_id" => {
                let text = field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read category_id: {}", e))
                })?;
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    category_id = Some(Uuid::parse_str(trimmed).map_err(|e| {
                        AppError::ValidationError(format!("category_id must be a UUID: {}", e))
                    })?);
                }
            }
            "published_at" => {
                let text = field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read published_at: {}", e))
                })?;
                if !text.is_empty() {
                    published_at = Some(
                        DateTime::parse_from_rfc3339(&text)
                            .map_err(|e| {
                                AppError::ValidationError(format!(
                                    "published_at must be RFC3339: {}",
                                    e
                                ))
                            })?
                            .with_timezone(&Utc),
                    );
                }
            }
            "media" => {
                if media.len() >= MEDIA_FILES_MAX {
                    return Err(AppError::ValidationError(format!(
                        "At most {} media files per album in a single request",
                        MEDIA_FILES_MAX
                    )));
                }
                let mime = field.content_type().map(|s| s.to_string()).ok_or_else(|| {
                    AppError::ValidationError("Media field must include a Content-Type".to_string())
                })?;
                if !crate::posts::routes::is_allowed_media_mime(&mime) {
                    return Err(AppError::ValidationError(format!(
                        "Unsupported media type '{}'",
                        mime
                    )));
                }
                let bytes = field.bytes().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read media bytes: {}", e))
                })?;
                let cap = crate::posts::routes::max_bytes_for_mime(&mime);
                if bytes.len() > cap {
                    return Err(AppError::ValidationError(format!(
                        "Media file ({}) exceeds {} byte limit",
                        bytes.len(),
                        cap
                    )));
                }
                media.push(PendingMedia {
                    bytes: bytes.to_vec(),
                    mime_type: mime,
                    caption: None,
                });
            }
            other if other.starts_with("caption_") => {
                if let Ok(idx) = other.trim_start_matches("caption_").parse::<usize>() {
                    let text = field.text().await.unwrap_or_default();
                    if !text.is_empty() {
                        captions_by_index.insert(idx, text);
                    }
                }
            }
            _ => {}
        }
    }

    let name = name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::ValidationError("Missing required field: name".to_string()))?;
    if !post::is_valid_visibility(&visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            visibility
        )));
    }
    for (idx, caption) in captions_by_index {
        if let Some(m) = media.get_mut(idx) {
            m.caption = Some(caption);
        }
    }
    if let Some(cid) = category_id {
        let cat = Category::find_by_id(cid)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
        crate::visibility::ensure_can_compose_into_category(&state.db, &user, &cat).await?;
    }

    let album_id = Uuid::new_v4();
    let now = Utc::now();

    let media_plan: Vec<(Uuid, String, PendingMedia, i32)> = media
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let media_id = Uuid::new_v4();
            let key = format!("albums/{}/{}", album_id, media_id);
            (media_id, key, m, i as i32)
        })
        .collect();

    let mut uploaded_keys: Vec<String> = Vec::new();
    for (_id, key, m, _ord) in &media_plan {
        if let Err(e) = state
            .s3
            .put_object_at(key, m.bytes.clone(), &m.mime_type)
            .await
        {
            for k in &uploaded_keys {
                let _ = state.s3.delete_object_at(k).await;
            }
            return Err(AppError::AuthError(format!(
                "Failed to upload media: {}",
                e
            )));
        }
        uploaded_keys.push(key.clone());
    }

    let cover_media_id = media_plan.first().map(|(id, _, _, _)| *id);

    let txn_result = async {
        let txn = state.db.begin().await?;
        let album_row = album::ActiveModel {
            id: Set(album_id),
            author_id: Set(user.id),
            name: Set(name),
            description: Set(description.filter(|s| !s.is_empty())),
            cover_media_id: Set(cover_media_id),
            visibility: Set(visibility),
            published_at: Set(published_at.unwrap_or(now).into()),
            import_source: Set(None),
            import_external_id: Set(None),
            deleted_at: Set(None),
            category_id: Set(category_id),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        let mut media_rows: Vec<album_media::Model> = Vec::with_capacity(media_plan.len());
        for (media_id, key, m, ordinal) in &media_plan {
            let row = album_media::ActiveModel {
                id: Set(*media_id),
                album_id: Set(album_id),
                s3_key: Set(key.clone()),
                mime_type: Set(m.mime_type.clone()),
                width: Set(None),
                height: Set(None),
                ordinal: Set(*ordinal),
                caption: Set(m.caption.clone()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
            media_rows.push(row);
        }
        txn.commit().await?;
        Ok::<(album::Model, Vec<album_media::Model>), sea_orm::DbErr>((album_row, media_rows))
    }
    .await;

    let (album_row, media_rows) = match txn_result {
        Ok(pair) => pair,
        Err(e) => {
            for k in &uploaded_keys {
                let _ = state.s3.delete_object_at(k).await;
            }
            return Err(AppError::AuthError(format!(
                "Failed to create album: {}",
                e
            )));
        }
    };

    let author = User::find_by_id(album_row.author_id).one(&state.db).await?;
    let cat = load_album_category(&state.db, &album_row).await?;
    Ok((
        StatusCode::CREATED,
        Json(build_album_response(
            album_row,
            author.as_ref(),
            media_rows,
            cat.as_ref(),
        )),
    ))
}

/// Fetch the album's category row, when set. Used by every endpoint that
/// produces an `AlbumResponse` so `effective_visibility` reflects the
/// category-driven tier
async fn load_album_category(
    db: &sea_orm::DatabaseConnection,
    row: &album::Model,
) -> AppResult<Option<category::Model>> {
    if let Some(cid) = row.category_id {
        Ok(Category::find_by_id(cid).one(db).await?)
    } else {
        Ok(None)
    }
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
        .min(ALBUMS_LIMIT_MAX)
        .max(1);

    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;

    let mut q = Album::find()
        .filter(album::Column::DeletedAt.is_null())
        .filter(ctx.feed_condition(
            album::Column::Visibility,
            album::Column::AuthorId,
            album::Column::CategoryId,
        ))
        .order_by_desc(album::Column::PublishedAt)
        .order_by_desc(album::Column::Id);

    if let Some(author_id) = query.author {
        q = q.filter(album::Column::AuthorId.eq(author_id));
    }
    if let Some(slug) = query.category.as_deref() {
        let slug = slug.trim();
        if slug == "uncategorized" {
            q = q.filter(album::Column::CategoryId.is_null());
        } else {
            let cat = Category::find()
                .filter(category::Column::Slug.eq(slug))
                .one(&state.db)
                .await?;
            match cat {
                Some(c) => q = q.filter(album::Column::CategoryId.eq(c.id)),
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
                .add(album::Column::PublishedAt.lt(cursor_at))
                .add(
                    sea_orm::Condition::all()
                        .add(album::Column::PublishedAt.eq(cursor_at))
                        .add(album::Column::Id.lt(cursor_id)),
                ),
        );
    }

    let rows = q.limit(limit + 1).all(&state.db).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<album::Model> = rows.into_iter().take(limit as usize).collect();

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

    let album_ids: Vec<Uuid> = page.iter().map(|a| a.id).collect();
    let counts: Vec<(Uuid, i64)> = if album_ids.is_empty() {
        vec![]
    } else {
        AlbumMedia::find()
            .filter(album_media::Column::AlbumId.is_in(album_ids))
            .select_only()
            .column(album_media::Column::AlbumId)
            .column_as(album_media::Column::Id.count(), "photo_count")
            .group_by(album_media::Column::AlbumId)
            .into_tuple()
            .all(&state.db)
            .await?
    };
    let counts_by_album: HashMap<Uuid, i64> = counts.into_iter().collect();

    let albums: Vec<AlbumSummary> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            AlbumSummary {
                id: row.id,
                author_id: row.author_id,
                author_display: author.and_then(|u| u.display_name.clone()),
                author_handle: author.map(|u| u.handle.clone()),
                cover_url: row.cover_media_id.map(|id| format!("/album-media/{}", id)),
                name: row.name,
                description: row.description,
                visibility: row.visibility,
                published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
                photo_count: counts_by_album.get(&row.id).copied().unwrap_or(0),
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

    let row = Album::find_by_id(id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Album not found".to_string()))?;

    let cat = load_album_category(&state.db, &row).await?;
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.can_view_album(&row, cat.as_ref()) {
        return Err(AppError::AuthError("Album not found".to_string()));
    }

    let media = AlbumMedia::find()
        .filter(album_media::Column::AlbumId.eq(id))
        .order_by_asc(album_media::Column::Ordinal)
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
    let row = Album::find_by_id(id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Album not found".to_string()))?;

    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
            "Only the author or an administrator can edit this album".to_string(),
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
    if let Some(cover_id) = req.cover_media_id {
        let exists = AlbumMedia::find_by_id(cover_id)
            .filter(album_media::Column::AlbumId.eq(id))
            .one(&state.db)
            .await?;
        if exists.is_none() {
            return Err(AppError::ValidationError(
                "cover_media_id must refer to a media item in this album".to_string(),
            ));
        }
    }

    let mut active: album::ActiveModel = row.into();
    if let Some(name) = req.name {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::ValidationError(
                "name cannot be empty".to_string(),
            ));
        }
        active.name = Set(trimmed);
    }
    if let Some(d) = req.description {
        let trimmed = d.trim();
        active.description = Set(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        });
    }
    if let Some(v) = req.visibility {
        active.visibility = Set(v);
    }
    if let Some(cover_id) = req.cover_media_id {
        active.cover_media_id = Set(Some(cover_id));
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

    let media = AlbumMedia::find()
        .filter(album_media::Column::AlbumId.eq(id))
        .order_by_asc(album_media::Column::Ordinal)
        .all(&state.db)
        .await?;
    let author = User::find_by_id(updated.author_id).one(&state.db).await?;
    let cat = load_album_category(&state.db, &updated).await?;
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
    let row = Album::find_by_id(id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Album not found".to_string()))?;
    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
            "Only the author or an administrator can delete this album".to_string(),
        ));
    }
    let mut active: album::ActiveModel = row.into();
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
    let album_row = Album::find_by_id(id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Album not found".to_string()))?;
    if album_row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
            "Only the author or an administrator can add to this album".to_string(),
        ));
    }

    let existing_max_ordinal: i32 = AlbumMedia::find()
        .filter(album_media::Column::AlbumId.eq(id))
        .order_by_desc(album_media::Column::Ordinal)
        .one(&state.db)
        .await?
        .map(|m| m.ordinal)
        .unwrap_or(-1);

    let mut media: Vec<PendingMedia> = Vec::new();
    let mut captions_by_index: HashMap<usize, String> = HashMap::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to parse multipart form: {}", e)))?
    {
        let n = field.name().unwrap_or("").to_string();
        if n == "media" {
            let mime = field.content_type().map(|s| s.to_string()).ok_or_else(|| {
                AppError::ValidationError("Media field must include a Content-Type".to_string())
            })?;
            if !crate::posts::routes::is_allowed_media_mime(&mime) {
                return Err(AppError::ValidationError(format!(
                    "Unsupported media type '{}'",
                    mime
                )));
            }
            let bytes = field.bytes().await.map_err(|e| {
                AppError::ValidationError(format!("Failed to read media bytes: {}", e))
            })?;
            let cap = crate::posts::routes::max_bytes_for_mime(&mime);
            if bytes.len() > cap {
                return Err(AppError::ValidationError(format!(
                    "Media file ({}) exceeds {} byte limit",
                    bytes.len(),
                    cap
                )));
            }
            media.push(PendingMedia {
                bytes: bytes.to_vec(),
                mime_type: mime,
                caption: None,
            });
        } else if n.starts_with("caption_") {
            if let Ok(idx) = n.trim_start_matches("caption_").parse::<usize>() {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    captions_by_index.insert(idx, text);
                }
            }
        }
    }
    for (idx, caption) in captions_by_index {
        if let Some(m) = media.get_mut(idx) {
            m.caption = Some(caption);
        }
    }

    if media.is_empty() {
        return Err(AppError::ValidationError(
            "Append requires at least one media file".to_string(),
        ));
    }

    let media_plan: Vec<(Uuid, String, PendingMedia, i32)> = media
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let media_id = Uuid::new_v4();
            let key = format!("albums/{}/{}", id, media_id);
            (media_id, key, m, existing_max_ordinal + 1 + i as i32)
        })
        .collect();

    let mut uploaded_keys: Vec<String> = Vec::new();
    for (_id, key, m, _o) in &media_plan {
        if let Err(e) = state
            .s3
            .put_object_at(key, m.bytes.clone(), &m.mime_type)
            .await
        {
            for k in &uploaded_keys {
                let _ = state.s3.delete_object_at(k).await;
            }
            return Err(AppError::AuthError(format!(
                "Failed to upload media: {}",
                e
            )));
        }
        uploaded_keys.push(key.clone());
    }

    let txn_result = async {
        let txn = state.db.begin().await?;
        for (media_id, key, m, ordinal) in &media_plan {
            album_media::ActiveModel {
                id: Set(*media_id),
                album_id: Set(id),
                s3_key: Set(key.clone()),
                mime_type: Set(m.mime_type.clone()),
                width: Set(None),
                height: Set(None),
                ordinal: Set(*ordinal),
                caption: Set(m.caption.clone()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
        }
        let mut updated_album = album_row.clone();
        if updated_album.cover_media_id.is_none() {
            if let Some((id_val, _, _, _)) = media_plan.first() {
                let mut active: album::ActiveModel = updated_album.into();
                active.cover_media_id = Set(Some(*id_val));
                let saved = active.update(&txn).await?;
                updated_album = saved;
            }
        }
        txn.commit().await?;
        Ok::<album::Model, sea_orm::DbErr>(updated_album)
    }
    .await;

    let updated_album = match txn_result {
        Ok(row) => row,
        Err(e) => {
            for k in &uploaded_keys {
                let _ = state.s3.delete_object_at(k).await;
            }
            return Err(AppError::AuthError(format!(
                "Failed to append media: {}",
                e
            )));
        }
    };

    let media = AlbumMedia::find()
        .filter(album_media::Column::AlbumId.eq(id))
        .order_by_asc(album_media::Column::Ordinal)
        .all(&state.db)
        .await?;
    let author = User::find_by_id(updated_album.author_id)
        .one(&state.db)
        .await?;
    let cat = load_album_category(&state.db, &updated_album).await?;
    Ok(Json(build_album_response(
        updated_album,
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
    let album_row = Album::find_by_id(album_id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Album not found".to_string()))?;
    if album_row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
            "Only the author or an administrator can edit this album".to_string(),
        ));
    }
    let media = AlbumMedia::find_by_id(media_id)
        .filter(album_media::Column::AlbumId.eq(album_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Media not found".to_string()))?;

    let mut active: album_media::ActiveModel = media.into();
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
    Ok(Json(AlbumMediaResponse::from_model(updated)))
}

async fn delete_album_media(
    State(state): State<AlbumsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((album_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let album_row = Album::find_by_id(album_id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Album not found".to_string()))?;
    if album_row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
            "Only the author or an administrator can delete from this album".to_string(),
        ));
    }
    let media = AlbumMedia::find_by_id(media_id)
        .filter(album_media::Column::AlbumId.eq(album_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Media not found".to_string()))?;
    let s3_key = media.s3_key.clone();
    let was_cover = album_row.cover_media_id == Some(media.id);
    let media_id_val = media.id;

    let row_to_delete: album_media::ActiveModel = media.into();
    row_to_delete.delete(&state.db).await?;
    let _ = state.s3.delete_object_at(&s3_key).await;

    if was_cover {
        let next = AlbumMedia::find()
            .filter(album_media::Column::AlbumId.eq(album_id))
            .order_by_asc(album_media::Column::Ordinal)
            .one(&state.db)
            .await?;
        let mut active: album::ActiveModel = album_row.into();
        active.cover_media_id = Set(next.map(|m| m.id));
        active.update(&state.db).await?;
    }
    let _ = media_id_val;

    Ok(StatusCode::NO_CONTENT)
}

async fn serve_album_media(
    State(state): State<AlbumsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(media_id): Path<Uuid>,
) -> AppResult<axum::response::Response> {
    use axum::body::Body;
    use axum::http::header;
    use axum::response::IntoResponse;

    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier)
        .await
        .map_err(|_| AppError::AuthError("Media not found".to_string()))?;

    let media = AlbumMedia::find_by_id(media_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Media not found".to_string()))?;
    let parent = Album::find_by_id(media.album_id)
        .filter(album::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Media not found".to_string()))?;
    let parent_cat = load_album_category(&state.db, &parent).await?;
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.can_view_album(&parent, parent_cat.as_ref()) {
        return Err(AppError::AuthError("Media not found".to_string()));
    }

    let (bytes, stored_type) = state
        .s3
        .get_object_at(&media.s3_key)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to load media: {}", e)))?;

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
    Err(AppError::AuthError("Not found".to_string()))
}

fn parse_cursor(cursor: &str) -> Option<(chrono::DateTime<chrono::FixedOffset>, Uuid)> {
    let (ts, id) = cursor.rsplit_once('_')?;
    let parsed_ts = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let parsed_id = Uuid::parse_str(id).ok()?;
    Some((parsed_ts, parsed_id))
}

#[allow(dead_code)]
type _Marker = AuthenticatedUser;
