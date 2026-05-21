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
//! Admin HTTP endpoints for import jobs.
//!
//! - `POST /api/admin/imports/facebook` (multipart): accept a Facebook
//!   archive ZIP, stream it to a tempfile, upload to S3, enqueue the job
//!   row, and `tokio::spawn` the worker. Returns the job id immediately
//!   so the admin UI can start polling without holding the request open
//!   for the duration of the import.
//! - `GET /api/admin/imports`: paginated list of recent jobs, newest first.
//! - `GET /api/admin/imports/{id}`: single job status.

use crate::admin::pagination::{Paginated, PaginationParams};
use crate::admin::UserAuth;
use crate::entities::{import_job, post, ImportJob};
use crate::errors::{AppError, AppResult};
use crate::imports::{
    self,
    facebook::{run_facebook_import, FacebookImportParams},
    types::ImportJobResponse,
};
use crate::s3::S3Service;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use sea_orm::{DatabaseConnection, EntityTrait, Order, PaginatorTrait, QueryOrder};
use std::io::Write;
use uuid::Uuid;

/// Hard cap on the uploaded archive. FB archives can run into the gigabytes
/// for very long-lived accounts; this is intentionally generous but bounded
/// so a malformed client can't wedge the connection holding unbounded data.
const ARCHIVE_MAX_BYTES: usize = 4 * 1024 * 1024 * 1024;

#[derive(Clone)]
pub struct ImportsState {
    pub db: DatabaseConnection,
    pub s3: S3Service,
    pub settings: crate::settings::SettingsService,
}

pub fn imports_routes() -> Router<ImportsState> {
    Router::new()
        .route("/api/admin/imports", get(list_imports))
        .route("/api/admin/imports/{id}", get(get_import))
        .route(
            "/api/admin/imports/facebook",
            post(create_facebook_import).layer(DefaultBodyLimit::max(ARCHIVE_MAX_BYTES)),
        )
}

async fn list_imports(
    State(state): State<ImportsState>,
    Query(params): Query<PaginationParams>,
) -> AppResult<Json<Paginated<ImportJobResponse>>> {
    let pagination = params.validate();
    let paginator = ImportJob::find()
        .order_by(import_job::Column::CreatedAt, Order::Desc)
        .paginate(&state.db, pagination.per_page);
    let total = paginator.num_items().await?;
    let total_pages = paginator.num_pages().await?;
    let rows = paginator.fetch_page(pagination.page - 1).await?;
    let data = rows.into_iter().map(Into::into).collect();
    Ok(Json(Paginated::new(
        data,
        total,
        pagination.page,
        pagination.per_page,
        total_pages,
    )))
}

