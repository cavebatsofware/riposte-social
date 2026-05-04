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
//! Write endpoints all live under `/api/categories/*` (no longer
//! `/api/admin/*` since posters can manage their own categories). Each
//! handler runs `can_create_category` / `can_manage_category` against
//! the caller before doing anything; non-admin posters are also subject
//! to the `poster_category_management_enabled` global gate.

use crate::admin::UserAuth;
use crate::categories::{can_create_category, can_manage_category, slugify, validate_slug_shape};
use crate::entities::{category, category_member, user, Category, CategoryMember, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::admin_auth::UserAuthSession;
use crate::settings::SettingsService;
use crate::visibility::{is_valid_category_visibility, VISIBILITY_USER_LIST};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct CategoriesState {
    pub db: DatabaseConnection,
    pub settings: SettingsService,
}

pub fn public_category_routes() -> Router<CategoriesState> {
    Router::new().route("/api/categories", get(list_categories))
}

/// Authenticated category-management routes. Each handler runs its own
/// permission check (admin OR poster-with-gate-on AND owner). The route
/// layer just enforces "must be logged in".
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

// ==================== wire format ====================

#[derive(Serialize)]
pub struct CategoryResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub ordinal: i32,
    pub color: Option<String>,
    pub visibility: String,
    pub created_by: Option<Uuid>,
    /// Whether the caller can manage (edit / delete / change membership)
    /// this category. Lets the social-frontend show or hide Manage
    /// affordances per row without a second permission check.
    pub manageable: bool,
}

fn into_response(m: category::Model, manageable: bool) -> CategoryResponse {
    CategoryResponse {
        id: m.id,
        name: m.name,
        slug: m.slug,
        ordinal: m.ordinal,
        color: m.color,
        visibility: m.visibility,
        created_by: m.created_by,
        manageable,
    }
}

#[derive(Deserialize)]
pub struct CreateCategoryRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub ordinal: Option<i32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct UpdateCategoryRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub ordinal: Option<i32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Serialize)]
pub struct CategoriesListResponse {
    pub categories: Vec<CategoryResponse>,
}

#[derive(Serialize)]
pub struct MemberResponse {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct MembersListResponse {
    pub members: Vec<MemberResponse>,
}

#[derive(Deserialize)]
pub struct ReplaceMembersRequest {
    pub user_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
}

// ==================== handlers ====================

/// `GET /api/categories`. Public read; the visibility filter from
/// `ViewerCtx` is applied so callers don't see categories they can't
/// read into. Each response row carries a `manageable` flag the
/// frontend uses to show or hide management affordances.
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

