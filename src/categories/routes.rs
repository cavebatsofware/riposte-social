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

use crate::categories::{slugify, validate_slug_shape};
use crate::entities::{category, Category};
use crate::errors::{AppError, AppResult};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct CategoriesState {
    pub db: DatabaseConnection,
}

pub fn public_category_routes() -> Router<CategoriesState> {
    Router::new().route("/api/categories", get(list_categories))
}

pub fn admin_category_routes() -> Router<CategoriesState> {
    Router::new()
        .route("/api/admin/categories", post(create_category))
        .route(
            "/api/admin/categories/{id}",
            axum::routing::patch(update_category).delete(delete_category),
        )
}

#[derive(Serialize)]
pub struct CategoryResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub ordinal: i32,
    pub color: Option<String>,
}

impl From<category::Model> for CategoryResponse {
    fn from(m: category::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            slug: m.slug,
            ordinal: m.ordinal,
            color: m.color,
        }
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
}

#[derive(Serialize)]
pub struct CategoriesListResponse {
    pub categories: Vec<CategoryResponse>,
}

async fn list_categories(
    State(state): State<CategoriesState>,
) -> AppResult<Json<CategoriesListResponse>> {
    let rows = Category::find()
        .order_by_asc(category::Column::Ordinal)
        .order_by_asc(category::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(CategoriesListResponse {
        categories: rows.into_iter().map(Into::into).collect(),
    }))
}

async fn create_category(
    State(state): State<CategoriesState>,
    Json(req): Json<CreateCategoryRequest>,
) -> AppResult<(StatusCode, Json<CategoryResponse>)> {
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

    let active = category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(name),
        slug: Set(slug),
        ordinal: Set(req.ordinal.unwrap_or(0)),
        color: Set(req.color.filter(|s| !s.is_empty())),
        ..Default::default()
    };
    let row = active.insert(&state.db).await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

async fn update_category(
    State(state): State<CategoriesState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCategoryRequest>,
) -> AppResult<Json<CategoryResponse>> {
    let row = Category::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::AuthError("Category not found".to_string()))?;

    if let Some(ref s) = req.slug {
        validate_slug_shape(s.trim()).map_err(AppError::ValidationError)?;
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

    let mut active: category::ActiveModel = row.into();
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
    let updated = active.update(&state.db).await?;
    Ok(Json(updated.into()))
}

async fn delete_category(
    State(state): State<CategoriesState>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    // FKs on posts/albums.category_id are ON DELETE SET NULL, so deleting
    // a category just leaves anything that referenced it as uncategorized.
    let res = Category::delete_by_id(id).exec(&state.db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::AuthError("Category not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}
