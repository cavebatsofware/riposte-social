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

use crate::entities::{post, post_media, user, Post, PostMedia, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::{markdown, FeedTier};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use axum_login::AuthSession;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostsState {
    pub db: DatabaseConnection,
}

/// Authenticated, role-gated write endpoints. The author/admin check on
/// PATCH/DELETE happens inside the handler.
pub fn post_write_routes() -> Router<PostsState> {
    Router::new()
        .route("/api/posts", post(create_post))
        .route(
            "/api/posts/{id}",
            axum::routing::patch(update_post).delete(delete_post),
        )
}

/// Public read endpoints. Caller's tier (anonymous / commenter / privileged)
/// is read from the optional auth session and used to filter visibility.
pub fn post_read_routes() -> Router<PostsState> {
    Router::new()
        .route("/api/posts/{id}", get(get_post))
        .route("/api/feed", get(feed))
}

// ==================== Wire format ====================

#[derive(Deserialize)]
pub struct CreatePostRequest {
    pub body: String,
    /// One of `public`, `commenters`, `posters`. Defaults to `public` when
    /// omitted so the simplest "Compose" form has a sensible default.
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// Optional override for the publish time (used by importers). Live
    /// authoring leaves this unset and inherits `now()`.
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
}

fn default_visibility() -> String {
    post::VISIBILITY_PUBLIC.to_string()
}

#[derive(Deserialize)]
pub struct UpdatePostRequest {
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Serialize)]
pub struct PostResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_display: Option<String>,
    pub author_email: Option<String>,
    pub body: String,
    pub body_html: String,
    pub visibility: String,
    pub published_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub media: Vec<PostMediaResponse>,
}

#[derive(Serialize)]
pub struct PostMediaResponse {
    pub id: Uuid,
    pub s3_key: String,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ordinal: i32,
    pub caption: Option<String>,
}

impl PostMediaResponse {
    fn from_model(m: post_media::Model) -> Self {
        Self {
            id: m.id,
            s3_key: m.s3_key,
            mime_type: m.mime_type,
            width: m.width,
            height: m.height,
            ordinal: m.ordinal,
            caption: m.caption,
        }
    }
}

fn build_post_response(
    row: post::Model,
    author: Option<&user::Model>,
    media: Vec<post_media::Model>,
) -> PostResponse {
    let body_html = markdown::render_to_html(&row.body);
    PostResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_email: author.map(|u| u.email.clone()),
        body: row.body,
        body_html,
        visibility: row.visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        media: media.into_iter().map(PostMediaResponse::from_model).collect(),
    }
}

// ==================== Handlers ====================

/// `POST /api/posts`. Create a post. Gated by `require_admin_or_poster` at
/// the route layer. Body is plain JSON for now; multipart media upload
/// lands in Phase 3c.
async fn create_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Json(req): Json<CreatePostRequest>,
) -> AppResult<(StatusCode, Json<PostResponse>)> {
    if req.body.trim().is_empty() {
        return Err(AppError::ValidationError(
            "Body cannot be empty".to_string(),
        ));
    }
    if !post::is_valid_visibility(&req.visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            req.visibility
        )));
    }

    let now = Utc::now();
    let active = post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(user.id),
        body: Set(req.body),
        visibility: Set(req.visibility),
        published_at: Set(req.published_at.unwrap_or(now).into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(None),
        ..Default::default()
    };
    let inserted = active.insert(&state.db).await?;

    let author = User::find_by_id(inserted.author_id)
        .one(&state.db)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(build_post_response(inserted, author.as_ref(), vec![])),
    ))
}

