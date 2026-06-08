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
//! Admin read view of customer orders (business module). Lists orders newest
//! first, decrypting the customer PII for display. Mirrors the access-log admin
//! list; behind admin auth + the `business` feature.
use crate::admin::pagination::{Paginated, PaginationParams};
use crate::crypto;
use crate::entities::{order, Order};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::settings::SettingsService;
use axum::{
    extract::{Path, Query, State},
    routing::{get, put},
    Json, Router,
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order as SortOrder,
    PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct OrdersAdminState {
    pub db: DatabaseConnection,
    pub settings: SettingsService,
}

pub fn orders_admin_routes() -> Router<OrdersAdminState> {
    Router::new()
        .route("/api/admin/orders", get(list_orders))
        .route("/api/admin/orders/statuses", get(list_order_statuses))
        .route("/api/admin/orders/{id}/status", put(update_order_status))
}

#[derive(Serialize)]
struct OrderAdminResponse {
    id: Uuid,
    kind: String,
    title: String,
    summary: String,
    /// Decrypted for display.
    customer_name: String,
    /// Decrypted for display.
    customer_phone: String,
    /// Decrypted for display, when present.
    customer_email: Option<String>,
    estimate_total: Option<f64>,
    total: Option<f64>,
    deposit: Option<f64>,
    /// JSON string of the form-specific selection (no PII).
    details: String,
    status: String,
    phone_verified: Option<bool>,
    phone_line_type: Option<String>,
    created_at: String,
}

/// Decrypt a ciphertext PII field, surfacing a marker rather than failing the
/// whole listing if a single row can't be read (e.g. key rotation).
fn decrypt_field(ciphertext: &str) -> String {
    crypto::decrypt_value(ciphertext).unwrap_or_else(|_| "[unreadable]".to_string())
}

impl From<order::Model> for OrderAdminResponse {
    fn from(model: order::Model) -> Self {
        Self {
            id: model.id,
            kind: model.kind,
            title: model.title,
            summary: model.summary,
            customer_name: decrypt_field(&model.customer_name),
            customer_phone: decrypt_field(&model.customer_phone),
            customer_email: model.customer_email.as_deref().map(decrypt_field),
            estimate_total: model.estimate_total,
            total: model.total,
            deposit: model.deposit,
            details: model.details,
            status: model.status,
            phone_verified: model.phone_verified,
            phone_line_type: model.phone_line_type,
            created_at: model.created_at.with_timezone(&Utc).to_rfc3339(),
        }
    }
}

/// Optional list filters (on non-encrypted columns only). Customer fields are
/// ciphertext, so they cannot be filtered server-side.
#[derive(Deserialize)]
struct OrderFilters {
    status: Option<String>,
    kind: Option<String>,
}

async fn list_orders(
    State(state): State<OrdersAdminState>,
    _user: AuthenticatedUser,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<OrderFilters>,
) -> AppResult<Json<Paginated<OrderAdminResponse>>> {
    let validated = params.validate();

    let mut query = Order::find().order_by(order::Column::CreatedAt, SortOrder::Desc);
    if let Some(status) = filters.status.as_deref().filter(|s| !s.is_empty()) {
        query = query.filter(order::Column::Status.eq(status));
    }
    if let Some(kind) = filters.kind.as_deref().filter(|k| !k.is_empty()) {
        query = query.filter(order::Column::Kind.eq(kind));
    }
    let paginator = query.paginate(&state.db, validated.per_page);

    let total = paginator.num_items().await?;
    let total_pages = paginator.num_pages().await?;
    let orders = paginator.fetch_page(validated.page - 1).await?;
    let responses: Vec<OrderAdminResponse> = orders.into_iter().map(Into::into).collect();

    Ok(Json(Paginated::new(
        responses,
        total,
        validated.page,
        validated.per_page,
        total_pages,
    )))
}

#[derive(Serialize)]
struct StatusesResponse {
    statuses: Vec<String>,
}

/// The configurable order-status workflow, for populating the admin dropdown.
async fn list_order_statuses(
    State(state): State<OrdersAdminState>,
    _user: AuthenticatedUser,
) -> AppResult<Json<StatusesResponse>> {
    let statuses = state
        .settings
        .get_order_statuses()
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    Ok(Json(StatusesResponse { statuses }))
}

#[derive(Deserialize)]
struct UpdateStatusRequest {
    status: String,
}

/// Move an order to a new status. The status must be one of the configured
/// workflow values.
async fn update_order_status(
    State(state): State<OrdersAdminState>,
    _user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStatusRequest>,
) -> AppResult<Json<OrderAdminResponse>> {
    let status = req.status.trim().to_string();
    let allowed = state
        .settings
        .get_order_statuses()
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    if !allowed.iter().any(|s| s == &status) {
        return Err(AppError::ValidationError(format!(
            "Unknown order status: {status}"
        )));
    }

    let order = Order::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Order not found".to_string()))?;

    let mut active: order::ActiveModel = order.into();
    active.status = Set(status);
    active.updated_at = Set(Utc::now().into());
    let updated = active.update(&state.db).await?;

    Ok(Json(updated.into()))
}
