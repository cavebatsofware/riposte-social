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
//! DB insertion, ordinal assignment, and post-lifecycle orchestration.
//! Sits between the multipart parser and the S3 uploader, gluing the
//! compose / append / delete / edit / reorder operations together.

use crate::admin::UserAuth;
use crate::entities::{category, post, post_media, user, Category, Post, PostMedia};
use crate::errors::{AppError, AppResult};
use crate::posts::media::parse::parse_media_only_multipart;
use crate::posts::media::upload::{build_media_plan, rollback_uploads, upload_media};
use crate::posts::media::variants::process_image_variants;
use crate::posts::media::{kind_noun, kind_noun_title, media_files_max_for_kind};
use crate::s3::S3Service;
use crate::settings::SettingsService;
use axum::extract::Multipart;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use std::collections::HashSet;
use uuid::Uuid;

/// Fetch a live (non-deleted) post of the expected kind. The kind
/// discriminator is part of the WHERE clause so a wrong-kind row produces
/// the same NotFound surface as a missing row.
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
/// the author or an admin.
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

/// Helper for response builders that need the implicit-cover (lowest
/// ordinal) media id. Returns None when the media list is empty.
pub fn implicit_cover_id(media: &[post_media::Model]) -> Option<Uuid> {
    media.iter().min_by_key(|m| m.ordinal).map(|m| m.id)
}

/// Centralized category lookup so the kind-agnostic visibility computation
/// stays consistent between post and album responses.
pub async fn load_category(
    db: &DatabaseConnection,
    category_id: Option<Uuid>,
) -> AppResult<Option<category::Model>> {
    match category_id {
        Some(cid) => Ok(Category::find_by_id(cid).one(db).await?),
        None => Ok(None),
    }
}

async fn insert_media_rows<C>(
    txn: &C,
    post_id: Uuid,
    plan: &[crate::posts::media::PlannedUpload],
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
            width: Set(item.width),
            height: Set(item.height),
            ordinal: Set(item.ordinal),
            caption: Set(item.media.caption.clone()),
            thumbnail_data: Set(item.thumbnail_data.clone()),
            icon_data: Set(item.icon_data.clone()),
            ..Default::default()
        }
        .insert(txn)
        .await?;
        rows.push(saved);
    }
    Ok(rows)
}

/// Site-mode toggle: posters can be muted by an admin without revoking
/// their role. Admins always bypass. Settings-read failures fail-closed.
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

/// Insert a `posts` row plus its `post_media` rows. Uploads media to S3
/// first; on DB failure, deletes any objects already uploaded so the
/// transaction's rollback doesn't leave orphans. Enforces the
/// poster-posting site toggle and category compose-permission rules.
pub async fn commit_compose(
    db: &DatabaseConnection,
    s3: &S3Service,
    settings: &SettingsService,
    user: &UserAuth,
    input: crate::posts::media::ComposeInput,
) -> AppResult<(post::Model, Vec<post_media::Model>)> {
    enforce_poster_gate(settings, user).await?;
    validate_category(db, user, input.category_id).await?;

    let post_id = Uuid::new_v4();
    let mut plan = build_media_plan(post_id, input.media, 0);
    process_image_variants(&mut plan, settings).await?;
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
    settings: &SettingsService,
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

    let mut plan = build_media_plan(post_id, media, next_ordinal);
    process_image_variants(&mut plan, settings).await?;
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
/// Implementation note: the swap runs under a transaction,
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
    let mut seen_ords: HashSet<i32> = HashSet::new();
    for (id, ord) in &ordinals {
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
        if *ord < 0 {
            return Err(AppError::ValidationError(format!(
                "ordinal {} is negative; ordinals must be non-negative",
                ord
            )));
        }
        if !seen_ords.insert(*ord) {
            return Err(AppError::ValidationError(format!(
                "ordinal {} is duplicated; each media must have a unique ordinal",
                ord
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
