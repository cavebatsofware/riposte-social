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
use crate::errors::AppResult;
use crate::middleware::AuthenticatedUser;
use crate::settings::SettingsService;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, put},
    Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SECRET_KEY_PREFIX: &str = "secret_";

#[derive(Clone)]
pub struct SettingsState {
    pub settings: SettingsService,
}

pub fn settings_routes() -> Router<SettingsState> {
    Router::new()
        .route("/api/admin/settings", get(get_all_settings))
        .route("/api/admin/settings", put(update_setting))
}

#[derive(Serialize)]
struct SettingResponse {
    id: Uuid,
    key: String,
    /// Empty string for encrypted rows: the admin UI never receives
    /// the ciphertext or a decrypted value, only a flag that one is
    /// set. Admins replace, they don't re-read.
    value: String,
    category: Option<String>,
    encrypted: bool,
    /// True when the row is encrypted and has a stored value. Lets
    /// the UI render "(set)" vs an empty password field.
    has_value: bool,
}

async fn get_all_settings(
    State(state): State<SettingsState>,
    _user: AuthenticatedUser,
) -> AppResult<Json<Vec<SettingResponse>>> {
    let settings = state.settings.get_all().await.map_err(|e| {
        tracing::error!("Failed to get settings from database: {}", e);
        crate::errors::AppError::InternalError(e.to_string())
    })?;

    let responses: Vec<SettingResponse> = settings
        .into_iter()
        .map(|s| {
            let has_value = !s.value.is_empty();
            let value = if s.encrypted { String::new() } else { s.value };
            SettingResponse {
                id: s.id,
                key: s.key,
                value,
                category: s.category,
                encrypted: s.encrypted,
                has_value,
            }
        })
        .collect();

    Ok(Json(responses))
}

#[derive(Deserialize)]
struct UpdateSettingRequest {
    key: String,
    value: String,
    category: Option<String>,
}

async fn update_setting(
    State(state): State<SettingsState>,
    _user: AuthenticatedUser,
    Json(req): Json<UpdateSettingRequest>,
) -> AppResult<StatusCode> {
    let is_secret = req.key.starts_with(SECRET_KEY_PREFIX);

    let result = if is_secret {
        state
            .settings
            .set_encrypted(&req.key, &req.value, req.category.as_deref(), None)
            .await
    } else {
        state
            .settings
            .set(&req.key, &req.value, req.category.as_deref(), None)
            .await
    };

    result.map_err(|e| crate::errors::AppError::InternalError(e.to_string()))?;

    Ok(StatusCode::OK)
}
