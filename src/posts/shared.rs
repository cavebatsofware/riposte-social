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
//! Cross-kind helpers shared by `posts/routes.rs` and `albums/routes.rs`.
//!
//! Both route surfaces stay distinct so wire formats and URL paths remain
//! separate; the work that's identical between kinds (multipart parsing,
//! S3 upload + DB insert with rollback, kind-checked media manipulation,
//! authorship checks) lives here so both modules call the same code.

#![allow(dead_code)]

use crate::admin::UserAuth;
use crate::entities::{category, post, post_media, user, Category, Post, PostMedia};
use crate::errors::{AppError, AppResult};
use crate::posts::routes::{is_allowed_media_mime, max_bytes_for_mime};
use crate::s3::S3Service;
use crate::settings::SettingsService;
use axum::extract::multipart::Field;
use axum::extract::Multipart;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Per-kind media count caps. Posts are tight (a card carries up to 8
/// attachments cleanly); albums hold a full upload batch.
const POST_MEDIA_FILES_MAX: usize = 8;
const ALBUM_MEDIA_FILES_MAX: usize = 50;

/// Total multipart body cap. Tracks the value already used by both route
/// modules so this constant doesn't drift away from the per-file caps.
pub const COMPOSE_BODY_MAX_BYTES: usize = 256 * 1024 * 1024;

/// Returns the maximum number of media files allowed in a single
/// compose / append request for the given kind.
pub fn media_files_max_for_kind(kind: &str) -> usize {
    if kind == post::KIND_ALBUM {
        ALBUM_MEDIA_FILES_MAX
    } else {
        POST_MEDIA_FILES_MAX
    }
}

/// Lowercase, human-friendly label for a kind. Used in 404 / 403 messages
/// so a kind-mismatch hit looks like a normal "post not found" / "album
/// not found" rather than disclosing the wrong kind.
fn kind_noun(kind: &str) -> &'static str {
    if kind == post::KIND_ALBUM {
        "album"
    } else {
        "post"
    }
}

/// Title-cased version of `kind_noun` for sentence-leading messages.
fn kind_noun_title(kind: &str) -> &'static str {
    if kind == post::KIND_ALBUM {
        "Album"
    } else {
        "Post"
    }
}

/// Buffered media upload: bytes plus the mime type the browser supplied
/// plus the optional caption (used by album compose; None for posts).
pub struct PendingMedia {
    pub bytes: Vec<u8>,
    pub mime_type: String,
    pub caption: Option<String>,
}

/// Upload-ready media with its allocated row id, S3 key, and ordinal slot.
struct PlannedUpload {
    media_id: Uuid,
    s3_key: String,
    media: PendingMedia,
    ordinal: i32,
}

/// Parsed multipart payload ready for `commit_compose`. Field naming
/// reflects the unified post row: `body` and `slug` (the album name lives
/// in `slug` for kind=album rows; the album description lives in `body`).
pub struct ComposeInput {
    pub kind: String,
    pub body: String,
    pub slug: Option<String>,
    pub visibility: String,
    pub category_id: Option<Uuid>,
    pub content_lang: String,
    pub media: Vec<PendingMedia>,
}

// ============================================================
// Lookups
// ============================================================

/// Fetch a live (non-deleted) post of the expected kind. The kind
/// discriminator is part of the WHERE clause so a wrong-kind row produces
/// the same NotFound surface as a missing row — a caller probing
/// `/api/posts/{album_id}` can't distinguish "doesn't exist" from "exists
/// as an album".
pub async fn load_post(
    db: &DatabaseConnection,
    post_id: Uuid,
    expected_kind: &str,
) -> AppResult<post::Model> {
    Post::find_by_id(post_id)
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(expected_kind))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("{} not found", kind_noun_title(expected_kind))))
}

/// Lookup + author/admin assertion in one call. Used by every write helper
/// (append/delete/edit/reorder media). Returns 403 when the caller isn't
/// the author or an admin; the message is kind-shaped so frontend toasts
/// can read naturally without per-action variants.
pub async fn load_owned_post(
    db: &DatabaseConnection,
    post_id: Uuid,
    expected_kind: &str,
    user: &UserAuth,
) -> AppResult<post::Model> {
    let row = load_post(db, post_id, expected_kind).await?;
    if row.author_id == user.id || user.role == user::ROLE_ADMINISTRATOR {
        Ok(row)
    } else {
        Err(AppError::Forbidden(format!(
            "Only the author or an administrator can modify this {}",
            kind_noun(expected_kind)
        )))
    }
}

