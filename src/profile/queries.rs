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
use crate::entities::{user, User};
use crate::profile::{sanitize_seed, HANDLE_MAX_LEN};
use anyhow::Result;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use uuid::Uuid;

/// Mint a unique handle for a new user. Strips disallowed chars from
/// `seed` (typically the email local-part), pads short seeds, and appends
/// a numeric suffix until uniqueness is found.
///
/// Used at every user-creation site so the NOT NULL handle column is always
/// populated. Suffix collision uses an incrementing `-1`, `-2`, ...; the
/// loop bounds the search so a pathological collision pattern still
/// terminates.
pub async fn mint_unique_handle<C: ConnectionTrait>(db: &C, seed: &str) -> Result<String> {
    let base = sanitize_seed(seed);
    if !is_handle_taken(db, &base).await? {
        return Ok(base);
    }
    for n in 1..10_000u32 {
        let suffix = format!("-{}", n);
        let trimmed = base
            .chars()
            .take(HANDLE_MAX_LEN.saturating_sub(suffix.chars().count()))
            .collect::<String>();
        let candidate = format!("{}{}", trimmed, suffix);
        if !is_handle_taken(db, &candidate).await? {
            return Ok(candidate);
        }
    }
    anyhow::bail!("Could not mint a unique handle for seed '{}'", seed)
}

pub async fn is_handle_taken<C: ConnectionTrait>(db: &C, candidate: &str) -> Result<bool> {
    let found = User::find()
        .filter(user::Column::Handle.eq(candidate))
        .one(db)
        .await?;
    Ok(found.is_some())
}

pub async fn find_user<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Option<user::Model>, DbErr> {
    User::find_by_id(id).one(db).await
}

pub async fn find_user_by_handle<C: ConnectionTrait>(
    db: &C,
    handle: &str,
) -> Result<Option<user::Model>, DbErr> {
    User::find()
        .filter(user::Column::Handle.eq(handle))
        .one(db)
        .await
}

/// Returns true when another row already uses `handle` (i.e. uniqueness
/// would fail for `id`).
pub async fn handle_taken_by_other<C: ConnectionTrait>(
    db: &C,
    handle: &str,
    id: Uuid,
) -> Result<bool, DbErr> {
    let conflict = User::find()
        .filter(user::Column::Handle.eq(handle))
        .filter(user::Column::Id.ne(id))
        .one(db)
        .await?;
    Ok(conflict.is_some())
}

pub async fn list_active_profiles_excluding<C: ConnectionTrait>(
    db: &C,
    exclude_viewer: Option<Uuid>,
    limit: u64,
) -> Result<Vec<user::Model>, DbErr> {
    let mut q = User::find()
        .filter(user::Column::Active.eq(true))
        .filter(user::Column::ActivatedAt.is_not_null())
        .order_by_asc(user::Column::Handle)
        .limit(limit);
    if let Some(vid) = exclude_viewer {
        q = q.filter(user::Column::Id.ne(vid));
    }
    q.all(db).await
}

pub async fn update_user<C: ConnectionTrait>(
    db: &C,
    active: user::ActiveModel,
) -> Result<user::Model, DbErr> {
    active.update(db).await
}
