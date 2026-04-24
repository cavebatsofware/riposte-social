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
use crate::admin::pagination::{Paginated, PaginationParams};
use crate::entities::{access_code, access_log, AccessCode, AccessLog};
use crate::errors::AppResult;
use crate::middleware::AuthenticatedUser;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, Order, PaginatorTrait, QueryFilter, QueryOrder,
};
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct AccessLogState {
    pub db: DatabaseConnection,
}

pub fn access_log_routes() -> Router<AccessLogState> {
    Router::new()
        .route("/api/admin/access-logs", get(list_logs).delete(clear_logs))
        .route("/api/admin/dashboard/metrics", get(get_dashboard_metrics))
}

#[derive(Serialize)]
struct AccessLogResponse {
    id: Uuid,
    access_code: String,
    ip_address: Option<String>,
    user_agent: Option<String>,
    tokens: Option<f64>,
    last_access_time: Option<String>,
    action: String,
    success: bool,
    admin_user_id: Option<String>,
    admin_user_email: Option<String>,
    created_at: String,
}

impl From<access_log::Model> for AccessLogResponse {
    fn from(model: access_log::Model) -> Self {
        let utc = Utc;
        Self {
            id: model.id,
            access_code: model.access_code,
            ip_address: model.ip_address,
            user_agent: model.user_agent,
            tokens: model.tokens,
            last_access_time: model
                .last_access_time
                .map(|dt| dt.with_timezone(&utc).to_rfc3339()),
            action: model.action,
            success: model.success,
            admin_user_id: model.admin_user_id.map(|id| id.to_string()),
            admin_user_email: model.admin_user_email,
            created_at: model.created_at.with_timezone(&utc).to_rfc3339(),
        }
    }
}

async fn list_logs(
    State(state): State<AccessLogState>,
    _user: AuthenticatedUser,
    Query(params): Query<PaginationParams>,
) -> AppResult<Json<Paginated<AccessLogResponse>>> {
    let validated = params.validate();

    let paginator = AccessLog::find()
        .order_by(access_log::Column::CreatedAt, Order::Desc)
        .paginate(&state.db, validated.per_page);

    let total = paginator.num_items().await?;
    let total_pages = paginator.num_pages().await?;
    let logs = paginator.fetch_page(validated.page - 1).await?;
    let log_responses: Vec<AccessLogResponse> = logs.into_iter().map(Into::into).collect();

    Ok(Json(Paginated::new(
        log_responses,
        total,
        validated.page,
        validated.per_page,
        total_pages,
    )))
}

async fn clear_logs(
    State(state): State<AccessLogState>,
    _user: AuthenticatedUser,
) -> AppResult<StatusCode> {
    AccessLog::delete_many().exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct HourlyAccessData {
    hour: String,
    count: i64,
}

#[derive(Serialize)]
struct RecentCodeData {
    id: String,
    name: String,
    count: i32,
}

#[derive(Serialize)]
struct DashboardMetrics {
    hourly_access_rates: Vec<HourlyAccessData>,
    recent_access_codes: Vec<RecentCodeData>,
}

async fn get_dashboard_metrics(
    State(state): State<AccessLogState>,
    _user: AuthenticatedUser,
) -> AppResult<Json<DashboardMetrics>> {
    let now = Utc::now();
    let twenty_four_hours_ago = now - Duration::hours(24);

    // Get successful access logs from the last 24 hours for hourly metrics
    let logs = AccessLog::find()
        .filter(access_log::Column::CreatedAt.gte(twenty_four_hours_ago))
        .filter(access_log::Column::Success.eq(true))
        .all(&state.db)
        .await?;

    // Build hourly access rate map
    let mut hourly_map = HashMap::new();
    for log in logs {
        let hour_key = log
            .created_at
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:00")
            .to_string();
        *hourly_map.entry(hour_key).or_insert(0i64) += 1;
    }

    // Generate complete 24-hour time series (fills gaps with 0)
    let hourly_access_rates: Vec<HourlyAccessData> = (0..24)
        .rev()
        .map(|hours_ago| {
            let hour_time = now - Duration::hours(hours_ago);
            let hour_key = hour_time.format("%Y-%m-%d %H:00").to_string();
            let count = hourly_map.get(&hour_key).copied().unwrap_or(0);

            HourlyAccessData {
                hour: hour_time.format("%H:%M").to_string(),
                count,
            }
        })
        .collect();

    // Get top 10 most-used access codes
    let mut access_codes: Vec<RecentCodeData> = AccessCode::find()
        .filter(access_code::Column::LastUsedAt.gte(twenty_four_hours_ago))
        .filter(access_code::Column::UsageCount.gt(0))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|code| RecentCodeData {
            id: code.id.to_string(),
            name: code.name,
            count: code.usage_count,
        })
        .collect();

    access_codes.sort_unstable_by(|a, b| b.count.cmp(&a.count));
    access_codes.truncate(10);

    Ok(Json(DashboardMetrics {
        hourly_access_rates,
        recent_access_codes: access_codes,
    }))
}
