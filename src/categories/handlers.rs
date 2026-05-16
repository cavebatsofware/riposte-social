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
//! HTTP handlers for categories.
//!
//! Read endpoint (`GET /api/categories`) is public; the response is
//! filtered to categories the caller can read into via `ViewerCtx`.
//!
//! Write endpoints all live under `/api/categories/*`. Each handler runs
//! `can_create_category` / `can_manage_category` against the caller; non-admin
//! posters are also subject to the `poster_category_management_enabled`
//! global gate.

use crate::admin::UserAuth;
use crate::categories::queries;
use crate::categories::types::{
    into_response, AddMemberRequest, CategoriesListResponse, CategoryResponse,
    CreateCategoryRequest, MemberResponse, MembersListResponse, ReplaceMembersRequest,
    UpdateCategoryRequest,
};
use crate::categories::{
    can_create_category, can_manage_category, slugify, validate_slug_shape, CategoriesState,
};
use crate::entities::category;
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::visibility::{is_valid_category_visibility, VISIBILITY_USER_LIST};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use sea_orm::{Set, TransactionTrait};
use uuid::Uuid;

pub fn public_category_routes() -> Router<CategoriesState> {
    Router::new().route("/api/categories", get(list_categories))
}

pub fn category_management_routes() -> Router<CategoriesState> {
    Router::new()
        .route("/api/categories", post(create_category))
        .route(
            "/api/categories/{id}",
            axum::routing::patch(update_category).delete(delete_category),
        )
        .route(
            "/api/categories/{id}/members",
            get(list_members).put(replace_members).post(add_member),
        )
        .route(
            "/api/categories/{id}/members/{user_id}",
            axum::routing::delete(remove_member),
        )
}

async fn list_categories(
    State(state): State<CategoriesState>,
    auth_session: UserAuthSession,
) -> AppResult<Json<CategoriesListResponse>> {
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    let user = auth_session.user().await;
    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;

    let rows = queries::list_categories_ordered(&state.db).await?;

    let visible: Vec<category::Model> = rows
        .into_iter()
        .filter(|c| ctx.can_view_category(c))
        .collect();

    let categories = visible
        .into_iter()
        .map(|c| {
            let manageable = match user.as_ref() {
                Some(u) => can_manage_category(u, &c, gate_enabled),
                None => false,
            };
            into_response(c, manageable)
        })
        .collect();
    Ok(Json(CategoriesListResponse { categories }))
}

async fn create_category(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Json(req): Json<CreateCategoryRequest>,
) -> AppResult<(StatusCode, Json<CategoryResponse>)> {
    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_create_category(&user, gate_enabled) {
        return Err(AppError::Forbidden(
            "Not allowed to create categories".to_string(),
        ));
    }

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::ValidationError(
            "Name cannot be empty".to_string(),
        ));
    }
    if name.chars().count() > 80 {
        return Err(AppError::ValidationError(
            "Name must be at most 80 characters".to_string(),
        ));
    }

    let slug = match req.slug {
        Some(s) => {
            let trimmed = s.trim().to_string();
            validate_slug_shape(&trimmed).map_err(AppError::ValidationError)?;
            trimmed
        }
        None => {
            let derived = slugify(&name);
            validate_slug_shape(&derived).map_err(|_| {
                AppError::ValidationError(
                    "Could not derive a slug from name. Provide one explicitly.".to_string(),
                )
            })?;
            derived
        }
    };

    let visibility = req
        .visibility
        .as_deref()
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| crate::entities::post::VISIBILITY_PUBLIC.to_string());
    if !is_valid_category_visibility(&visibility) {
        return Err(AppError::ValidationError(format!(
            "Invalid visibility '{}'",
            visibility
        )));
    }

    if queries::find_category_by_name(&state.db, &name)
        .await?
        .is_some()
    {
        return Err(AppError::ValidationError(
            "A category with that name already exists".to_string(),
        ));
    }
    if queries::find_category_by_slug(&state.db, &slug)
        .await?
        .is_some()
    {
        return Err(AppError::ValidationError(
            "A category with that slug already exists".to_string(),
        ));
    }

    let txn = state.db.begin().await?;
    let active = category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name),
        slug: Set(slug),
        ordinal: Set(req.ordinal.unwrap_or(0)),
        color: Set(req.color.filter(|s| !s.is_empty())),
        visibility: Set(visibility.clone()),
        created_by: Set(Some(user.id)),
        ..Default::default()
    };
    let row = queries::insert_category(&txn, active).await?;

    if visibility == VISIBILITY_USER_LIST {
        queries::insert_member(&txn, row.id, user.id).await?;
    }
    txn.commit().await?;
    Ok((StatusCode::CREATED, Json(into_response(row, true))))
}