// ============================================================
// Multipart parsing
// ============================================================

/// Multipart fields recognized for posts:
///   `body` (text, required), `visibility`, `category_id`, `content_lang`,
///   `slug` (optional), `media` (file, repeated).
///
/// Multipart fields recognized for albums:
///   `name` (text, required → slug), `description` (text, optional → body),
///   `visibility`, `category_id`, `media` (file, repeated),
///   `caption_<index>` (text, optional, attached to media at index).
///
/// Format-level value checks (visibility allowlist, content_lang allowlist,
/// slug length) run here. DB-level checks (category exists, member rules)
/// run in `commit_compose`.
pub async fn parse_compose_multipart(
    multipart: &mut Multipart,
    kind: &str,
) -> AppResult<ComposeInput> {
    if !post::is_valid_kind(kind) {
        return Err(AppError::ValidationError(format!(
            "Invalid kind '{}'",
            kind
        )));
    }

    let mut body: Option<String> = None;
    let mut name: Option<String> = None;
    let mut slug: Option<String> = None;
    let mut visibility = post::VISIBILITY_PRIVATE.to_string();
    let mut category_id: Option<Uuid> = None;
    let mut content_lang = post::CONTENT_LANG_ENGLISH.to_string();
    let mut media: Vec<PendingMedia> = Vec::new();
    let mut captions: HashMap<usize, String> = HashMap::new();
    let media_cap = media_files_max_for_kind(kind);

    while let Some(field) = next_field(multipart).await? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "body" | "description" => {
                body = Some(read_text(field, &field_name).await?);
            }
            "name" => {
                name = Some(read_text(field, "name").await?);
            }
            "slug" => {
                let trimmed = read_text(field, "slug").await?.trim().to_string();
                if !trimmed.is_empty() {
                    slug = Some(trimmed);
                }
            }
            "visibility" => {
                visibility = read_text(field, "visibility").await?;
            }
            "category_id" => {
                let trimmed = read_text(field, "category_id").await?.trim().to_string();
                if !trimmed.is_empty() {
                    category_id = Some(Uuid::parse_str(&trimmed).map_err(|e| {
                        AppError::ValidationError(format!("category_id must be a UUID: {}", e))
                    })?);
                }
            }
            "content_lang" => {
                let trimmed = read_text(field, "content_lang").await?.trim().to_string();
                if !trimmed.is_empty() {
                    content_lang = trimmed;
                }
            }
            "media" => {
                consume_media_field(field, &mut media, media_cap).await?;
            }
            other if other.starts_with("caption_") => {
                consume_caption_field(other, field, &mut captions).await;
            }
            _ => {
                // Forward-compat: ignore unknown fields rather than rejecting.
            }
        }
    }

    apply_captions(&mut media, captions);

    let (resolved_body, resolved_slug) = if kind == post::KIND_ALBUM {
        let n = name
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::ValidationError("Missing required field: name".to_string()))?;
        (body.unwrap_or_default(), Some(n))
    } else {
        let b = body.ok_or_else(|| {
            AppError::ValidationError("Missing required field: body".to_string())
        })?;
        if b.trim().is_empty() {
            return Err(AppError::ValidationError(
                "Body cannot be empty".to_string(),
            ));
        }
        (b, slug)
    };

    if let Some(ref s) = resolved_slug {
        if s.chars().count() > post::SLUG_MAX_LEN {
            return Err(AppError::ValidationError(format!(
                "slug exceeds {}-character limit",
                post::SLUG_MAX_LEN
            )));
        }
    }
    if !post::is_valid_visibility(&visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            visibility
        )));
    }
    if !post::is_valid_content_lang(&content_lang) {
        return Err(AppError::ValidationError(format!(
            "Invalid content_lang '{}'",
            content_lang
        )));
    }

    Ok(ComposeInput {
        kind: kind.to_string(),
        body: resolved_body,
        slug: resolved_slug,
        visibility,
        category_id,
        content_lang,
        media,
    })
}

/// Drain the remaining multipart fields, accumulating `media` (with the
/// per-kind count cap) and `caption_<index>` captions. Used by
/// `append_media` so the parsing logic for media-only requests doesn't
/// have to be repeated.
async fn parse_media_only_multipart(
    multipart: &mut Multipart,
    cap: usize,
) -> AppResult<Vec<PendingMedia>> {
    let mut media: Vec<PendingMedia> = Vec::new();
    let mut captions: HashMap<usize, String> = HashMap::new();
    while let Some(field) = next_field(multipart).await? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "media" {
            consume_media_field(field, &mut media, cap).await?;
        } else if field_name.starts_with("caption_") {
            consume_caption_field(&field_name, field, &mut captions).await;
        }
    }
    apply_captions(&mut media, captions);
    Ok(media)
}

