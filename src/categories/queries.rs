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
use crate::entities::{category, category_member, user, Category, CategoryMember, User};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
};
use std::collections::HashMap;
use uuid::Uuid;

pub async fn list_categories_ordered<C: ConnectionTrait>(
    conn: &C,
) -> Result<Vec<category::Model>, DbErr> {
    Category::find()
        .order_by_asc(category::Column::Ordinal)
        .order_by_asc(category::Column::Name)
        .all(conn)
        .await
}

pub async fn find_category<C: ConnectionTrait>(
    conn: &C,
    id: Uuid,
) -> Result<Option<category::Model>, DbErr> {
    Category::find_by_id(id).one(conn).await
}

pub async fn find_category_by_name<C: ConnectionTrait>(
    conn: &C,
    name: &str,
) -> Result<Option<category::Model>, DbErr> {
    Category::find()
        .filter(category::Column::Name.eq(name))
        .one(conn)
        .await
}

pub async fn find_category_by_slug<C: ConnectionTrait>(
    conn: &C,
    slug: &str,
) -> Result<Option<category::Model>, DbErr> {
    Category::find()
        .filter(category::Column::Slug.eq(slug))
        .one(conn)
        .await
}

pub async fn find_category_by_name_excluding<C: ConnectionTrait>(
    conn: &C,
    name: &str,
    exclude_id: Uuid,
) -> Result<Option<category::Model>, DbErr> {
    Category::find()
        .filter(category::Column::Name.eq(name))
        .filter(category::Column::Id.ne(exclude_id))
        .one(conn)
        .await
}

pub async fn find_category_by_slug_excluding<C: ConnectionTrait>(
    conn: &C,
    slug: &str,
    exclude_id: Uuid,
) -> Result<Option<category::Model>, DbErr> {
    Category::find()
        .filter(category::Column::Slug.eq(slug))
        .filter(category::Column::Id.ne(exclude_id))
        .one(conn)
        .await
}

pub async fn insert_category<C: ConnectionTrait>(
    conn: &C,
    active: category::ActiveModel,
) -> Result<category::Model, DbErr> {
    active.insert(conn).await
}

pub async fn update_category<C: ConnectionTrait>(
    conn: &C,
    active: category::ActiveModel,
) -> Result<category::Model, DbErr> {
    active.update(conn).await
}

pub async fn list_members<C: ConnectionTrait>(
    conn: &C,
    category_id: Uuid,
) -> Result<Vec<category_member::Model>, DbErr> {
    CategoryMember::find()
        .filter(category_member::Column::CategoryId.eq(category_id))
        .all(conn)
        .await
}

pub async fn load_users_by_ids<C: ConnectionTrait>(
    conn: &C,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, user::Model>, DbErr> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let users = User::find()
        .filter(user::Column::Id.is_in(ids))
        .all(conn)
        .await?;
    Ok(users.into_iter().map(|u| (u.id, u)).collect())
}

pub async fn member_exists<C: ConnectionTrait>(
    conn: &C,
    category_id: Uuid,
    user_id: Uuid,
) -> Result<bool, DbErr> {
    Ok(CategoryMember::find()
        .filter(category_member::Column::CategoryId.eq(category_id))
        .filter(category_member::Column::UserId.eq(user_id))
        .one(conn)
        .await?
        .is_some())
}

pub async fn user_exists<C: ConnectionTrait>(conn: &C, user_id: Uuid) -> Result<bool, DbErr> {
    Ok(User::find_by_id(user_id).one(conn).await?.is_some())
}

pub async fn insert_member<C: ConnectionTrait>(
    conn: &C,
    category_id: Uuid,
    user_id: Uuid,
) -> Result<(), DbErr> {
    category_member::ActiveModel {
        category_id: sea_orm::Set(category_id),
        user_id: sea_orm::Set(user_id),
        ..Default::default()
    }
    .insert(conn)
    .await?;
    Ok(())
}

pub async fn delete_category<C: ConnectionTrait>(conn: &C, id: Uuid) -> Result<(), DbErr> {
    Category::delete_by_id(id).exec(conn).await?;
    Ok(())
}

pub async fn delete_member<C: ConnectionTrait>(
    conn: &C,
    category_id: Uuid,
    user_id: Uuid,
) -> Result<(), DbErr> {
    CategoryMember::delete_many()
        .filter(category_member::Column::CategoryId.eq(category_id))
        .filter(category_member::Column::UserId.eq(user_id))
        .exec(conn)
        .await?;
    Ok(())
}

pub async fn delete_all_members<C: ConnectionTrait>(
    conn: &C,
    category_id: Uuid,
) -> Result<(), DbErr> {
    CategoryMember::delete_many()
        .filter(category_member::Column::CategoryId.eq(category_id))
        .exec(conn)
        .await?;
    Ok(())
}
