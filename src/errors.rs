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
// Custom error types for better error handling and user experience

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use thiserror::Error;

/// Application-specific errors with proper HTTP status codes
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("File system error: {0}")]
    FileSystem(#[from] std::io::Error),

    #[error("Invalid access code")]
    InvalidAccess,

    #[error("Authentication error: {0}")]
    AuthError(String),

    /// 403 for requests that are well-formed and authenticated but denied
    /// by policy. Use this for feature flags ("X is currently disabled by
    /// an administrator"), role/permission gates that reject the request
    /// rather than hide the resource, and any other authorization-level
    /// denial that is not a credential-verification failure.
    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// 404 for missing or hidden domain entities. Use this when a row
    /// doesn't exist OR exists but the caller isn't permitted to see it,
    /// returning the same response in both cases so existence isn't
    /// disclosed. Distinct from `AuthError` (401), which belongs to
    /// credential-verification flows (login, MFA, password reset).
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    /// 409 for a well-formed request that conflicts with the current state,
    /// such as a concurrent write that trips a unique constraint and can
    /// succeed on retry.
    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Database(ref e) => {
                tracing::error!("Database error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::FileSystem(ref e) => {
                tracing::error!("File system error: {}", e);
                (StatusCode::NOT_FOUND, "Resource not found".to_string())
            }
            AppError::InvalidAccess => {
                tracing::warn!("Invalid access attempt");
                (StatusCode::NOT_FOUND, "Not found".to_string())
            }
            AppError::AuthError(msg) => {
                tracing::warn!("Authentication error: {}", msg);
                (StatusCode::UNAUTHORIZED, msg)
            }
            AppError::Forbidden(msg) => {
                tracing::debug!("Forbidden: {}", msg);
                (StatusCode::FORBIDDEN, msg)
            }
            AppError::NotFound(msg) => {
                tracing::debug!("Not found: {}", msg);
                (StatusCode::NOT_FOUND, msg)
            }
            AppError::ValidationError(msg) => {
                tracing::debug!("Validation error: {}", msg);
                (StatusCode::BAD_REQUEST, msg)
            }
            AppError::Conflict(msg) => {
                tracing::debug!("Conflict: {}", msg);
                (StatusCode::CONFLICT, msg)
            }
            AppError::InternalError(ref e) => {
                tracing::error!("Internal error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        (
            status,
            Json(ErrorResponse {
                error: error_message,
            }),
        )
            .into_response()
    }
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;