async fn next_field<'a>(
    multipart: &'a mut Multipart,
) -> AppResult<Option<Field<'a>>> {
    multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to parse multipart form: {}", e)))
}

async fn read_text(field: Field<'_>, label: &str) -> AppResult<String> {
    field
        .text()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to read {}: {}", label, e)))
}

async fn consume_media_field(
    field: Field<'_>,
    media: &mut Vec<PendingMedia>,
    cap: usize,
) -> AppResult<()> {
    if media.len() >= cap {
        return Err(AppError::ValidationError(format!(
            "At most {} media files per request",
            cap
        )));
    }
    let mime = field.content_type().map(|s| s.to_string()).ok_or_else(|| {
        AppError::ValidationError("Media field must include a Content-Type".to_string())
    })?;
    if !is_allowed_media_mime(&mime) {
        return Err(AppError::ValidationError(format!(
            "Unsupported media type '{}'",
            mime
        )));
    }
    let bytes = field.bytes().await.map_err(|e| {
        AppError::ValidationError(format!("Failed to read media bytes: {}", e))
    })?;
    let cap_bytes = max_bytes_for_mime(&mime);
    if bytes.len() > cap_bytes {
        return Err(AppError::ValidationError(format!(
            "Media file ({}) exceeds {} byte limit",
            bytes.len(),
            cap_bytes
        )));
    }
    media.push(PendingMedia {
        bytes: bytes.to_vec(),
        mime_type: mime,
        caption: None,
    });
    Ok(())
}

async fn consume_caption_field(
    name: &str,
    field: Field<'_>,
    captions: &mut HashMap<usize, String>,
) {
    if let Ok(idx) = name.trim_start_matches("caption_").parse::<usize>() {
        let text = field.text().await.unwrap_or_default();
        if !text.is_empty() {
            captions.insert(idx, text);
        }
    }
}

fn apply_captions(media: &mut [PendingMedia], captions: HashMap<usize, String>) {
    for (idx, caption) in captions {
        if let Some(m) = media.get_mut(idx) {
            m.caption = Some(caption);
        }
    }
}

// ============================================================
// S3 + DB persistence helpers
// ============================================================

fn build_media_plan(
    post_id: Uuid,
    media: Vec<PendingMedia>,
    base_ordinal: i32,
) -> Vec<PlannedUpload> {
    media
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let media_id = Uuid::new_v4();
            PlannedUpload {
                media_id,
                s3_key: format!("posts/{}/{}", post_id, media_id),
                media: m,
                ordinal: base_ordinal + i as i32,
            }
        })
        .collect()
}

/// Push every planned upload into S3, tracking successful keys so a
/// later failure can roll the whole batch back.
async fn upload_media(s3: &S3Service, plan: &[PlannedUpload]) -> AppResult<Vec<String>> {
    let mut uploaded: Vec<String> = Vec::new();
    for item in plan {
        if let Err(e) = s3
            .put_object_at(&item.s3_key, item.media.bytes.clone(), &item.media.mime_type)
            .await
        {
            rollback_uploads(s3, &uploaded).await;
            return Err(AppError::InternalError(format!(
                "Failed to upload media: {}",
                e
            )));
        }
        uploaded.push(item.s3_key.clone());
    }
    Ok(uploaded)
}

/// Best-effort cleanup. We swallow per-key errors because the call site
/// already failed on something else; surfacing a secondary delete error
/// would just mask the real cause.
async fn rollback_uploads(s3: &S3Service, keys: &[String]) {
    for k in keys {
        let _ = s3.delete_object_at(k).await;
    }
}

async fn insert_media_rows<C>(
    txn: &C,
    post_id: Uuid,
    plan: &[PlannedUpload],
) -> Result<Vec<post_media::Model>, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    let mut rows = Vec::with_capacity(plan.len());
    for item in plan {
        let saved = post_media::ActiveModel {
            id: Set(item.media_id),
            post_id: Set(post_id),
            s3_key: Set(item.s3_key.clone()),
            mime_type: Set(item.media.mime_type.clone()),
            width: Set(None),
            height: Set(None),
            ordinal: Set(item.ordinal),
            caption: Set(item.media.caption.clone()),
            ..Default::default()
        }
        .insert(txn)
        .await?;
        rows.push(saved);
    }
    Ok(rows)
}

