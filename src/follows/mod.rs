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
//! Follower / followed social graph.
//!
//! One directed edge per (follower, followed) pair. "Mutual follow"
//! emerges when both edges exist; the UI surfaces it from the
//! `follows_you` / `you_follow` booleans.
//!
//! Visibility today: any authenticated caller can follow any other active
//! user. Listings filter to active (not soft-deleted) users on both sides.
//! When per-user visibility lands later, the gate slots into
//! [`routes::ensure_target_visible`] without changing the wire format.

pub mod routes;

use crate::entities::{follow, user, Follow};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, JoinType, PaginatorTrait, QueryFilter, QuerySelect,
    RelationTrait,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Bulk follower-state lookup. Returns one entry per user_id in `targets`,
/// including ones that don't exist or aren't active. The unknown / inactive
/// case still receives the false/false default so the UI can render a
/// no-op CTA without a second round trip.
///
/// Runs two queries against the `follows` table (one for "viewer follows
/// target", one for "target follows viewer"), both bounded by the
/// composite PK or `idx_follows_followed_id`.
pub(crate) async fn fetch_follow_states<C>(
    db: &C,
    viewer_id: Uuid,
    targets: &[Uuid],
) -> Result<HashMap<Uuid, FollowState>, sea_orm::DbErr>
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FollowState {
    /// Viewer follows this target.
    pub you_follow: bool,
    /// This target follows the viewer.
    pub follows_you: bool,
}

/// Count followers of a single user. Joins through the `Follower` relation
/// and filters to active follower rows so a soft-deleted account doesn't
/// pad the count. The Follow entity has two FKs to users (follower and
/// followed), so the join is via the explicit relation rather than
/// `Related`-derived `inner_join`.
pub(crate) async fn count_followers<C>(db: &C, user_id: Uuid) -> Result<u64, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Follow::find()
        .filter(follow::Column::FollowedId.eq(user_id))
        .join(JoinType::InnerJoin, follow::Relation::Follower.def())
        .filter(user::Column::Active.eq(true))
        .count(db)
        .await
}

pub(crate) async fn count_following<C>(db: &C, user_id: Uuid) -> Result<u64, sea_orm::DbErr>
where
    C: ConnectionTrait,
{
    Follow::find()
        .filter(follow::Column::FollowerId.eq(user_id))
        .join(JoinType::InnerJoin, follow::Relation::Followed.def())
        .filter(user::Column::Active.eq(true))
        .count(db)
        .await
}