async fn get_import(
    State(state): State<ImportsState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ImportJobResponse>> {
    let row = ImportJob::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Import job not found".to_string()))?;
    Ok(Json(row.into()))
}

/// `POST /api/admin/imports/facebook`. Multipart fields:
/// - `archive` (file, required): the FB export ZIP
/// - `visibility` (text, optional): one of `post::VISIBILITY_*`. Defaults
///   to `public`.
async fn create_facebook_import(
    State(state): State<ImportsState>,
    Extension(user): Extension<UserAuth>,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<ImportJobResponse>)> {
    // Fail closed: settings read failures bubble up as 500s rather than
    // silently allowing imports during a transient DB problem.
    let fb_import_enabled = state
        .settings
        .get_fb_import_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !fb_import_enabled {
        return Err(AppError::Forbidden(
            "Facebook imports are currently disabled by an administrator".to_string(),
        ));
    }

    let mut staged_archive: Option<tempfile::NamedTempFile> = None;
    let mut original_filename: Option<String> = None;
    let mut visibility: String = post::VISIBILITY_PUBLIC.to_string();

    // Stream each multipart field. The `archive` file is written to a
    // tempfile chunk-by-chunk so we never hold the whole archive in
    // memory. Other fields are short text values read inline.
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("multipart parse failed: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "archive" => {
                original_filename = field.file_name().map(|s| s.to_string());
                let mut tmp = tempfile::Builder::new()
                    .prefix("riposte-fb-upload-")
                    .suffix(".zip")
                    .tempfile()
                    .map_err(|e| {
                        AppError::InternalError(format!("tempfile create failed: {}", e))
                    })?;
                while let Some(chunk) = field.chunk().await.map_err(|e| {
                    AppError::ValidationError(format!("upload chunk read failed: {}", e))
                })? {
                    tmp.write_all(&chunk).map_err(|e| {
                        AppError::InternalError(format!("tempfile write failed: {}", e))
                    })?;
                }
                tmp.flush().map_err(|e| {
                    AppError::InternalError(format!("tempfile flush failed: {}", e))
                })?;
                staged_archive = Some(tmp);
            }
            "visibility" => {
                visibility = field.text().await.map_err(|e| {
                    AppError::ValidationError(format!("visibility read failed: {}", e))
                })?;
            }
            _ => {
                // Forward-compatible: ignore unknown fields rather than
                // 400-ing so the frontend can add new fields cleanly.
            }
        }
    }

    let staged = staged_archive
        .ok_or_else(|| AppError::ValidationError("Missing required field: archive".to_string()))?;
    if !post::is_valid_visibility(&visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            visibility
        )));
    }

    let job_id = Uuid::new_v4();
    let s3_key = format!("imports/facebook/{}/archive.zip", job_id);
    let size_bytes = std::fs::metadata(staged.path()).ok().map(|m| m.len());

    state
        .s3
        .put_object_from_path(&s3_key, staged.path(), "application/zip")
        .await
        // `{:#}` walks the anyhow cause chain so the operator sees the
        // underlying network/credential/service reason in the response,
        // not just the SDK's top-level "dispatch failure" message.
        .map_err(|e| AppError::InternalError(format!("Failed to stage archive: {:#}", e)))?;
    // The local tempfile is no longer needed once the upload succeeded.
    drop(staged);

    let params = FacebookImportParams {
        s3_key: s3_key.clone(),
        visibility: visibility.clone(),
        original_filename,
        size_bytes,
    };
    let params_value = serde_json::to_value(&params)
        .map_err(|e| AppError::InternalError(format!("Failed to serialize params: {}", e)))?;

    // Insert the row with the pre-allocated id so the response carries the
    // same id the spawned worker will read.
    use sea_orm::{ActiveModelTrait, Set};
    let row = import_job::ActiveModel {
        id: Set(job_id),
        kind: Set(import_job::KIND_FACEBOOK.to_string()),
        status: Set(import_job::STATUS_QUEUED.to_string()),
        created_by: Set(user.id),
        parent_job_id: Set(None),
        params: Set(params_value),
        progress: Set(imports::JobProgress::default().into_value()),
        log: Set(imports::JobLog::default().into_value()),
        error: Set(None),
        last_heartbeat_at: Set(None),
        started_at: Set(None),
        finished_at: Set(None),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    crate::metrics::IMPORTS_TOTAL
        .with_label_values(&["started"])
        .inc();

    // Spawn the worker. Failures are logged + persisted to the row's
    // `error` field; the request returns the job id regardless.
    let db_clone = state.db.clone();
    let s3_clone = state.s3.clone();
    let settings_clone = state.settings.clone();
    let user_id = user.id;
    tokio::spawn(async move {
        match run_facebook_import(
            db_clone.clone(),
            s3_clone,
            settings_clone,
            job_id,
            params,
            user_id,
        )
        .await
        {
            Ok(summary) => {
                tracing::info!(
                    "facebook import {} done: {} succeeded, {} skipped, {} failed (of {})",
                    job_id,
                    summary.succeeded,
                    summary.skipped,
                    summary.failed,
                    summary.total,
                );
                crate::metrics::IMPORTS_TOTAL
                    .with_label_values(&["succeeded"])
                    .inc();
                let _ =
                    imports::finish(&db_clone, job_id, import_job::STATUS_SUCCEEDED, None).await;
            }
            Err(e) => {
                tracing::error!("facebook import {} failed: {:#}", job_id, e);
                crate::metrics::IMPORTS_TOTAL
                    .with_label_values(&["failed"])
                    .inc();
                let _ = imports::finish(
                    &db_clone,
                    job_id,
                    import_job::STATUS_FAILED,
                    Some(format!("{:#}", e)),
                )
                .await;
            }
        }
    });

    Ok((StatusCode::CREATED, Json(row.into())))
}
