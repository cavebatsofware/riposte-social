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

use crate::engagement::comments::{build_comment_response, CommentResponse};
use crate::engagement::{fetch_engagement_for_posts, PostEngagement};
use crate::entities::{category, post, post_media, user, Category, Post, PostMedia, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::{markdown, shared, FeedTier};
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
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostsState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: crate::settings::SettingsService,
}

/// Per-image-file size cap (10 MiB). Phone-shot photos comfortably fit;
/// edited / RAW exports get rejected before hitting S3.
const IMAGE_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Per-video-file size cap (100 MiB). Family video clips are typically
/// short; transcoded HEVC is even smaller. Anything beyond this is more
/// than the browser-inline player path is meant to serve.
const VIDEO_FILE_MAX_BYTES: usize = 100 * 1024 * 1024;

/// Allowlisted image mime types. Browsers render these inline; everything
/// else is rejected so uploaded files cannot become a vector for malicious
/// content disguised as media (`text/html` payloads, SVG with embedded
/// scripts, etc.).
const ALLOWED_IMAGE_MIME_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Allowlisted video mime types. Constrained to formats every modern
/// browser plays inline with `<video controls>`. No flash, no hls, no
/// formats requiring transcoding on the server side.
const ALLOWED_VIDEO_MIME_TYPES: &[&str] = &["video/mp4", "video/webm"];

/// Returns true when `mime` is on either the image or the video allowlist.
/// Re-exported (pub) so the albums module can reuse the same allowlist
/// without drifting.
pub fn is_allowed_media_mime(mime: &str) -> bool {
    ALLOWED_IMAGE_MIME_TYPES.contains(&mime) || ALLOWED_VIDEO_MIME_TYPES.contains(&mime)
}

/// Returns true when `mime` is a video mime — used to pick the per-file
/// size cap and to dispatch the frontend `<video>` vs `<img>` render.
pub fn is_video_mime(mime: &str) -> bool {
    ALLOWED_VIDEO_MIME_TYPES.contains(&mime)
}

/// Per-file cap depends on the mime: images stay at 10 MiB, videos go up
/// to 100 MiB. Picking the cap by mime keeps a fat finger from sneaking
/// a 100 MiB image past the cheaper image cap.
pub fn max_bytes_for_mime(mime: &str) -> usize {
    if is_video_mime(mime) {
        VIDEO_FILE_MAX_BYTES
    } else {
        IMAGE_FILE_MAX_BYTES
    }
}

