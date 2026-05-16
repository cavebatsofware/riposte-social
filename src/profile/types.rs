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
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Response shape for the caller's own profile. Includes `email` since
/// the caller is reading their own row.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct MeProfileResponse {
    pub user_id: Uuid,
    pub handle: String,
    pub email: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    #[ts(type = "number")]
    pub follower_count: i64,
    #[ts(type = "number")]
    pub following_count: i64,
}

/// Response shape for public profile reads. Email is intentionally omitted:
/// public profiles are visible to anyone who can reach the page; the
/// recipient's email is not.
///
/// `follows_you` and `you_follow` are populated for authenticated viewers
/// only; both are `false` for anonymous reads since the relationship has
/// no defined viewer.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct PublicProfileResponse {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    #[ts(type = "number")]
    pub follower_count: i64,
    #[ts(type = "number")]
    pub following_count: i64,
    pub follows_you: bool,
    pub you_follow: bool,
}

/// Compact profile shape used by the list endpoint. Drops `bio` and
/// `pronouns` since they aren't shown in the rail or directory grid.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct ProfileSummary {
    pub user_id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct ListProfilesResponse {
    pub profiles: Vec<ProfileSummary>,
}

#[derive(Deserialize, Default, TS)]
#[serde(default)]
#[ts(export)]
pub struct ListProfilesQuery {
    #[ts(type = "number | null")]
    pub limit: Option<u64>,
}

#[derive(Deserialize, Default, TS)]
#[serde(default)]
#[ts(export)]
pub struct PatchMeProfileRequest {
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct AvatarUploadResponse {
    pub avatar_url: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct PatchMeLocaleRequest {
    /// One of `crate::profile::locale::SUPPORTED_LOCALES`. Anything else
    /// returns 400.
    pub locale: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct LocaleResponse {
    pub locale: String,
}