/// Site-mode toggle: posters can be muted by an admin without revoking
/// their role. Admins always bypass. Settings-read failures fail-closed —
/// for a security gate, "I don't know" must mean "deny", not "permit".
async fn enforce_poster_gate(settings: &SettingsService, user: &UserAuth) -> AppResult<()> {
    if user.role != user::ROLE_POSTER {
        return Ok(());
    }
    let enabled = settings
        .get_poster_posting_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if enabled {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Posting is currently disabled by an administrator".to_string(),
        ))
    }
}

/// Validate the optional category and confirm the caller may compose into
/// it. No-op when `category_id` is None.
async fn validate_category(
    db: &DatabaseConnection,
    user: &UserAuth,
    category_id: Option<Uuid>,
) -> AppResult<()> {
    let Some(cid) = category_id else {
        return Ok(());
    };
    let cat = Category::find_by_id(cid)
        .one(db)
        .await?
        .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
    crate::visibility::ensure_can_compose_into_category(db, user, &cat).await
}

// ============================================================
// Top-level operations
// ============================================================

/// Insert a `posts` row plus its `post_media` rows. Uploads media to S3
/// first; on DB failure, deletes any objects already uploaded so the
/// transaction's rollback doesn't leave orphans. Enforces the
/// poster-posting site toggle and category compose-permission rules.
pub async fn commit_compose(
    db: &DatabaseConnection,
    s3: &S3Service,
    settings: &SettingsService,
    user: &UserAuth,
    input: ComposeInput,
) -> AppResult<(post::Model, Vec<post_media::Model>)> {
    enforce_poster_gate(settings, user).await?;
    validate_category(db, user, input.category_id).await?;

    let post_id = Uuid::new_v4();
    let plan = build_media_plan(post_id, input.media, 0);
    let uploaded = upload_media(s3, &plan).await?;

    let txn_result = async {
        let txn = db.begin().await?;
        let post_row = post::ActiveModel {
            id: Set(post_id),
            author_id: Set(user.id),
            body: Set(input.body),
            visibility: Set(input.visibility),
            published_at: Set(Utc::now().into()),
            import_source: Set(None),
            import_external_id: Set(None),
            deleted_at: Set(None),
            category_id: Set(input.category_id),
            content_lang: Set(input.content_lang),
            kind: Set(input.kind),
            slug: Set(input.slug),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        let media_rows = insert_media_rows(&txn, post_id, &plan).await?;
        txn.commit().await?;
        Ok::<(post::Model, Vec<post_media::Model>), sea_orm::DbErr>((post_row, media_rows))
    }
    .await;

    match txn_result {
        Ok(pair) => Ok(pair),
        Err(e) => {
            rollback_uploads(s3, &uploaded).await;
            Err(AppError::InternalError(format!(
                "Failed to create post: {}",
                e
            )))
        }
    }
}

/// Append media to an existing post or album. Kind-checked + author/admin
/// gated. Returns the newly inserted media rows; callers re-fetch when
/// they need the full media list.
pub async fn append_media(
    db: &DatabaseConnection,
    s3: &S3Service,
    user: &UserAuth,
    post_id: Uuid,
    expected_kind: &str,
    multipart: &mut Multipart,
) -> AppResult<Vec<post_media::Model>> {
    load_owned_post(db, post_id, expected_kind, user).await?;

    let next_ordinal: i32 = PostMedia::find()
        .filter(post_media::Column::PostId.eq(post_id))
        .order_by_desc(post_media::Column::Ordinal)
        .one(db)
        .await?
        .map(|m| m.ordinal + 1)
        .unwrap_or(0);

    let media =
        parse_media_only_multipart(multipart, media_files_max_for_kind(expected_kind)).await?;
    if media.is_empty() {
        return Err(AppError::ValidationError(
            "Append requires at least one media file".to_string(),
        ));
    }

    let plan = build_media_plan(post_id, media, next_ordinal);
    let uploaded = upload_media(s3, &plan).await?;

    let txn_result = async {
        let txn = db.begin().await?;
        let rows = insert_media_rows(&txn, post_id, &plan).await?;
        txn.commit().await?;
        Ok::<Vec<post_media::Model>, sea_orm::DbErr>(rows)
    }
    .await;

    match txn_result {
        Ok(rows) => Ok(rows),
        Err(e) => {
            rollback_uploads(s3, &uploaded).await;
            Err(AppError::InternalError(format!(
                "Failed to append media: {}",
                e
            )))
        }
    }
}

/// Delete one media item from a post or album. The S3 object is removed
/// best-effort after the DB delete commits; a stale S3 object is
/// preferable to a stale DB row.
pub async fn delete_media(
    db: &DatabaseConnection,
    s3: &S3Service,
    user: &UserAuth,
    post_id: Uuid,
    media_id: Uuid,
    expected_kind: &str,
) -> AppResult<()> {
    load_owned_post(db, post_id, expected_kind, user).await?;

    let media = PostMedia::find_by_id(media_id)
        .filter(post_media::Column::PostId.eq(post_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let s3_key = media.s3_key.clone();
    let active: post_media::ActiveModel = media.into();
    active.delete(db).await?;
    let _ = s3.delete_object_at(&s3_key).await;
    Ok(())
}

/// Edit a media item's caption. An empty/whitespace caption clears the
/// field (NULL); a present caption replaces it.
pub async fn edit_media_caption(
    db: &DatabaseConnection,
    user: &UserAuth,
    post_id: Uuid,
    media_id: Uuid,
    expected_kind: &str,
    caption: Option<String>,
) -> AppResult<post_media::Model> {
    load_owned_post(db, post_id, expected_kind, user).await?;

    let media = PostMedia::find_by_id(media_id)
        .filter(post_media::Column::PostId.eq(post_id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Media not found".to_string()))?;

    let mut active: post_media::ActiveModel = media.into();
    let next = caption.and_then(|c| {
        let trimmed = c.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    active.caption = Set(next);
    Ok(active.update(db).await?)
}

/// Reassign ordinals across a post or album's media in one transaction.
/// `ordinals` lists every (media_id, new_ordinal) pair the caller wants
/// applied; each id must belong to the post and each post-owned media
/// must appear exactly once so a partial reorder can't leave the set in
/// an inconsistent state.
///
/// Implementation note: the swap runs in two phases under a transaction,
/// using a +1_000_000 offset so per-row updates don't transiently collide
/// on a (post_id, ordinal) uniqueness constraint if one is added later.
/// No such constraint exists today; this is defense-in-depth.
pub async fn reorder_media(
    db: &DatabaseConnection,
    user: &UserAuth,
    post_id: Uuid,
    expected_kind: &str,
    ordinals: Vec<(Uuid, i32)>,
) -> AppResult<()> {
    load_owned_post(db, post_id, expected_kind, user).await?;

    let existing: Vec<post_media::Model> = PostMedia::find()
        .filter(post_media::Column::PostId.eq(post_id))
        .all(db)
        .await?;
    let existing_ids: HashSet<Uuid> = existing.iter().map(|m| m.id).collect();

    if ordinals.len() != existing.len() {
        return Err(AppError::ValidationError(format!(
            "reorder requires every media id ({} expected, {} provided)",
            existing.len(),
            ordinals.len()
        )));
    }
    let mut seen: HashSet<Uuid> = HashSet::new();
    for (id, _) in &ordinals {
        if !existing_ids.contains(id) {
            return Err(AppError::ValidationError(format!(
                "media {} does not belong to this post",
                id
            )));
        }
        if !seen.insert(*id) {
            return Err(AppError::ValidationError(format!(
                "media {} appears more than once in the reorder list",
                id
            )));
        }
    }

    const OFFSET: i32 = 1_000_000;
    let txn = db.begin().await?;
    for (id, new_ord) in &ordinals {
        post_media::ActiveModel {
            id: Set(*id),
            ordinal: Set(new_ord + OFFSET),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    for (id, new_ord) in &ordinals {
        post_media::ActiveModel {
            id: Set(*id),
            ordinal: Set(*new_ord),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok(())
}

// ============================================================
// Response-builder helpers
// ============================================================

/// Helper for response builders that need the implicit-cover (lowest
/// ordinal) media id. Used by the album alias to produce the legacy
/// `cover_media_id` field. Returns None when the media list is empty.
pub fn implicit_cover_id(media: &[post_media::Model]) -> Option<Uuid> {
    media.iter().min_by_key(|m| m.ordinal).map(|m| m.id)
}

/// Mirror of `category` lookup used by every response builder. Centralized
/// here so the kind-agnostic visibility computation stays consistent
/// between post and album responses.
pub async fn load_category(
    db: &DatabaseConnection,
    category_id: Option<Uuid>,
) -> AppResult<Option<category::Model>> {
    match category_id {
        Some(cid) => Ok(Category::find_by_id(cid).one(db).await?),
        None => Ok(None),
    }
}
