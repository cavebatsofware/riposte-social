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

use crate::admin::UserAuth;
use crate::entities::user;
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::profile::queries;
use crate::profile::types::{
    AvatarUploadResponse, ListProfilesQuery, ListProfilesResponse, LocaleResponse,
    MeProfileResponse, PatchMeLocaleRequest, PatchMeProfileRequest, ProfileSummary,
    PublicProfileResponse,
};
use crate::profile::{
    avatar_icon_data_for, avatar_url_for, locale, validate_handle_shape, ProfileState, BIO_MAX_LEN,
    PRONOUNS_MAX_LEN,
};
use crate::settings::SettingsService;
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Extension, Router,
};
use sea_orm::Set;
use std::io::Cursor;
use uuid::Uuid;

/// Hard cap on avatar uploads. 5 MiB is plenty for a webp/jpeg square.
const AVATAR_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Side length of the cropped, resized avatar. 512px serves both retina
/// profile cards (256pt @ 2x) and feed-meta thumbnails without an extra
/// rendition pipeline.
const AVATAR_OUTPUT_SIDE: u32 = 512;
const AVATAR_ICON_SIDE: u32 = 64;

/// Browsers feed us these from `<input type="file" accept="image/*">`; SVG
/// and HEIC stay out because they have unsafe / patent-encumbered codecs.
const AVATAR_ALLOWED_MIMES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

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

pub fn public_profile_routes() -> Router<ProfileState> {
    Router::new()
        .route("/api/profiles", get(list_profiles))
        .route("/api/profiles/{handle}", get(get_profile_by_handle))
        .route("/avatars/{user_id}", get(serve_avatar))
}

async fn get_me_profile(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
) -> AppResult<Json<MeProfileResponse>> {
    let model = queries::find_user(&state.db, user_auth.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

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
        avatar_icon_data: avatar_icon_data_for(&model),
        role: model.role.clone(),
        follower_count,
        following_count,
    }))
}

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

    let model = queries::find_user(&state.db, user_auth.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    // No-op when the value hasn't changed: avoids touching `updated_at`
    // on every page-load that fires a redundant PATCH.
    if model.locale.as_deref() == Some(req.locale.as_str()) {
        return Ok(Json(LocaleResponse { locale: req.locale }));
    }

    let mut active: user::ActiveModel = model.into();
    active.locale = Set(Some(req.locale.clone()));
    queries::update_user(&state.db, active).await?;
    Ok(Json(LocaleResponse { locale: req.locale }))
}