async fn update_category(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCategoryRequest>,
) -> AppResult<Json<CategoryResponse>> {
    let row = queries::find_category(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Category not found".to_string()))?;

    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_manage_category(&user, &row, gate_enabled) {
        return Err(AppError::Forbidden(
            "Not allowed to modify this category".to_string(),
        ));
    }

    if let Some(ref s) = req.slug {
        validate_slug_shape(s.trim()).map_err(AppError::ValidationError)?;
    }

    if let Some(ref v) = req.visibility {
        if !is_valid_category_visibility(v.trim()) {
            return Err(AppError::ValidationError(format!(
                "Invalid visibility '{}'",
                v
            )));
        }
    }

    if let Some(ref name) = req.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AppError::ValidationError(
                "Name cannot be empty".to_string(),
            ));
        }
        if trimmed != row.name
            && queries::find_category_by_name_excluding(&state.db, trimmed, id)
                .await?
                .is_some()
        {
            return Err(AppError::ValidationError(
                "A category with that name already exists".to_string(),
            ));
        }
    }
    if let Some(ref slug) = req.slug {
        let trimmed = slug.trim();
        if trimmed != row.slug
            && queries::find_category_by_slug_excluding(&state.db, trimmed, id)
                .await?
                .is_some()
        {
            return Err(AppError::ValidationError(
                "A category with that slug already exists".to_string(),
            ));
        }
    }

    let new_visibility: Option<String> = req.visibility.as_ref().map(|v| v.trim().to_string());
    let resulting_visibility = new_visibility
        .clone()
        .unwrap_or_else(|| row.visibility.clone());

    let txn = state.db.begin().await?;

    let mut active: category::ActiveModel = row.clone().into();
    if let Some(name) = req.name {
        active.name = Set(name.trim().to_string());
    }
    if let Some(slug) = req.slug {
        active.slug = Set(slug.trim().to_string());
    }
    if let Some(o) = req.ordinal {
        active.ordinal = Set(o);
    }
    if let Some(c) = req.color {
        let trimmed = c.trim();
        active.color = Set(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        });
    }
    if let Some(v) = req.visibility {
        active.visibility = Set(v.trim().to_string());
    }
    let updated = queries::update_category(&txn, active).await?;

    // Cycling user_list -> public -> user_list can leave a stale member
    // row from the first round, so the existence check keeps the insert
    // idempotent against the (category_id, user_id) composite PK.
    if resulting_visibility == VISIBILITY_USER_LIST {
        if let Some(owner_id) = row.created_by {
            if !queries::member_exists(&txn, updated.id, owner_id).await? {
                queries::insert_member(&txn, updated.id, owner_id).await?;
            }
        }
    }

    txn.commit().await?;
    Ok(Json(into_response(updated, true)))
}

async fn delete_category(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = queries::find_category(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Category not found".to_string()))?;

    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_manage_category(&user, &row, gate_enabled) {
        return Err(AppError::Forbidden(
            "Not allowed to delete this category".to_string(),
        ));
    }

    queries::delete_category(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_can_manage(
    state: &CategoriesState,
    user: &UserAuth,
    id: Uuid,
) -> AppResult<category::Model> {
    let row = queries::find_category(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Category not found".to_string()))?;
    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_manage_category(user, &row, gate_enabled) {
        return Err(AppError::Forbidden(
            "Not allowed to manage this category".to_string(),
        ));
    }
    Ok(row)
}

async fn list_members(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<MembersListResponse>> {
    ensure_can_manage(&state, &user, id).await?;

    let rows = queries::list_members(&state.db, id).await?;
    let user_ids: Vec<Uuid> = rows.iter().map(|r| r.user_id).collect();
    let by_id = queries::load_users_by_ids(&state.db, user_ids).await?;

    let members = rows
        .into_iter()
        .filter_map(|m| {
            let u = by_id.get(&m.user_id)?;
            Some(MemberResponse {
                user_id: u.id,
                handle: u.handle.clone(),
                display_name: u.display_name.clone(),
                created_at: m.created_at.with_timezone(&chrono::Utc).to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(MembersListResponse { members }))
}

async fn replace_members(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<ReplaceMembersRequest>,
) -> AppResult<Json<MembersListResponse>> {
    let cat = ensure_can_manage(&state, &user, id).await?;

    let mut want = req.user_ids.clone();
    want.sort();
    want.dedup();

    if let Some(creator_id) = cat.created_by {
        if !want.contains(&creator_id) {
            return Err(AppError::ValidationError(
                "the category creator cannot be removed".to_string(),
            ));
        }
    }

    let txn = state.db.begin().await?;
    queries::delete_all_members(&txn, id).await?;
    for uid in &want {
        queries::insert_member(&txn, id, *uid).await?;
    }
    txn.commit().await?;

    list_members(State(state), Extension(user), Path(id)).await
}

async fn add_member(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> AppResult<Json<MembersListResponse>> {
    ensure_can_manage(&state, &user, id).await?;

    if !queries::member_exists(&state.db, id, req.user_id).await? {
        if !queries::user_exists(&state.db, req.user_id).await? {
            return Err(AppError::ValidationError("User not found".to_string()));
        }
        queries::insert_member(&state.db, id, req.user_id).await?;
    }

    list_members(State(state), Extension(user), Path(id)).await
}

async fn remove_member(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path((id, member_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    let cat = ensure_can_manage(&state, &user, id).await?;
    if cat.created_by == Some(member_id) {
        return Err(AppError::ValidationError(
            "the category creator cannot be removed".to_string(),
        ));
    }
    queries::delete_member(&state.db, id, member_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