/// Authenticated, role-gated write endpoints. The author/admin check on
/// PATCH/DELETE happens inside the handler (or inside the shared media
/// helpers). Multipart body limit is lifted to `COMPOSE_BODY_MAX_BYTES`
/// so a small handful of image or video attachments fit in one request.
pub fn post_write_routes() -> Router<PostsState> {
    Router::new()
        .route(
            "/api/posts",
            post(create_post).layer(DefaultBodyLimit::max(shared::COMPOSE_BODY_MAX_BYTES)),
        )
        .route(
            "/api/posts/{id}",
            axum::routing::patch(update_post).delete(delete_post),
        )
        .route(
            "/api/posts/{id}/media",
            post(append_post_media).layer(DefaultBodyLimit::max(shared::COMPOSE_BODY_MAX_BYTES)),
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
pub struct UpdatePostMediaRequest {
    /// Empty/whitespace clears the caption (NULL); a present value
    /// replaces it. Omitted means "leave alone" (today the only field
    /// that can be edited is caption, so the omit-vs-clear distinction
    /// only matters for `caption`).
    #[serde(default)]
    pub caption: Option<String>,
}

#[derive(Deserialize)]
pub struct MediaOrderItem {
    pub id: Uuid,
    pub ordinal: i32,
}

#[derive(Deserialize)]
pub struct UpdatePostRequest {
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    /// Phase 9e: assign / move to a different category. Send `null` to
    /// clear (mark uncategorized); omit to leave unchanged. JSON
    /// distinguishes `{}` (omitted) from `{"category_id": null}` (clear)
    /// only with a sentinel; we use a double-Option via serde_with's
    /// `default + skip_serializing_if = is_none` pattern, but for the v1
    /// the simpler-but-good-enough rule is: provide a UUID to set; omit
    /// to leave alone. To clear, edit through the admin UI or PATCH a
    /// distinct sentinel value (handled at the handler).
    #[serde(default)]
    pub category_id: Option<Uuid>,
    /// Explicit boolean: set to true to clear the post's category.
    /// Mirrors the `category_id` field — omitted = no change, true =
    /// clear, false = no change. Pairs with `category_id` so a single
    /// payload can either set or clear without serde-with gymnastics.
    #[serde(default)]
    pub clear_category: bool,
    /// FTS configuration name. Validated against the allowlist when
    /// present. Omitting leaves the existing value alone.
    #[serde(default)]
    pub content_lang: Option<String>,
}

#[derive(Serialize)]
pub struct PostCategoryRef {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub color: Option<String>,
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
    /// Author's public handle. Used by the social-frontend to link the
    /// avatar/byline to `/u/{handle}`.
    pub author_handle: Option<String>,
    /// Author's avatar URL, derived in `profile::avatar_url_for`. Falls
    /// back to None when the author has no uploaded avatar.
    pub author_avatar_url: Option<String>,
    pub body: String,
    pub body_html: String,
    /// Visibility stored on the post row. For categorized posts this is
    /// preserved-but-ignored — the category drives access; see
    /// `effective_visibility` for the value clients should render.
    pub visibility: String,
    /// Visibility actually enforced for this post: when the post has a
    /// category, equals the category's visibility; otherwise equals the
    /// post's own `visibility`. Always populated.
    pub effective_visibility: String,
    pub content_lang: String,
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
    /// The post's category, or null when uncategorized. Includes slug +
    /// color so PostCard can render the chip without a second fetch.
    pub category: Option<PostCategoryRef>,
    /// Up to three most-recent live comments on this post, newest-first.
    /// Used by the feed PostCard to surface inline conversation context;
    /// the permalink page renders the full thread separately and can
    /// ignore this field.
    pub top_comments: Vec<CommentResponse>,
}

#[derive(Serialize)]
pub struct PostMediaResponse {
    pub id: Uuid,
    /// Browser-facing URL. Hits `/media/{media_id}` which checks tier
    /// visibility before serving from S3.
    pub url: String,
    pub mime_type: String,
    /// Coarse kind derived from `mime_type`: `"image"` or `"video"`.
    /// Lets the client render `<img>` vs `<video>` without parsing mime
    /// strings on the client side.
    pub media_kind: &'static str,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ordinal: i32,
    pub caption: Option<String>,
}

impl PostMediaResponse {
    fn from_model(m: post_media::Model) -> Self {
        let media_kind = if is_video_mime(&m.mime_type) {
            "video"
        } else {
            "image"
        };
        Self {
            url: format!("/media/{}", m.id),
            id: m.id,
            mime_type: m.mime_type,
            media_kind,
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
    category: Option<&category::Model>,
    top_comment_authors: &HashMap<Uuid, user::Model>,
) -> PostResponse {
    let body_html = markdown::render_to_html(&row.body);
    let category_ref = category.map(|c| PostCategoryRef {
        id: c.id,
        slug: c.slug.clone(),
        name: c.name.clone(),
        color: c.color.clone(),
    });
    let top_comments = engagement
        .top_comments
        .into_iter()
        .map(|c| {
            let cauthor = top_comment_authors.get(&c.user_id);
            build_comment_response(c, cauthor)
        })
        .collect();
    let effective_visibility = category
        .map(|c| c.visibility.clone())
        .unwrap_or_else(|| row.visibility.clone());
    PostResponse {
        id: row.id,
        author_id: row.author_id,
        author_display: author.and_then(|u| u.display_name.clone()),
        author_handle: author.map(|u| u.handle.clone()),
        author_avatar_url: author.and_then(crate::profile::avatar_url_for),
        body: row.body,
        body_html,
        visibility: row.visibility,
        effective_visibility,
        content_lang: row.content_lang,
        published_at: row.published_at.with_timezone(&Utc).to_rfc3339(),
        created_at: row.created_at.with_timezone(&Utc).to_rfc3339(),
        updated_at: row.updated_at.with_timezone(&Utc).to_rfc3339(),
        media: media
            .into_iter()
            .map(PostMediaResponse::from_model)
            .collect(),
        reaction_counts: engagement.reaction_counts,
        viewer_reaction_kinds: engagement.viewer_reaction_kinds,
        comment_count: engagement.comment_count,
        category: category_ref,
        top_comments,
    }
}

// ==================== Handlers ====================

/// `POST /api/posts`. Create a post via multipart upload. Gated by
/// `require_admin_or_poster` at the route layer.
///
/// Multipart shape:
/// - `body` (text, required): markdown source.
/// - `slug` (text, optional): short title. Capped at `SLUG_MAX_LEN` chars.
/// - `visibility` (text, optional): one of public|commenters|posters|private.
///   Defaults to private (draft-leak protection — author has to opt in to
///   public visibility).
/// - `category_id` (text, optional): UUID. Caller must be allowed to
///   compose into that category.
/// - `content_lang` (text, optional): FTS configuration. Defaults to
///   `english`.
/// - `media` (file, 0 or more): image or video attachments. Format-level
///   validation (mime allowlist, per-file size cap, count cap) happens in
///   `shared::parse_compose_multipart`.
///
/// Compose flow lives in `shared::commit_compose`: media upload happens
/// before the DB transaction so a failure can't leave half-written state,
/// and S3 objects are deleted on transaction rollback.
async fn create_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<PostResponse>)> {
    let input = shared::parse_compose_multipart(&mut multipart, post::KIND_POST).await?;
    let (post_row, media_rows) =
        shared::commit_compose(&state.db, &state.s3, &state.settings, &user, input).await?;

    crate::metrics::POSTS_CREATED_TOTAL.inc();
    let author = User::find_by_id(post_row.author_id).one(&state.db).await?;
    let cat = shared::load_category(&state.db, post_row.category_id).await?;
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

/// `POST /api/posts/{id}/media`. Append media to an existing post.
/// Author / admin only. The kind is locked to `KIND_POST`; an album id
/// here returns 404 like any other not-found post.
async fn append_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<Vec<PostMediaResponse>>> {
    let rows = shared::append_media(
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

/// `PATCH /api/posts/{id}/media/{media_id}`. Edit a media item's caption.
/// Empty/whitespace caption clears the field.
async fn update_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdatePostMediaRequest>,
) -> AppResult<Json<PostMediaResponse>> {
    let row = shared::edit_media_caption(
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

/// `DELETE /api/posts/{id}/media/{media_id}`. Remove one media item from
/// a post. Author / admin only.
async fn delete_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path((post_id, media_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    shared::delete_media(
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

/// `PUT /api/posts/{id}/media/order`. Reassign every media item's
/// ordinal in one transaction. Body shape: `[{"id": uuid, "ordinal": i32}, ...]`.
async fn reorder_post_media(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(post_id): Path<Uuid>,
    Json(items): Json<Vec<MediaOrderItem>>,
) -> AppResult<StatusCode> {
    let ordinals: Vec<(Uuid, i32)> = items.into_iter().map(|i| (i.id, i.ordinal)).collect();
    shared::reorder_media(&state.db, &user, post_id, post::KIND_POST, ordinals).await?;
    Ok(StatusCode::NO_CONTENT)
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

    let parent_cat = if let Some(cid) = parent.category_id {
        Category::find_by_id(cid).one(&state.db).await?
    } else {
        None
    };
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.can_view_post(&parent, parent_cat.as_ref()) {
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

/// `GET /api/posts/{id}`. Single post, visibility-filtered for the caller.
async fn get_post(
    State(state): State<PostsState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PostResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;

    let row = shared::load_post(&state.db, id, post::KIND_POST).await?;

    let parent_cat = if let Some(cid) = row.category_id {
        Category::find_by_id(cid).one(&state.db).await?
    } else {
        None
    };
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    let viewer_id = ctx.viewer_id;
    if !ctx.can_view_post(&row, parent_cat.as_ref()) {
        // Don't disclose existence to under-tier callers. Same error as
        // missing post.
        return Err(AppError::NotFound("Post not found".to_string()));
    }

    let media = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(&state.db)
        .await?;

    let author = User::find_by_id(row.author_id).one(&state.db).await?;
    let mut engagement_map = fetch_engagement_for_posts(&state.db, &[row.id], viewer_id).await?;
    let engagement = engagement_map.remove(&row.id).unwrap_or_default();
    let comment_authors = load_top_comment_authors(&state.db, std::iter::once(&engagement)).await?;
    Ok(Json(build_post_response(
        row,
        author.as_ref(),
        media,
        engagement,
        parent_cat.as_ref(),
        &comment_authors,
    )))
}

/// `PATCH /api/posts/{id}`. Edit. Author or administrator only.
async fn update_post(
    State(state): State<PostsState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePostRequest>,
) -> AppResult<Json<PostResponse>> {
    let row = shared::load_post(&state.db, id, post::KIND_POST).await?;

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
        let exists = Category::find_by_id(cid).one(&state.db).await?;
        if exists.is_none() {
            return Err(AppError::ValidationError("Category not found".to_string()));
        }
    }
    if let Some(ref lang) = req.content_lang {
        if !post::is_valid_content_lang(lang) {
            return Err(AppError::ValidationError(format!(
                "Invalid content_lang '{}'",
                lang
            )));
        }
    }

    // Compose-time member check: if the post is moving INTO a category,
    // confirm the author may compose there. Admins always pass.
    if !req.clear_category {
        if let Some(cid) = req.category_id {
            let cat = Category::find_by_id(cid)
                .one(&state.db)
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
    if let Some(lang) = req.content_lang {
        active.content_lang = Set(lang);
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
    let cat = match updated.category_id {
        Some(cid) => Category::find_by_id(cid).one(&state.db).await?,
        None => None,
    };
    let comment_authors = load_top_comment_authors(&state.db, std::iter::once(&engagement)).await?;
    Ok(Json(build_post_response(
        updated,
        author.as_ref(),
        media,
        engagement,
        cat.as_ref(),
        &comment_authors,
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
    let row = shared::load_post(&state.db, id, post::KIND_POST).await?;

    if row.author_id != user.id && user.role != user::ROLE_ADMINISTRATOR {
        return Err(AppError::Forbidden(
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
    /// Optional author filter. Applied on top of the visibility-tier
    /// filter so an under-tier viewer of a profile still only sees the
    /// posts that are visible to them.
    #[serde(default)]
    pub author: Option<Uuid>,
    /// Optional category filter (Phase 9e). Slug, not id, for nice URLs.
    /// `?category=uncategorized` is reserved as a synthetic filter that
    /// matches posts with `category_id IS NULL`.
    #[serde(default)]
    pub category: Option<String>,
    /// Search term. Empty/whitespace = no filter.
    #[serde(default)]
    pub q: Option<String>,
    /// Locale code (en/es/fr/zh/de) the search term is parsed under.
    /// Defaults to English when omitted or unrecognized.
    #[serde(default)]
    pub lang: Option<String>,
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
    enforce_public_feed_gate(&state.settings, tier).await?;
    let limit = query
        .limit
        .unwrap_or(FEED_LIMIT_DEFAULT)
        .clamp(1, FEED_LIMIT_MAX);

    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;

    let mut q = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(post::KIND_POST))
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
    // Phase 9e: ?category=<slug> filters by category. The synthetic
    // `uncategorized` slug matches NULL category_id. An unknown slug
    // returns an empty page (no error — the rail typically built the
    // URL from a known slug, so a stale rail entry just renders empty).
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
                    // Force-empty result so callers see {posts: [], next_cursor: null}.
                    return Ok(Json(FeedResponse {
                        posts: vec![],
                        next_cursor: None,
                    }));
                }
            }
        }
    }

    // Search filters on content_lang as well as the FTS predicate. The
    // explicit language match prevents cross-language coincidences from
    // stemmer overlap and shrinks the scan to the same-language subset
    // before the GIN index does the per-token lookup.
    if let Some(term) = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let lang = crate::posts::fts_config_for_locale(query.lang.as_deref());
        q = q
            .filter(post::Column::ContentLang.eq(lang))
            .filter(Expr::cust_with_values(
                "body_tsv @@ websearch_to_tsquery($1::regconfig, $2)",
                [lang.to_string(), term.to_string()],
            ));
    }

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
    let post_ids_for_engagement: Vec<Uuid> = page.iter().map(|p| p.id).collect();
    let mut engagement_by_post =
        fetch_engagement_for_posts(&state.db, &post_ids_for_engagement, ctx.viewer_id).await?;

    // Phase 9e: hydrate the page's categories in one batched query so the
    // chip can render on each card without N+1.
    let category_ids: Vec<Uuid> = page.iter().filter_map(|p| p.category_id).collect();
    let categories_by_id: HashMap<Uuid, category::Model> = if category_ids.is_empty() {
        HashMap::new()
    } else {
        Category::find()
            .filter(category::Column::Id.is_in(category_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|c| (c.id, c))
            .collect()
    };

    // Hydrate authors of any top_comments returned by the engagement
    // batch so each PostCard can render the inline conversation context
    // with display name + avatar without N+1.
    let top_comment_authors =
        load_top_comment_authors(&state.db, engagement_by_post.values()).await?;

    let posts: Vec<PostResponse> = page
        .into_iter()
        .map(|row| {
            let author = authors_by_id.get(&row.author_id);
            let media = media_by_post.remove(&row.id).unwrap_or_default();
            let engagement = engagement_by_post.remove(&row.id).unwrap_or_default();
            let cat = row.category_id.and_then(|id| categories_by_id.get(&id));
            build_post_response(row, author, media, engagement, cat, &top_comment_authors)
        })
        .collect();

    Ok(Json(FeedResponse { posts, next_cursor }))
}

// ==================== Helpers ====================

/// Batch-fetch user rows for every author referenced by the `top_comments`
/// in the given engagement entries. Returns a single map keyed by user_id
/// so the response builder can hydrate display name + avatar without
/// per-comment round trips.
async fn load_top_comment_authors<'a, I>(
    db: &DatabaseConnection,
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
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = User::find()
        .filter(user::Column::Id.is_in(ids))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|u| (u.id, u)).collect())
}

async fn caller_tier(auth_session: &AuthSession<crate::admin::UserAuthBackend>) -> FeedTier {
    let user = auth_session.user().await;
    FeedTier::from_role(user.as_ref().map(|u| u.role.as_str()))
}

/// Block anonymous reads when `public_feed_enabled` is off. Authed
/// callers (any tier) bypass — the gate is about whether the feed is
/// readable without an account, not about content visibility. A settings
/// read failure surfaces as a 500 (fail-closed for a security gate)
/// rather than silently allowing the read.
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
    // Same surface as missing-post: don't disclose whether anything
    // exists to a caller who shouldn't see anything.
    Err(AppError::NotFound("Not found".to_string()))
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