    let rows = Category::find()
        .order_by_asc(category::Column::Ordinal)
        .order_by_asc(category::Column::Name)
        .all(&state.db)
        .await?;

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

/// `POST /api/categories`. Admin or poster (when the gate is on) creates
/// a new category. `created_by` is set to the caller.
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
        return Err(AppError::AuthError(
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

    if Category::find()
        .filter(category::Column::Name.eq(&name))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(AppError::ValidationError(
            "A category with that name already exists".to_string(),
        ));
    }
    if Category::find()
        .filter(category::Column::Slug.eq(&slug))
        .one(&state.db)
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
    let row = active.insert(&txn).await?;

    // Auto-add the creator as a member when the new category is
    // user_list. The category is brand new so there can be no existing
    // membership row for it; a plain insert is correct here.
    if visibility == VISIBILITY_USER_LIST {
        category_member::ActiveModel {
            category_id: Set(row.id),
            user_id: Set(user.id),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok((StatusCode::CREATED, Json(into_response(row, true))))
}

/// `PATCH /api/categories/{id}`. Manageable by admin or owner-when-gate-on.
async fn update_category(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCategoryRequest>,
) -> AppResult<Json<CategoryResponse>> {
    let row = Category::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Category not found".to_string()))?;

    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_manage_category(&user, &row, gate_enabled) {
        return Err(AppError::AuthError(
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
        if trimmed != row.name {
            let conflict = Category::find()
                .filter(category::Column::Name.eq(trimmed))
                .filter(category::Column::Id.ne(id))
                .one(&state.db)
                .await?;
            if conflict.is_some() {
                return Err(AppError::ValidationError(
                    "A category with that name already exists".to_string(),
                ));
            }
        }
    }
    if let Some(ref slug) = req.slug {
        let trimmed = slug.trim();
        if trimmed != row.slug {
            let conflict = Category::find()
                .filter(category::Column::Slug.eq(trimmed))
                .filter(category::Column::Id.ne(id))
                .one(&state.db)
                .await?;
            if conflict.is_some() {
                return Err(AppError::ValidationError(
                    "A category with that slug already exists".to_string(),
                ));
            }
        }
    }

    // Determine the new visibility (post-patch) up-front so we can
    // run any membership-side housekeeping in the same transaction.
    let new_visibility: Option<String> = req.visibility.as_ref().map(|v| v.trim().to_string());
    let resulting_visibility = new_visibility
        .clone()
        .unwrap_or_else(|| row.visibility.clone());

    // Determine the new owner for legacy rows (created_by IS NULL):
    // when an admin first patches such a row to a non-public tier,
    // claim ownership so the row has a manager going forward.
    let claim_ownership = row.created_by.is_none()
        && user.role == user::ROLE_ADMINISTRATOR
        && new_visibility
            .as_deref()
            .map(|v| v != crate::entities::post::VISIBILITY_PUBLIC)
            .unwrap_or(false);
    let resulting_owner: Option<Uuid> = if claim_ownership {
        Some(user.id)
    } else {
        row.created_by
    };

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
    if claim_ownership {
        active.created_by = Set(Some(user.id));
    }
    let updated = active.update(&txn).await?;

    // Auto-add the resulting owner to the user_list ACL when the
    // category is now user_list and they aren't already a member.
    // Cycling user_list -> public -> user_list can leave a stale member
    // row from the first round, so we existence-check before insert to
    // keep the operation idempotent against the (category_id, user_id)
    // composite PK.
    if resulting_visibility == VISIBILITY_USER_LIST {
        if let Some(owner_id) = resulting_owner {
            let already_member = CategoryMember::find()
                .filter(category_member::Column::CategoryId.eq(updated.id))
                .filter(category_member::Column::UserId.eq(owner_id))
                .one(&txn)
                .await?
                .is_some();
            if !already_member {
                category_member::ActiveModel {
                    category_id: Set(updated.id),
                    user_id: Set(owner_id),
                    ..Default::default()
                }
                .insert(&txn)
                .await?;
            }
        }
    }

    txn.commit().await?;
    Ok(Json(into_response(updated, true)))
}

/// `DELETE /api/categories/{id}`. Manageable. Posts/albums.category_id is
/// `ON DELETE SET NULL`, so deletion just unlinks rather than cascading.
async fn delete_category(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = Category::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Category not found".to_string()))?;

    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_manage_category(&user, &row, gate_enabled) {
        return Err(AppError::AuthError(
            "Not allowed to delete this category".to_string(),
        ));
    }

    Category::delete_by_id(id).exec(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ==================== membership ====================

async fn ensure_can_manage(
    state: &CategoriesState,
    user: &UserAuth,
    id: Uuid,
) -> AppResult<category::Model> {
    let row = Category::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Category not found".to_string()))?;
    let gate_enabled = state
        .settings
        .get_poster_category_management_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if !can_manage_category(user, &row, gate_enabled) {
        return Err(AppError::AuthError(
            "Not allowed to manage this category".to_string(),
        ));
    }
    Ok(row)
}

/// `GET /api/categories/{id}/members`. Manageable.
async fn list_members(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<MembersListResponse>> {
    ensure_can_manage(&state, &user, id).await?;

    let rows = CategoryMember::find()
        .filter(category_member::Column::CategoryId.eq(id))
        .all(&state.db)
        .await?;

    let user_ids: Vec<Uuid> = rows.iter().map(|r| r.user_id).collect();
    let users = if user_ids.is_empty() {
        vec![]
    } else {
        User::find()
            .filter(user::Column::Id.is_in(user_ids))
            .all(&state.db)
            .await?
    };
    let by_id: std::collections::HashMap<Uuid, user::Model> =
        users.into_iter().map(|u| (u.id, u)).collect();

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

/// `PUT /api/categories/{id}/members`. Manageable. Replaces the whole
/// list in one transaction. Rejects any payload that omits the
/// category's creator — the creator is always implicitly a member of
/// their own `user_list` category and cannot be removed while it
/// exists.
async fn replace_members(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<ReplaceMembersRequest>,
) -> AppResult<Json<MembersListResponse>> {
    let cat = ensure_can_manage(&state, &user, id).await?;

    // De-dup the input list before inserting; user-provided uniqueness
    // is not guaranteed.
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
    CategoryMember::delete_many()
        .filter(category_member::Column::CategoryId.eq(id))
        .exec(&txn)
        .await?;
    for uid in &want {
        category_member::ActiveModel {
            category_id: Set(id),
            user_id: Set(*uid),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
    }
    txn.commit().await?;

    list_members(State(state), Extension(user), Path(id)).await
}

/// `POST /api/categories/{id}/members`. Manageable. Idempotent: re-adding
/// an existing member is a no-op (returns the unchanged member list).
async fn add_member(
    State(state): State<CategoriesState>,
    Extension(user): Extension<UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> AppResult<Json<MembersListResponse>> {
    ensure_can_manage(&state, &user, id).await?;

    // Insert-or-ignore via existence check; the composite PK would error
    // on duplicate but we want idempotent semantics for the API.
    let existing = CategoryMember::find()
        .filter(category_member::Column::CategoryId.eq(id))
        .filter(category_member::Column::UserId.eq(req.user_id))
        .one(&state.db)
        .await?;
    if existing.is_none() {
        // Confirm the user actually exists before creating the row.
        let user_exists = User::find_by_id(req.user_id)
            .one(&state.db)
            .await?
            .is_some();
        if !user_exists {
            return Err(AppError::ValidationError("User not found".to_string()));
        }
        category_member::ActiveModel {
            category_id: Set(id),
            user_id: Set(req.user_id),
            ..Default::default()
        }
        .insert(&state.db)
        .await?;
    }

    list_members(State(state), Extension(user), Path(id)).await
}

/// `DELETE /api/categories/{id}/members/{user_id}`. Manageable. Rejects
/// removal of the category's creator — they remain a member as long as
/// the category exists.
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
    CategoryMember::delete_many()
        .filter(category_member::Column::CategoryId.eq(id))
        .filter(category_member::Column::UserId.eq(member_id))
        .exec(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
