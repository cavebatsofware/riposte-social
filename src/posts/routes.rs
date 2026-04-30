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
use crate::entities::{post, post_media, user, Post, PostMedia, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::{markdown, FeedTier};
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
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostsState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
}

/// Per-file size cap (10 MiB). Beyond this we reject the upload before
/// hitting S3.
const MEDIA_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Total request size cap. Multipart bodies are bounded so a malformed
/// client cannot wedge the connection holding gigabytes in memory.
const POST_BODY_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Maximum media files attached to a single post. Hard cap so the client
/// cannot stash unbounded references that we then have to track + clean up.
const MEDIA_FILES_MAX: usize = 8;

/// Allowlisted image mime types. Browsers render these inline; everything
/// else is rejected so uploaded files cannot become a vector for malicious
/// content disguised as media (`text/html` payloads, SVG with embedded
/// scripts, etc.).
const ALLOWED_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
];

/// Authenticated, role-gated write endpoints. The author/admin check on
/// PATCH/DELETE happens inside the handler. Multipart body limit is lifted
/// to `POST_BODY_MAX_BYTES` so a small handful of image attachments fit.
pub fn post_write_routes() -> Router<PostsState> {
    Router::new()
        .route(
            "/api/posts",
            post(create_post).layer(DefaultBodyLimit::max(POST_BODY_MAX_BYTES)),
        )
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
        .route("/media/{media_id}", get(serve_media))
}

// ==================== Wire format ====================

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
    /// Author's chosen display name, when set. Email is intentionally not
    /// exposed: post payloads are visible to every reader of the post and
    /// public posts are also visible to anonymous visitors. Clients fall
    /// back to a generic label when display_name is absent.
    pub author_display: Option<String>,
    pub body: String,
    pub body_html: String,
    pub visibility: String,
    pub published_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub media: Vec<PostMediaResponse>,
    /// Per-kind reaction counts. Kinds with zero count are omitted from
    /// the map.
    pub reaction_counts: HashMap<String, i64>,
    /// Reaction kinds the viewing caller has applied. Empty for anonymous
    /// callers and for callers who haven't reacted to this post.
    pub viewer_reaction_kinds: Vec<String>,
    /// Live (non-soft-deleted) comment count.
    pub comment_count: i64,
}

#[derive(Serialize)]
pub struct PostMediaResponse {
    pub id: Uuid,
    /// Browser-facing URL. Hits `/media/{media_id}` which checks tier
    /// visibility before serving from S3.
    pub url: String,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ordinal: i32,
    pub caption: Option<String>,
}

