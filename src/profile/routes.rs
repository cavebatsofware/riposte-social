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
//! Profile HTTP handlers.
//!
//! - `GET    /api/me/profile`           caller's own profile (auth required)
//! - `PATCH  /api/me/profile`           edit handle/display_name/bio/pronouns
//! - `PATCH  /api/me/locale`            save UI locale preference
//! - `POST   /api/me/avatar`            multipart upload, server-side crop
//! - `DELETE /api/me/avatar`            remove avatar, drop S3 object
//! - `GET    /api/profiles/{handle}`    public profile by handle
//! - `GET    /avatars/{user_id}`        serve cropped avatar bytes
//!
//! `PATCH` and uploads are gated by `require_authenticated`. The public
//! profile read uses the same `public_feed_enabled` gate as the feed: when
//! anonymous reads are off, `/api/profiles/{handle}` returns the same 404
//! shape as a missing user, so existence isn't disclosed.

use crate::admin::UserAuth;
use crate::entities::{user, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::profile::{
    avatar_url_for, locale, validate_handle_shape, BIO_MAX_LEN, PRONOUNS_MAX_LEN,
};
use crate::s3::S3Service;
use crate::settings::SettingsService;
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Extension, Router,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use uuid::Uuid;

/// Hard cap on avatar uploads. 5 MiB is plenty for a webp/jpeg square; a
/// well-shot phone photo lands well under this.
const AVATAR_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Side length of the cropped, resized avatar. 512px serves both retina
/// profile cards (256pt @ 2x) and feed-meta thumbnails without an extra
/// rendition pipeline.
const AVATAR_OUTPUT_SIDE: u32 = 512;

/// Reject obviously absurd inputs before we spend memory decoding them.
/// 8000^2 RGBA is ~256 MiB; anything beyond that is a bomb attempt, not
/// a real photo.
const AVATAR_MAX_INPUT_DIMENSION: u32 = 8000;

/// Allowed mime types for the upload. Browsers feed us these from
/// `<input type="file" accept="image/*">`; SVG and HEIC stay out because
/// they have unsafe / patent-encumbered codecs.
const AVATAR_ALLOWED_MIMES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

#[derive(Clone)]
pub struct ProfileState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: SettingsService,
}

/// Self-service routes (gated by `require_authenticated` at app.rs).
pub fn me_profile_routes() -> Router<ProfileState> {
    Router::new()
        .route(
            "/api/me/profile",
            get(get_me_profile).patch(patch_me_profile),
        )
        .route("/api/me/locale", axum::routing::patch(patch_me_locale))
        .route(
            "/api/me/avatar",
            post(post_me_avatar)
                .layer(DefaultBodyLimit::max(AVATAR_MAX_BYTES))
                .delete(delete_me_avatar),
        )
}

/// Public profile reads + avatar serving. No auth required; the public-feed
/// gate filters anonymous callers when the site is in invite-only mode.
pub fn public_profile_routes() -> Router<ProfileState> {
    Router::new()
        .route("/api/profiles", get(list_profiles))
        .route("/api/profiles/{handle}", get(get_profile_by_handle))
        .route("/avatars/{user_id}", get(serve_avatar))
}

// ==================== wire format ====================

/// Response shape for the caller's own profile. Includes `email` since
/// the caller is reading their own row.
#[derive(Serialize)]
pub struct MeProfileResponse {
    pub user_id: Uuid,
    pub handle: String,
    pub email: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub follower_count: i64,
    pub following_count: i64,
}

/// Response shape for public profile reads. Email is intentionally omitted:
/// public profiles are visible to anyone who can reach the page; the
/// recipient's email is not.
///
/// `follows_you` and `you_follow` are populated for authenticated viewers
/// only; they're both `false` for anonymous reads since the relationship
/// has no defined viewer.
#[derive(Serialize)]
pub struct PublicProfileResponse {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub follower_count: i64,
    pub following_count: i64,
    pub follows_you: bool,
    pub you_follow: bool,
}

