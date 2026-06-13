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
use crate::entities::{invite_code, InviteCode};
use crate::errors::{AppError, AppResult};
use crate::invites::{hash_invite_code, DEFAULT_INVITE_LIFETIME_HOURS};
use chrono::{Duration, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, Order,
    QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

/// Returns the live invite matching the supplied plaintext `code`, or `None`
/// if it doesn't exist, has been revoked, has been used, or has expired.
/// Callers don't get to distinguish these cases at the API surface; knowing
/// *why* a code is invalid leaks information about prior issuance.
///
/// The DB stores `hash_invite_code(plaintext)` in the `code` column rather
/// than the plaintext itself, so this function hashes the input and looks
/// the row up by indexed equality (O(1) via the unique index on `code`).
pub async fn validate_invite_code(
    db: &DatabaseConnection,
    code: &str,
) -> AppResult<Option<invite_code::Model>> {
    let hashed = hash_invite_code(code);
    let row = InviteCode::find()
        .filter(invite_code::Column::Code.eq(hashed))
        .one(db)
        .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    if row.revoked_at.is_some() {
        return Ok(None);
    }
    if row.used_at.is_some() {
        return Ok(None);
    }

    let now = Utc::now();
    if row.expires_at.with_timezone(&Utc) <= now {
        return Ok(None);
    }

    Ok(Some(row))
}

/// Marks an invite as accepted by the given user. The UPDATE only lands when
/// `used_at` is still NULL, so two racing acceptances cannot both consume the
/// same code: the loser's update affects 0 rows and errors. Callers run this
/// inside their activation transaction so that error rolls the activation
/// back.
///
/// A 0-row update is disambiguated so the error is accurate: a present row
/// means another acceptance already consumed it (`ValidationError`); an
/// absent row means the id doesn't exist (`NotFound`, as the pre-atomic
/// version returned).
pub async fn mark_used<C>(db: &C, invite_id: Uuid, user_id: Uuid) -> AppResult<()>
where
    C: ConnectionTrait,
{
    let result = InviteCode::update_many()
        .col_expr(invite_code::Column::UsedAt, Expr::value(Utc::now()))
        .col_expr(invite_code::Column::UsedByUserId, Expr::value(user_id))
        .filter(invite_code::Column::Id.eq(invite_id))
        .filter(invite_code::Column::UsedAt.is_null())
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        let exists = InviteCode::find_by_id(invite_id).one(db).await?.is_some();
        return Err(if exists {
            AppError::ValidationError("Invite has already been used".to_string())
        } else {
            AppError::NotFound("Invite not found".to_string())
        });
    }
    Ok(())
}

/// Create an invite_code with the default lifetime, used both by the admin
/// `POST /api/admin/invite-codes` route and by the auto-issue path inside
/// `POST /api/admin/users`. Accepts any `ConnectionTrait` so it can run
/// inside a transaction.
///
/// Returns the inserted row AND the plaintext code. The plaintext exists
/// only in this call's return value: the DB stores only the hash, so the
/// admin must capture and deliver the plaintext at issuance time.
pub async fn issue_invite_for_user<C>(
    db: &C,
    created_by: Uuid,
    email_hint: Option<String>,
) -> AppResult<(invite_code::Model, String)>
where
    C: ConnectionTrait,
{
    let plaintext = crate::invites::generate_invite_code();
    let expires_at = Utc::now() + Duration::hours(DEFAULT_INVITE_LIFETIME_HOURS);
    let active = invite_code::ActiveModel {
        id: Set(Uuid::new_v4()),
        code: Set(hash_invite_code(&plaintext)),
        email_hint: Set(email_hint),
        created_by: Set(created_by),
        expires_at: Set(expires_at.into()),
        used_at: Set(None),
        used_by_user_id: Set(None),
        revoked_at: Set(None),
        ..Default::default()
    };
    let inserted = active.insert(db).await?;
    Ok((inserted, plaintext))
}

pub async fn insert_invite(
    db: &DatabaseConnection,
    active: invite_code::ActiveModel,
) -> AppResult<invite_code::Model> {
    Ok(active.insert(db).await?)
}

pub async fn list_invites_desc(db: &DatabaseConnection) -> AppResult<Vec<invite_code::Model>> {
    Ok(InviteCode::find()
        .order_by(invite_code::Column::CreatedAt, Order::Desc)
        .all(db)
        .await?)
}

pub async fn find_invite(
    db: &DatabaseConnection,
    id: Uuid,
) -> AppResult<Option<invite_code::Model>> {
    Ok(InviteCode::find_by_id(id).one(db).await?)
}

pub async fn update_invite(
    db: &DatabaseConnection,
    active: invite_code::ActiveModel,
) -> AppResult<invite_code::Model> {
    Ok(active.update(db).await?)
}
