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
use crate::entities::category;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, TS)]
#[ts(export)]
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

pub fn into_response(m: category::Model, manageable: bool) -> CategoryResponse {
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

#[derive(Deserialize, TS)]
#[ts(export)]
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

#[derive(Deserialize, Default, TS)]
#[ts(export)]
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

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CategoriesListResponse {
    pub categories: Vec<CategoryResponse>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MemberResponse {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MembersListResponse {
    pub members: Vec<MemberResponse>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ReplaceMembersRequest {
    pub user_ids: Vec<Uuid>,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
}
