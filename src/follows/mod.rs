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

pub mod handlers;
pub mod queries;
pub mod types;

use sea_orm::DatabaseConnection;

pub use handlers::{follows_read_routes, follows_write_routes};
pub use queries::{count_followers, count_following, fetch_follow_states, FollowState};

#[derive(Clone)]
pub struct FollowsState {
    pub db: DatabaseConnection,
}