/// `GET /api/posts/{id}`. Single post, visibility-filtered for the caller.
async fn get_post(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostResponse>> {
    let row = Post::find_by_id(id)
        .filter(post::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Post not found".to_string()))?;

    let tier = caller_tier(&auth_session).await;
    if !tier.can_read(&row.visibility) {
        // Don't disclose existence to under-tier callers. Same error as
        // missing post.
        return Err(AppError::AuthError("Post not found".to_string()));
    }

    let media = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(&state.db)
        .await?;

    let author = User::find_by_id(row.author_id).one(&state.db).await?;
    Ok(Json(build_post_response(row, author.as_ref(), media)))
}

/// `PATCH /api/posts/{id}`. Edit. Author or administrator only.
async fn update_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePostRequest>,
) -> AppResult<Json<PostResponse>> {
    let row = Post::find_by_id(id)
        .filter(post::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Post not found".to_string()))?;

    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
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

    let mut active: post::ActiveModel = row.into();
    if let Some(body) = req.body {
        active.body = Set(body);
    }
    if let Some(v) = req.visibility {
        active.visibility = Set(v);
    }
    let updated = active.update(&state.db).await?;

    let media = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(&state.db)
        .await?;
    let author = User::find_by_id(updated.author_id).one(&state.db).await?;
    Ok(Json(build_post_response(updated, author.as_ref(), media)))
}

/// `DELETE /api/posts/{id}`. Soft delete (sets `deleted_at`). Author or
/// administrator. The post stops appearing in the feed and `GET /api/posts/{id}`
/// returns 404 thereafter; physical removal of media is a Phase 6 concern.
async fn delete_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = Post::find_by_id(id)
        .filter(post::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Post not found".to_string()))?;

    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::AuthError(
            "Only the author or an administrator can delete this post".to_string(),
        ));
    }

    let mut active: post::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct FeedQuery {
    /// Cursor in the form `{published_at_rfc3339}_{post_id}`. Returned in
    /// the previous page's `next_cursor`. Omitted on first page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size. Capped server-side.
    #[serde(default)]
    pub limit: Option<u64>,
}

#[derive(Serialize)]
pub struct FeedResponse {
    pub posts: Vec<PostResponse>,
    pub next_cursor: Option<String>,
}

const FEED_LIMIT_DEFAULT: u64 = 20;
const FEED_LIMIT_MAX: u64 = 100;

/// `GET /api/feed`. Cursor-paginated feed scoped to the caller's tier.
/// Anonymous visitors get public posts only; commenters get public +
/// commenter-visible; posters and admins see everything.
///
/// Cursor format is `{published_at_rfc3339}_{post_id}`, sorted descending
/// by `(published_at, id)` so ties don't lose rows. Cursors that fail to
/// parse fall back to a fresh first page.
async fn feed(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Query(query): Query<FeedQuery>,
) -> AppResult<Json<FeedResponse>> {
    let tier = caller_tier(&auth_session).await;
    let limit = query
        .limit
        .unwrap_or(FEED_LIMIT_DEFAULT)
        .min(FEED_LIMIT_MAX)
        .max(1);

    let allowed: Vec<&'static str> = tier.allowed_visibilities().to_vec();

    let mut q = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Visibility.is_in(allowed))
        .order_by_desc(post::Column::PublishedAt)
        .order_by_desc(post::Column::Id);

    if let Some(cursor) = query.cursor.as_deref().and_then(parse_cursor) {
        let (cursor_published_at, cursor_id) = cursor;
        q = q.filter(
            sea_orm::Condition::any()
                .add(post::Column::PublishedAt.lt(cursor_published_at))
                .add(
                    sea_orm::Condition::all()
                        .add(post::Column::PublishedAt.eq(cursor_published_at))
                        .add(post::Column::Id.lt(cursor_id)),
                ),
        );
    }

    // Fetch limit+1 to detect whether more pages exist without a count query.
    let rows = q
        .limit(limit + 1)
        .all(&state.db)
        .await?;

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

    // Fetch authors and media in a batch each to avoid N+1.
    let author_ids: Vec<Uuid> = page.iter().map(|p| p.author_id).collect();
    let post_ids: Vec<Uuid> = page.iter().map(|p| p.id).collect();

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

    let media_rows = if post_ids.is_empty() {
        vec![]
    } else {
        PostMedia::find()
            .filter(post_media::Column::PostId.is_in(post_ids))
            .order_by_asc(post_media::Column::Ordinal)
            .all(&state.db)
            .await?
    };
    let mut media_by_post: HashMap<Uuid, Vec<post_media::Model>> = HashMap::new();
    for m in media_rows {
        media_by_post.entry(m.post_id).or_default().push(m);
    }

    let posts: Vec<PostResponse> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            let media = media_by_post.remove(&row.id).unwrap_or_default();
            build_post_response(row, author, media)
        })
        .collect();

    Ok(Json(FeedResponse { posts, next_cursor }))
}

// ==================== Helpers ====================

async fn caller_tier(
    auth_session: &AuthSession<crate::admin::UserAuthBackend>,
) -> FeedTier {
    let user = auth_session.user().await;
    FeedTier::from_role(user.as_ref().map(|u| u.role.as_str()))
}

fn parse_cursor(cursor: &str) -> Option<(chrono::DateTime<chrono::FixedOffset>, Uuid)> {
    let (ts, id) = cursor.rsplit_once('_')?;
    let parsed_ts = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let parsed_id = Uuid::parse_str(id).ok()?;
    Some((parsed_ts, parsed_id))
}

/// Extractor used by routes that need an authenticated principal even though
/// they don't apply `require_admin_or_poster` (e.g. read endpoints that
/// adjust visibility based on the caller's tier).
#[allow(dead_code)]
async fn optional_auth(
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
) -> Option<crate::admin::UserAuth> {
    auth_session.user().await
}

/// Marker so unused `AuthenticatedUser` import doesn't trigger a warning
/// when only some routes consume it.
#[allow(dead_code)]
type _Marker = AuthenticatedUser;
