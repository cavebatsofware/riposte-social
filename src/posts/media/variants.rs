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
//! Thumbnail + icon generation for uploaded image media.
//!
//! Runs after `build_media_plan` and before `upload_media`: decodes each
//! image once on a blocking worker, captures its native dimensions, and
//! emits two derived WebP sizes (400px thumbnail, 64px icon) that get
//! stored alongside the row and surfaced as base64 data URIs in API
//! responses. Video items pass through untouched.

use crate::errors::{AppError, AppResult};
use crate::posts::media::{is_video_mime, PlannedUpload};
use crate::settings::SettingsService;
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader};
use std::io::Cursor;
use std::mem;

const THUMBNAIL_MAX_PX: u32 = 400;
const ICON_MAX_PX: u32 = 64;

pub(crate) struct ImageVariants {
    pub width: i32,
    pub height: i32,
    pub thumbnail: Vec<u8>,
    pub icon: Vec<u8>,
}

fn longest_edge_resize(img: &DynamicImage, max_px: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_px && h <= max_px {
        return img.clone();
    }
    let longest = w.max(h) as f32;
    let scale = max_px as f32 / longest;
    let new_w = ((w as f32) * scale).round().max(1.0) as u32;
    let new_h = ((h as f32) * scale).round().max(1.0) as u32;
    img.resize(new_w, new_h, FilterType::Lanczos3)
}

fn encode_webp_lossless(img: &DynamicImage) -> Result<Vec<u8>, String> {
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
    image::ImageEncoder::write_image(
        encoder,
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("Could not encode webp: {}", e))?;
    Ok(out)
}

/// Decode `input` once and produce a 400px WebP thumbnail + 64px WebP icon
/// plus the native pixel dimensions. `max_input_dimension` rejects oversized
/// inputs before decode to bound peak memory; an NxN RGBA decode is ~4*N^2
/// bytes resident. Synchronous and CPU-bound: callers must invoke from a
/// blocking context (`spawn_blocking` or an importer worker thread). Video
/// items must be filtered out upstream; this fails on anything `image` can't
/// decode.
pub(crate) fn generate_variants_blocking(
    input: &[u8],
    max_input_dimension: u32,
) -> Result<ImageVariants, String> {
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
    if w > max_input_dimension || h > max_input_dimension {
        return Err(format!(
            "Image dimensions exceed {}x{}",
            max_input_dimension, max_input_dimension
        ));
    }

    let thumbnail = encode_webp_lossless(&longest_edge_resize(&img, THUMBNAIL_MAX_PX))?;
    let icon = encode_webp_lossless(&longest_edge_resize(&img, ICON_MAX_PX))?;

    Ok(ImageVariants {
        width: w as i32,
        height: h as i32,
        thumbnail,
        icon,
    })
}

/// Generate width/height + thumbnail + icon for every image item in `plan`,
/// in-place. Video items are skipped. Image processing runs on a blocking
/// worker so the async runtime isn't blocked by libwebp encoding. The input
/// bytes are moved into the worker (not cloned) and written back after to
/// keep peak memory bounded for multi-image albums.
pub(crate) async fn process_image_variants(
    plan: &mut [PlannedUpload],
    settings: &SettingsService,
) -> AppResult<()> {
    let max_input_dimension = settings
        .get_max_image_dimension()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;

    let image_inputs: Vec<(usize, Vec<u8>)> = plan
        .iter_mut()
        .enumerate()
        .filter(|(_, p)| !is_video_mime(&p.media.mime_type))
        .map(|(i, p)| (i, mem::take(&mut p.media.bytes)))
        .collect();

    if image_inputs.is_empty() {
        return Ok(());
    }

    let results = tokio::task::spawn_blocking(move || {
        image_inputs
            .into_iter()
            .map(|(i, bytes)| {
                let res = generate_variants_blocking(&bytes, max_input_dimension);
                (i, bytes, res)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| AppError::InternalError(format!("variant worker join failed: {}", e)))?;

    // Restore every byte buffer first so a failure on item N doesn't leave
    // items N+1.. with empty `media.bytes`. The caller may inspect the plan
    // on error (e.g. to log filenames), so leaving it consistent matters.
    let mut variants_out: Vec<(usize, Result<ImageVariants, String>)> =
        Vec::with_capacity(results.len());
    for (i, bytes, res) in results {
        plan[i].media.bytes = bytes;
        variants_out.push((i, res));
    }

    for (i, res) in variants_out {
        let variants = res.map_err(AppError::ValidationError)?;
        plan[i].width = Some(variants.width);
        plan[i].height = Some(variants.height);
        plan[i].thumbnail_data = Some(variants.thumbnail);
        plan[i].icon_data = Some(variants.icon);
    }

    Ok(())
}