async fn patch_me_profile(
    State(state): State<ProfileState>,
    Extension(user_auth): Extension<UserAuth>,
    Json(req): Json<PatchMeProfileRequest>,
) -> AppResult<Json<MeProfileResponse>> {
    let model = queries::find_user(&state.db, user_auth.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    if let Some(ref h) = req.handle {
        let trimmed = h.trim();
        if trimmed != model.handle {
            validate_handle_shape(trimmed).map_err(AppError::ValidationError)?;
            if queries::handle_taken_by_other(&state.db, trimmed, model.id).await? {
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
    let updated = queries::update_user(&state.db, active).await?;

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
        avatar_icon_data: avatar_icon_data_for(&updated),
        role: updated.role.clone(),
        follower_count,
        following_count,
    }))
}

async fn list_profiles(
    State(state): State<ProfileState>,
    auth_session: UserAuthSession,
    Query(query): Query<ListProfilesQuery>,
) -> AppResult<Json<ListProfilesResponse>> {
    enforce_public_profile_gate(&state.settings, &auth_session).await?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let viewer_id = auth_session.user().await.map(|u| u.id);

    let rows = queries::list_active_profiles_excluding(&state.db, viewer_id, limit).await?;
    let profiles = rows
        .into_iter()
        .map(|m| ProfileSummary {
            user_id: m.id,
            handle: m.handle.clone(),
            display_name: m.display_name.clone(),
            avatar_url: avatar_url_for(&m),
            avatar_icon_data: avatar_icon_data_for(&m),
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

    let model = queries::find_user_by_handle(&state.db, &handle)
        .await?
        .ok_or_else(|| AppError::NotFound("Profile not found".to_string()))?;

    if !model.active {
        return Err(AppError::NotFound("Profile not found".to_string()));
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
        avatar_icon_data: avatar_icon_data_for(&model),
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
    let _ = file_mime;

    let max_input_dimension = state
        .settings
        .get_max_image_dimension()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    let (normalized, icon) =
        tokio::task::spawn_blocking(move || normalize_avatar_bytes(&bytes, max_input_dimension))
            .await
            .map_err(|e| AppError::InternalError(format!("avatar worker join failed: {}", e)))?
            .map_err(AppError::ValidationError)?;

    let new_key = format!("avatars/{}/{}.webp", user_auth.id, Uuid::new_v4());
    state
        .s3
        .put_object_at(&new_key, normalized, "image/webp")
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to upload avatar: {:#}", e)))?;

    // Swap the row's avatar_s3_key and inline icon bytes. Best-effort delete
    // the previous object after the row update commits; a stale object is
    // harmless storage, a missing row pointer would break image loads.
    let prev_key = {
        let model = queries::find_user(&state.db, user_auth.id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
        let prev = model.avatar_s3_key.clone();
        let mut active: user::ActiveModel = model.into();
        active.avatar_s3_key = Set(Some(new_key.clone()));
        active.avatar_icon_data = Set(Some(icon));
        queries::update_user(&state.db, active).await?;
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
    let model = queries::find_user(&state.db, user_auth.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let prev = model.avatar_s3_key.clone();
    if prev.is_none() {
        return Ok(StatusCode::NO_CONTENT);
    }

    let mut active: user::ActiveModel = model.into();
    active.avatar_s3_key = Set(None);
    active.avatar_icon_data = Set(None);
    queries::update_user(&state.db, active).await?;

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

    let model = queries::find_user(&state.db, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Avatar not found".to_string()))?;
    if !model.active {
        return Err(AppError::NotFound("Avatar not found".to_string()));
    }
    let key = model
        .avatar_s3_key
        .ok_or_else(|| AppError::NotFound("Avatar not found".to_string()))?;

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

/// Anonymous reads of profiles and avatars are gated by the same
/// `public_feed_enabled` switch as the feed itself. Settings read
/// failures fail closed.
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
    Err(AppError::NotFound("Profile not found".to_string()))
}

/// Decode arbitrary input bytes, center-crop to a square, and emit two
/// WebP encodings: the full `AVATAR_OUTPUT_SIDE` avatar (uploaded to S3)
/// and an `AVATAR_ICON_SIDE` icon (embedded inline on user responses so
/// list views render without per-row avatar fetches). `max_input_dimension`
/// is enforced from the header before the full decode, so an oversized
/// image never allocates the RGBA buffer (an NxN decode is ~4*N^2 bytes
/// resident).
fn normalize_avatar_bytes(
    input: &[u8],
    max_input_dimension: u32,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    use image::imageops::{crop_imm, resize, FilterType};
    use image::ImageReader;

    let header_reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|e| format!("Could not read image: {}", e))?;
    let (w, h) = header_reader
        .into_dimensions()
        .map_err(|e| format!("Could not read image dimensions: {}", e))?;
    if w == 0 || h == 0 {
        return Err("Image has zero dimension".to_string());
    }
    if w > max_input_dimension || h > max_input_dimension {
        return Err(format!(
            "Image dimensions exceed {}x{}",
            max_input_dimension, max_input_dimension
        ));
    }

    let decoder = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|e| format!("Could not read image: {}", e))?;
    let img = decoder
        .decode()
        .map_err(|e| format!("Could not decode image: {}", e))?;

    let side = w.min(h);
    let x_off = (w - side) / 2;
    let y_off = (h - side) / 2;
    let square = crop_imm(&img, x_off, y_off, side, side).to_image();

    let avatar_img = if side != AVATAR_OUTPUT_SIDE {
        resize(
            &square,
            AVATAR_OUTPUT_SIDE,
            AVATAR_OUTPUT_SIDE,
            FilterType::Lanczos3,
        )
    } else {
        square.clone()
    };
    let icon_img = resize(
        &square,
        AVATAR_ICON_SIDE,
        AVATAR_ICON_SIDE,
        FilterType::Lanczos3,
    );

    Ok((
        encode_avatar_webp(&avatar_img)?,
        encode_avatar_webp(&icon_img)?,
    ))
}

fn encode_avatar_webp(img: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        img.width(),
        img.height(),
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
        let (webp, icon) = normalize_avatar_bytes(&png, 8000).unwrap();
        let img = image::load_from_memory(&webp).unwrap();
        assert_eq!(img.width(), AVATAR_OUTPUT_SIDE);
        assert_eq!(img.height(), AVATAR_OUTPUT_SIDE);
        let icon_img = image::load_from_memory(&icon).unwrap();
        assert_eq!(icon_img.width(), AVATAR_ICON_SIDE);
        assert_eq!(icon_img.height(), AVATAR_ICON_SIDE);
    }

    #[test]
    fn normalize_rejects_garbage() {
        let res = normalize_avatar_bytes(b"not an image", 8000);
        assert!(res.is_err());
    }

    #[test]
    fn normalize_rejects_oversized() {
        let png = synth_png(1024);
        let res = normalize_avatar_bytes(&png, 512);
        assert!(res.is_err());
    }
}