impl PostMediaResponse {
    fn from_model(m: post_media::Model) -> Self {
        Self {
            url: format!("/media/{}", m.id),
            id: m.id,
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
    engagement: PostEngagement,
) -> PostResponse {
    let body_html = markdown::render_to_html(&row.body);
    PostResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        body: row.body,
        body_html,
        visibility: row.visibility,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        media: media.into_iter().map(PostMediaResponse::from_model).collect(),
        reaction_counts: engagement.reaction_counts,
        viewer_reaction_kinds: engagement.viewer_reaction_kinds,
        comment_count: engagement.comment_count,
    }
}

// ==================== Handlers ====================

/// Buffered media upload: bytes plus the mime type the browser supplied.
struct PendingMedia {
    bytes: Vec<u8>,
    mime_type: String,
}

/// `POST /api/posts`. Create a post via multipart upload. Gated by
/// `require_admin_or_poster` at the route layer.
///
/// Multipart shape:
/// - `body` (text, required): markdown source.
/// - `visibility` (text, optional): one of public|commenters|posters.
///   Defaults to public.
/// - `published_at` (text, optional): RFC3339 timestamp. Live authoring
///   omits this. Importers set it to preserve original ordering.
/// - `media` (file, 0 or more): image attachments. Each must be in
///   `ALLOWED_MIME_TYPES` and below `MEDIA_FILE_MAX_BYTES`. Capped at
///   `MEDIA_FILES_MAX` per post. Order in the request becomes the
///   `ordinal` for the created post_media rows.
///
/// The post row + post_media rows + S3 uploads happen behind a
/// transaction: any failure rolls back the DB rows. S3 uploads are best-
/// effort to revert (we issue deletes after a rollback) so a failed
/// transaction doesn't leave orphan objects.
async fn create_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<PostResponse>)> {
    let mut body: Option<String> = None;
    let mut visibility: String = post::VISIBILITY_PUBLIC.to_string();
    let mut published_at: Option<DateTime<Utc>> = None;
    let mut media: Vec<PendingMedia> = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::ValidationError(format!("Failed to parse multipart form: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "body" => {
                body = Some(field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read body: {}", e))
                })?);
            }
            "visibility" => {
                visibility = field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read visibility: {}", e))
                })?;
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
                        "At most {} media files per post",
                        MEDIA_FILES_MAX
                    )));
                }
                let mime = field
                    .content_type()
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        AppError::ValidationError(
                            "Media field must include a Content-Type".to_string(),
                        )
                    })?;
                if !ALLOWED_MIME_TYPES.contains(&mime.as_str()) {
                    return Err(AppError::ValidationError(format!(
                        "Unsupported media type '{}'. Allowed: {:?}",
                        mime, ALLOWED_MIME_TYPES
                    )));
                }
                let bytes = field.bytes().await.map_err(|e| {
                    AppError::ValidationError(format!("Failed to read media bytes: {}", e))
                })?;
                if bytes.len() > MEDIA_FILE_MAX_BYTES {
                    return Err(AppError::ValidationError(format!(
                        "Media file exceeds {} byte limit",
                        MEDIA_FILE_MAX_BYTES
                    )));
                }
                media.push(PendingMedia {
                    bytes: bytes.to_vec(),
                    mime_type: mime,
                });
            }
            _ => {
                // Ignore unknown fields rather than 400-ing; lets clients
                // add forward-compatible fields without breaking us.
            }
        }
    }

    let body = body.ok_or_else(|| {
        AppError::ValidationError("Missing required field: body".to_string())
    })?;
    if body.trim().is_empty() {
        return Err(AppError::ValidationError(
            "Body cannot be empty".to_string(),
        ));
    }
    if !post::is_valid_visibility(&visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            visibility
        )));
    }

    let post_id = Uuid::new_v4();
    let now = Utc::now();

    // Pre-generate IDs and S3 keys so the upload + DB insert can pair up
    // and we can clean up uploads if the DB transaction fails.
    let media_plan: Vec<(Uuid, String, PendingMedia, i32)> = media
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let media_id = Uuid::new_v4();
            let key = format!("posts/{}/{}", post_id, media_id);
            (media_id, key, m, i as i32)
        })
        .collect();

    // Upload first so a failure surfaces before any DB write. Track each
    // successful key for rollback compensation.
    let mut uploaded_keys: Vec<String> = Vec::new();
    for (_id, key, m, _ordinal) in &media_plan {
        if let Err(e) = state
            .s3
            .put_object_at(key, m.bytes.clone(), &m.mime_type)
            .await
        {
            // Undo prior uploads in this request.
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

    // DB writes run in a transaction so the post + media rows commit or
    // roll back together. On rollback we delete the S3 objects we just
    // uploaded.
    let txn_result = async {
        let txn = state.db.begin().await?;
        let post_row = post::ActiveModel {
            id: Set(post_id),
            author_id: Set(user.id),
            body: Set(body),
            visibility: Set(visibility),
            published_at: Set(published_at.unwrap_or(now).into()),
            import_source: Set(None),
            import_external_id: Set(None),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        let mut media_rows: Vec<post_media::Model> = Vec::with_capacity(media_plan.len());
        for (media_id, key, m, ordinal) in &media_plan {
            let row = post_media::ActiveModel {
                id: Set(*media_id),
                post_id: Set(post_id),
                s3_key: Set(key.clone()),
                mime_type: Set(m.mime_type.clone()),
                width: Set(None),
                height: Set(None),
                ordinal: Set(*ordinal),
                caption: Set(None),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
            media_rows.push(row);
        }

        txn.commit().await?;
        Ok::<(post::Model, Vec<post_media::Model>), sea_orm::DbErr>((post_row, media_rows))
    }
    .await;

    let (post_row, media_rows) = match txn_result {
        Ok(pair) => pair,
        Err(e) => {
            for k in &uploaded_keys {
                let _ = state.s3.delete_object_at(k).await;
            }
            return Err(AppError::AuthError(format!(
                "Failed to create post: {}",
                e
            )));
        }
    };

    let author = User::find_by_id(post_row.author_id).one(&state.db).await?;
    // Newly created post: no reactions or comments yet, so engagement is
    // the default zero state. Skip the lookup.
    Ok((
        StatusCode::CREATED,
        Json(build_post_response(
            post_row,
            author.as_ref(),
            media_rows,
            PostEngagement::default(),
        )),
    ))
}

/// `GET /media/{media_id}`. Serve a media file from S3, gated by the
/// caller's visibility tier against the parent post. Returns 404 (not 403)
/// when the caller cannot read the parent so existence isn't disclosed
/// to under-tier visitors. Cache headers are set conservatively because
/// admins can soft-delete the parent post.
async fn serve_media(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(media_id): Path<Uuid>,
) -> AppResult<Response> {
    let media = PostMedia::find_by_id(media_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Media not found".to_string()))?;

    let parent = Post::find_by_id(media.post_id)
        .filter(post::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Media not found".to_string()))?;

    let tier = caller_tier(&auth_session).await;
    if !tier.can_read(&parent.visibility) {
        return Err(AppError::AuthError("Media not found".to_string()));
    }

    let (bytes, stored_type) = state
        .s3
        .get_object_at(&media.s3_key)
        .await
        .map_err(|e| AppError::AuthError(format!("Failed to load media: {}", e)))?;

    // Public posts go to a CDN-friendly cache; gated tiers stay private so
    // a shared cache can't leak commenter/poster content to anonymous
    // visitors. Both still get a long max-age because media is keyed by an
    // unguessable UUID and only ever changes via post deletion.
    let cache_control = if parent.visibility == post::VISIBILITY_PUBLIC {
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
    let viewer_id = auth_session.user().await.map(|u| u.id);
    let mut engagement_map =
        fetch_engagement_for_posts(&state.db, &[row.id], viewer_id).await?;
    let engagement = engagement_map.remove(&row.id).unwrap_or_default();
    Ok(Json(build_post_response(
        row,
        author.as_ref(),
        media,
        engagement,
    )))
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
    let mut engagement_map =
        fetch_engagement_for_posts(&state.db, &[updated.id], Some(user.id)).await?;
    let engagement = engagement_map.remove(&updated.id).unwrap_or_default();
    Ok(Json(build_post_response(
        updated,
        author.as_ref(),
        media,
        engagement,
    )))
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

    // Engagement is fetched in batch keyed by post_id. Anonymous callers
    // get reaction counts and comment counts but no `viewer_reaction_kinds`.
    let viewer_id = auth_session.user().await.map(|u| u.id);
    let post_ids_for_engagement: Vec<Uuid> = page.iter().map(|p| p.id).collect();
    let mut engagement_by_post =
        fetch_engagement_for_posts(&state.db, &post_ids_for_engagement, viewer_id).await?;

    let posts: Vec<PostResponse> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            let media = media_by_post.remove(&row.id).unwrap_or_default();
            let engagement = engagement_by_post.remove(&row.id).unwrap_or_default();
            build_post_response(row, author, media, engagement)
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
