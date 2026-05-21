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
use crate::entities::{follow, user, Follow, User};
use crate::follows::types::UserSummary;
use crate::profile::{avatar_icon_data_for, avatar_url_for};
use chrono::{DateTime, FixedOffset};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, JoinType, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait, Set,
};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FollowState {
    pub you_follow: bool,
    pub follows_you: bool,
}

pub async fn fetch_follow_states<C>(
    db: &C,
    viewer_id: Uuid,
    targets: &[Uuid],
) -> Result<HashMap<Uuid, FollowState>, DbErr>
where
    C: ConnectionTrait,
{
    let mut out: HashMap<Uuid, FollowState> = targets
        .iter()
        .map(|id| (*id, FollowState::default()))
        .collect();
    if targets.is_empty() {
        return Ok(out);
    }

    let you_follow: Vec<(Uuid, Uuid)> = Follow::find()
        .filter(follow::Column::FollowerId.eq(viewer_id))
        .filter(follow::Column::FollowedId.is_in(targets.iter().copied()))
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.follower_id, m.followed_id))
        .collect();
    for (_follower, followed) in you_follow {
        if let Some(s) = out.get_mut(&followed) {
            s.you_follow = true;
        }
    }

    let follows_you: Vec<(Uuid, Uuid)> = Follow::find()
        .filter(follow::Column::FollowedId.eq(viewer_id))
        .filter(follow::Column::FollowerId.is_in(targets.iter().copied()))
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.follower_id, m.followed_id))
        .collect();
    for (follower, _followed) in follows_you {
        if let Some(s) = out.get_mut(&follower) {
            s.follows_you = true;
        }
    }

    Ok(out)
}

pub async fn count_followers<C: ConnectionTrait>(db: &C, user_id: Uuid) -> Result<u64, DbErr> {
    Follow::find()
        .filter(follow::Column::FollowedId.eq(user_id))
        .join(JoinType::InnerJoin, follow::Relation::Follower.def())
        .filter(user::Column::Active.eq(true))
        .count(db)
        .await
}

pub async fn count_following<C: ConnectionTrait>(db: &C, user_id: Uuid) -> Result<u64, DbErr> {
    Follow::find()
        .filter(follow::Column::FollowerId.eq(user_id))
        .join(JoinType::InnerJoin, follow::Relation::Followed.def())
        .filter(user::Column::Active.eq(true))
        .count(db)
        .await
}

pub async fn find_active_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Option<user::Model>, DbErr> {
    User::find_by_id(user_id).one(db).await
}

pub async fn upsert_follow<C: ConnectionTrait>(
    db: &C,
    follower_id: Uuid,
    followed_id: Uuid,
    now: DateTime<FixedOffset>,
) -> Result<bool, DbErr> {
    let am = follow::ActiveModel {
        follower_id: Set(follower_id),
        followed_id: Set(followed_id),
        created_at: Set(now),
    };
    let conflict = OnConflict::columns([follow::Column::FollowerId, follow::Column::FollowedId])
        .do_nothing()
        .to_owned();
    match Follow::insert(am).on_conflict(conflict).exec(db).await {
        Ok(_) => Ok(true),
        Err(DbErr::RecordNotInserted) => Ok(false),
        Err(e) => Err(e),
    }
}

pub async fn delete_follow<C: ConnectionTrait>(
    db: &C,
    follower_id: Uuid,
    followed_id: Uuid,
) -> Result<u64, DbErr> {
    let result = Follow::delete_many()
        .filter(follow::Column::FollowerId.eq(follower_id))
        .filter(follow::Column::FollowedId.eq(followed_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

pub async fn list_followers_page<C: ConnectionTrait>(
    db: &C,
    target_id: Uuid,
    cursor: Option<(DateTime<FixedOffset>, Uuid)>,
    fetch: u64,
) -> Result<Vec<follow::Model>, DbErr> {
    let mut q = Follow::find()
        .filter(follow::Column::FollowedId.eq(target_id))
        .join(JoinType::InnerJoin, follow::Relation::Follower.def())
        .filter(user::Column::Active.eq(true))
        .order_by_desc(follow::Column::CreatedAt)
        .order_by_desc(follow::Column::FollowerId);
    if let Some((c_at, c_id)) = cursor {
        q = q.filter(
            sea_orm::Condition::any()
                .add(follow::Column::CreatedAt.lt(c_at))
                .add(
                    sea_orm::Condition::all()
                        .add(follow::Column::CreatedAt.eq(c_at))
                        .add(follow::Column::FollowerId.lt(c_id)),
                ),
        );
    }
    q.limit(fetch).all(db).await
}

pub async fn list_following_page<C: ConnectionTrait>(
    db: &C,
    target_id: Uuid,
    cursor: Option<(DateTime<FixedOffset>, Uuid)>,
    fetch: u64,
) -> Result<Vec<follow::Model>, DbErr> {
    let mut q = Follow::find()
        .filter(follow::Column::FollowerId.eq(target_id))
        .join(JoinType::InnerJoin, follow::Relation::Followed.def())
        .filter(user::Column::Active.eq(true))
        .order_by_desc(follow::Column::CreatedAt)
        .order_by_desc(follow::Column::FollowedId);
    if let Some((c_at, c_id)) = cursor {
        q = q.filter(
            sea_orm::Condition::any()
                .add(follow::Column::CreatedAt.lt(c_at))
                .add(
                    sea_orm::Condition::all()
                        .add(follow::Column::CreatedAt.eq(c_at))
                        .add(follow::Column::FollowedId.lt(c_id)),
                ),
        );
    }
    q.limit(fetch).all(db).await
}

pub async fn fetch_user_summaries<C: ConnectionTrait>(
    db: &C,
    ids: &[Uuid],
) -> Result<Vec<UserSummary>, DbErr> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = User::find()
        .filter(user::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await?;
    let by_id: HashMap<Uuid, user::Model> = rows.into_iter().map(|m| (m.id, m)).collect();
    Ok(ids
        .iter()
        .filter_map(|id| {
            by_id.get(id).map(|m| UserSummary {
                user_id: m.id,
                handle: m.handle.clone(),
                display_name: m.display_name.clone(),
                avatar_url: avatar_url_for(m),
                avatar_icon_data: avatar_icon_data_for(m),
                role: m.role.clone(),
            })
        })
        .collect())
}