/// Compact profile shape used by the list endpoint. Drops `bio` and
/// `pronouns` since they aren't shown in the rail or directory grid;
/// callers fetch the full `PublicProfileResponse` when navigating to a
/// profile.
#[derive(Serialize)]
pub struct ProfileSummary {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
}

#[derive(Serialize)]
pub struct ListProfilesResponse {
    pub profiles: Vec<ProfileSummary>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListProfilesQuery {
    pub limit: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PatchMeProfileRequest {
    /// New handle. Validated for shape + uniqueness; only included when the
    /// caller is changing it.
    pub handle: Option<String>,
    /// Sets or replaces display_name. To clear it, send an empty string.
    pub display_name: Option<String>,
    /// Sets or replaces bio. Empty string clears.
    pub bio: Option<String>,
    /// Sets or replaces pronouns. Empty string clears.
    pub pronouns: Option<String>,
}

#[derive(Serialize)]
pub struct AvatarUploadResponse {
    pub avatar_url: String,
}

#[derive(Deserialize)]
pub struct PatchMeLocaleRequest {
    /// One of `crate::profile::locale::SUPPORTED_LOCALES`. Anything else
    /// returns 400.
    pub locale: String,
}

#[derive(Serialize)]
pub struct LocaleResponse {
    pub locale: String,
}

// ==================== handlers ====================

async fn get_me_profile(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
) -> AppResult<Json<MeProfileResponse>> {
    let model = User::find_by_id(user_auth.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    let follower_count = crate::follows::count_followers(&state.db, model.id).await? as i64;
    let following_count = crate::follows::count_following(&state.db, model.id).await? as i64;

    Ok(Json(MeProfileResponse {
        user_id: model.id,
        handle: model.handle.clone(),
        email: model.email.clone(),
        display_name: model.display_name.clone(),
        bio: model.bio.clone(),
        pronouns: model.pronouns.clone(),
        avatar_url: avatar_url_for(&model),
        role: model.role.clone(),
        follower_count,
        following_count,
    }))
}

/// `PATCH /api/me/locale`. Single-purpose write that runs on every
/// language switch from the social-frontend's LanguagePicker. Cheaper than
/// going through `/api/me/profile` (which validates handle uniqueness and
/// other fields). Idempotent — re-saving the same locale is a no-op write.
async fn patch_me_locale(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
    Json(req): Json<PatchMeLocaleRequest>,
) -> AppResult<Json<LocaleResponse>> {
    if !locale::is_supported(&req.locale) {
        return Err(AppError::ValidationError(format!(
            "Unsupported locale '{}'",
            req.locale
        )));
    }

    let model = User::find_by_id(user_auth.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    // No-op when the value hasn't changed: avoids touching `updated_at`
    // on every page-load that fires a redundant PATCH.
    if model.locale.as_deref() == Some(req.locale.as_str()) {
        return Ok(Json(LocaleResponse { locale: req.locale }));
    }

    let mut active: user::ActiveModel = model.into();
    active.locale = Set(Some(req.locale.clone()));
    active.update(&state.db).await?;
    Ok(Json(LocaleResponse { locale: req.locale }))
}

async fn patch_me_profile(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
    Json(req): Json<PatchMeProfileRequest>,
) -> AppResult<Json<MeProfileResponse>> {
    let model = User::find_by_id(user_auth.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    // Validate up front so we don't half-apply changes.
    if let Some(ref h) = req.handle {
        let trimmed = h.trim();
        if trimmed != model.handle {
            validate_handle_shape(trimmed).map_err(AppError::ValidationError)?;
            // Uniqueness: we exclude the caller's row since "set to my
            // current handle" is a no-op rather than a collision.
            let conflict = User::find()
                .filter(user::Column::Handle.eq(trimmed))
                .filter(user::Column::Id.ne(model.id))
                .one(&state.db)
                .await?;
            if conflict.is_some() {
                return Err(AppError::ValidationError(
                    "That handle is already taken".to_string(),
                ));
            }
        }
    }
    if let Some(ref bio) = req.bio {
        if bio.chars().count() > BIO_MAX_LEN {
            return Err(AppError::ValidationError(format!(
                "Bio must be at most {} characters",
                BIO_MAX_LEN
            )));
        }
    }
    if let Some(ref p) = req.pronouns {
        if p.chars().count() > PRONOUNS_MAX_LEN {
            return Err(AppError::ValidationError(format!(
                "Pronouns must be at most {} characters",
                PRONOUNS_MAX_LEN
            )));
        }
    }

    let mut active: user::ActiveModel = model.into();
    if let Some(h) = req.handle {
        active.handle = Set(h.trim().to_string());
    }
    if let Some(name) = req.display_name {
        let trimmed = name.trim();
        active.display_name = Set(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        });
    }
    if let Some(bio) = req.bio {
        let trimmed = bio.trim();
        active.bio = Set(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        });
    }
    if let Some(p) = req.pronouns {
        let trimmed = p.trim();
        active.pronouns = Set(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        });
    }
    let updated = active.update(&state.db).await?;

    let follower_count = crate::follows::count_followers(&state.db, updated.id).await? as i64;
    let following_count = crate::follows::count_following(&state.db, updated.id).await? as i64;

    Ok(Json(MeProfileResponse {
        user_id: updated.id,
        handle: updated.handle.clone(),
        email: updated.email.clone(),
        display_name: updated.display_name.clone(),
        bio: updated.bio.clone(),
        pronouns: updated.pronouns.clone(),
        avatar_url: avatar_url_for(&updated),
        role: updated.role.clone(),
        follower_count,
        following_count,
    }))
}

/// `GET /api/profiles` returns activated, active users visible to the
/// caller. Anonymous callers are gated by `public_feed_enabled` (same as
/// the per-handle read). The caller's own row is excluded so the rail and
/// directory show *other* members; callers reach their own profile via the
/// avatar menu.
async fn list_profiles(
    State(state): State<ProfileState>,
    auth_session: UserAuthSession,
    Query(query): Query<ListProfilesQuery>,
) -> AppResult<Json<ListProfilesResponse>> {
    enforce_public_profile_gate(&state.settings, &auth_session).await?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let viewer_id = auth_session.user().await.map(|u| u.id);

    let mut q = User::find()
        .filter(user::Column::Active.eq(true))
        .filter(user::Column::ActivatedAt.is_not_null())
        .order_by_asc(user::Column::Handle)
        .limit(limit);
    if let Some(vid) = viewer_id {
        q = q.filter(user::Column::Id.ne(vid));
    }

    let rows = q.all(&state.db).await?;
    let profiles = rows
        .into_iter()
        .map(|m| ProfileSummary {
            user_id: m.id,
            handle: m.handle.clone(),
            display_name: m.display_name.clone(),
            avatar_url: avatar_url_for(&m),
            role: m.role.clone(),
        })
        .collect();
    Ok(Json(ListProfilesResponse { profiles }))
}

async fn get_profile_by_handle(
    State(state): State<ProfileState>,
    auth_session: UserAuthSession,
    Path(handle): Path<String>,
) -> AppResult<Json<PublicProfileResponse>> {
    enforce_public_profile_gate(&state.settings, &auth_session).await?;

    let model = User::find()
        .filter(user::Column::Handle.eq(&handle))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Profile not found".to_string()))?;

    // Inactive users disappear from the public profile surface (same as
    // login is rejected for them). Soft-delete equivalent.
    if !model.active {
        return Err(AppError::AuthError("Profile not found".to_string()));
    }

    let follower_count = crate::follows::count_followers(&state.db, model.id).await? as i64;
    let following_count = crate::follows::count_following(&state.db, model.id).await? as i64;
    let viewer_id = auth_session.user().await.map(|u| u.id);
    let (follows_you, you_follow) = if let Some(vid) = viewer_id {
        if vid == model.id {
            (false, false)
        } else {
            let states = crate::follows::fetch_follow_states(&state.db, vid, &[model.id]).await?;
            let s = states.get(&model.id).copied().unwrap_or_default();
            (s.follows_you, s.you_follow)
        }
    } else {
        (false, false)
    };

    Ok(Json(PublicProfileResponse {
        user_id: model.id,
        handle: model.handle.clone(),
        display_name: model.display_name.clone(),
        bio: model.bio.clone(),
        pronouns: model.pronouns.clone(),
        avatar_url: avatar_url_for(&model),
        role: model.role.clone(),
        follower_count,
        following_count,
        follows_you,
        you_follow,
    }))
}

async fn post_me_avatar(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
    mut multipart: Multipart,
) -> AppResult<Json<AvatarUploadResponse>> {
    // Read the single `file` field.
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_mime: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("Failed to parse multipart form: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name != "file" {
            continue;
        }
        let mime = field.content_type().map(|s| s.to_string()).ok_or_else(|| {
            AppError::ValidationError("Avatar field must include a Content-Type".to_string())
        })?;
        if !AVATAR_ALLOWED_MIMES.contains(&mime.as_str()) {
            return Err(AppError::ValidationError(format!(
                "Unsupported avatar type '{}'. Allowed: {:?}",
                mime, AVATAR_ALLOWED_MIMES
            )));
        }
        let bytes = field.bytes().await.map_err(|e| {
            AppError::ValidationError(format!("Failed to read avatar bytes: {}", e))
        })?;
        if bytes.len() > AVATAR_MAX_BYTES {
            return Err(AppError::ValidationError(format!(
                "Avatar exceeds {} byte limit",
                AVATAR_MAX_BYTES
            )));
        }
        file_bytes = Some(bytes.to_vec());
        file_mime = Some(mime);
        break;
    }
    let bytes = file_bytes
        .ok_or_else(|| AppError::ValidationError("Missing required field: file".to_string()))?;
    let _ = file_mime; // mime check already enforced above

    // CPU-bound: decode + crop + resize + encode happens on a blocking
    // worker so the async runtime stays responsive.
    let normalized = tokio::task::spawn_blocking(move || normalize_avatar_bytes(&bytes))
        .await
        .map_err(|e| AppError::InternalError(format!("avatar worker join failed: {}", e)))?
        .map_err(AppError::ValidationError)?;

    // Upload first; on failure we don't change the row.
    let new_key = format!("avatars/{}/{}.webp", user_auth.id, Uuid::new_v4());
    state
        .s3
        .put_object_at(&new_key, normalized, "image/webp")
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to upload avatar: {:#}", e)))?;

    // Swap the row's avatar_s3_key. Best-effort delete the previous object
    // after the row update commits (a stale object is harmless storage; a
    // missing row pointer would break image loads, so prefer the latter
    // failure mode).
    let prev_key = {
        let model = User::find_by_id(user_auth.id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;
        let prev = model.avatar_s3_key.clone();
        let mut active: user::ActiveModel = model.into();
        active.avatar_s3_key = Set(Some(new_key.clone()));
        active.update(&state.db).await?;
        prev
    };
    if let Some(prev) = prev_key {
        if prev != new_key {
            if let Err(e) = state.s3.delete_object_at(&prev).await {
                tracing::warn!("Failed to delete previous avatar object {}: {:#}", prev, e);
            }
        }
    }

    Ok(Json(AvatarUploadResponse {
        avatar_url: format!("/avatars/{}", user_auth.id),
    }))
}

async fn delete_me_avatar(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
) -> AppResult<StatusCode> {
    let model = User::find_by_id(user_auth.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("User not found".to_string()))?;

    let prev = model.avatar_s3_key.clone();
    if prev.is_none() {
        return Ok(StatusCode::NO_CONTENT);
    }

    let mut active: user::ActiveModel = model.into();
    active.avatar_s3_key = Set(None);
    active.update(&state.db).await?;

    if let Some(prev_key) = prev {
        if let Err(e) = state.s3.delete_object_at(&prev_key).await {
            tracing::warn!(
                "Failed to delete avatar object {} for user {}: {:#}",
                prev_key,
                user_auth.id,
                e
            );
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn serve_avatar(
    State(state): State<ProfileState>,
    auth_session: UserAuthSession,
    Path(user_id): Path<Uuid>,
) -> AppResult<Response> {
    enforce_public_profile_gate(&state.settings, &auth_session).await?;

    let model = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Avatar not found".to_string()))?;
    if !model.active {
        return Err(AppError::AuthError("Avatar not found".to_string()));
    }
    let key = model
        .avatar_s3_key
        .ok_or_else(|| AppError::AuthError("Avatar not found".to_string()))?;

    let (bytes, _stored_type) = state
        .s3
        .get_object_at(&key)
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to load avatar: {:#}", e)))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/webp".to_string()),
            // Avatars are addressed by user_id but the underlying object
            // changes whenever the user replaces it. Short cache + we
            // re-key the S3 object on each upload so a stale cached body
            // isn't load-bearing.
            (header::CACHE_CONTROL, "public, max-age=300".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

// ==================== helpers ====================

/// Anonymous reads of profiles and avatars are gated by the same
/// `public_feed_enabled` switch as the feed itself. Authed callers (any
/// tier) bypass. Settings read failures fail closed — same convention as
/// the feed gate in `posts/routes.rs`.
async fn enforce_public_profile_gate(
    settings: &SettingsService,
    auth_session: &UserAuthSession,
) -> AppResult<()> {
    if auth_session.user().await.is_some() {
        return Ok(());
    }
    let enabled = settings
        .get_public_feed_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if enabled {
        return Ok(());
    }
    Err(AppError::AuthError("Profile not found".to_string()))
}

/// Decode arbitrary input bytes, center-crop to a square, resize to
/// `AVATAR_OUTPUT_SIDE`, and encode as webp. Returns the encoded bytes.
///
/// Errors are returned as user-visible messages — they always indicate the
/// input the user uploaded, so surfacing the reason ("could not decode
/// image", "image too large") is fine.
fn normalize_avatar_bytes(input: &[u8]) -> Result<Vec<u8>, String> {
    use image::imageops::{crop_imm, resize, FilterType};
    use image::ImageReader;

    let reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|e| format!("Could not read image: {}", e))?;
    let img = reader
        .decode()
        .map_err(|e| format!("Could not decode image: {}", e))?;

    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err("Image has zero dimension".to_string());
    }
    if w > AVATAR_MAX_INPUT_DIMENSION || h > AVATAR_MAX_INPUT_DIMENSION {
        return Err(format!(
            "Image dimensions exceed {}x{}",
            AVATAR_MAX_INPUT_DIMENSION, AVATAR_MAX_INPUT_DIMENSION
        ));
    }

    // Center-crop to a square.
    let side = w.min(h);
    let x_off = (w - side) / 2;
    let y_off = (h - side) / 2;
    let square = crop_imm(&img, x_off, y_off, side, side).to_image();

    // Resize to the canonical output size. Lanczos3 is the highest-quality
    // option in `image`'s built-in filters.
    let resized = if side != AVATAR_OUTPUT_SIDE {
        resize(
            &square,
            AVATAR_OUTPUT_SIDE,
            AVATAR_OUTPUT_SIDE,
            FilterType::Lanczos3,
        )
    } else {
        square
    };

    // Encode as webp.
    let mut out = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
    image::ImageEncoder::write_image(
        encoder,
        resized.as_raw(),
        resized.width(),
        resized.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("Could not encode webp: {}", e))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_png(side: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let img = image::RgbaImage::from_pixel(side, side, image::Rgba([10, 20, 30, 255]));
        let mut out = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut out);
        encoder
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        out
    }

    #[test]
    fn normalize_centers_and_resizes() {
        let png = synth_png(1024);
        let webp = normalize_avatar_bytes(&png).unwrap();
        // Re-decode and check dimensions.
        let img = image::load_from_memory(&webp).unwrap();
        assert_eq!(img.width(), AVATAR_OUTPUT_SIDE);
        assert_eq!(img.height(), AVATAR_OUTPUT_SIDE);
    }

    #[test]
    fn normalize_rejects_garbage() {
        let res = normalize_avatar_bytes(b"not an image");
        assert!(res.is_err());
    }
}
