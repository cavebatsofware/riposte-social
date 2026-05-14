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
use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};

/// Visibility tiers for a post. Stored as the column's string value, not a
/// Postgres enum, so adding tiers later is a no-op DDL.
///
/// `private` is author-only: the post is invisible to every other user
/// including administrators in feed/listing contexts. Admin moderation
/// against a known post id still works through explicit moderation routes;
/// the rule here is "feed surfaces never expose private posts to anyone but
/// the author".
pub const VISIBILITY_PUBLIC: &str = "public";
pub const VISIBILITY_COMMENTERS: &str = "commenters";
pub const VISIBILITY_POSTERS: &str = "posters";
pub const VISIBILITY_PRIVATE: &str = "private";

/// Returns true when `visibility` is one of the four valid tiers.
pub fn is_valid_visibility(v: &str) -> bool {
    matches!(
        v,
        VISIBILITY_PUBLIC | VISIBILITY_COMMENTERS | VISIBILITY_POSTERS | VISIBILITY_PRIVATE
    )
}

/// Discriminator across post-like rows. Open-ended TEXT so future kinds
/// (e.g. `'article'`) drop in without DDL. Stored on `posts.kind`.
pub const KIND_POST: &str = "post";
pub const KIND_ALBUM: &str = "album";

pub const ALLOWED_KINDS: &[&str] = &[KIND_POST, KIND_ALBUM];

pub fn is_valid_kind(v: &str) -> bool {
    ALLOWED_KINDS.contains(&v)
}

/// App-layer cap on `slug` length (the album name field for kind=album,
/// optional title-like field for kind=post).
pub const SLUG_MAX_LEN: usize = 200;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub author_id: Uuid,
    pub body: String,
    pub visibility: String,
    pub published_at: DateTimeWithTimeZone,
    pub import_source: Option<String>,
    pub import_external_id: Option<String>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// nullable category FK. NULL means uncategorized. Deleting
    /// a category sets this to NULL via `ON DELETE SET NULL`.
    pub category_id: Option<Uuid>,
    /// `'post'` (default) or `'album'`. Future
    /// kinds (`'article'`, ...) extend this without DDL.
    pub kind: String,
    /// For `kind='album'` this carries the album
    /// name (required at the handler layer); for `kind='post'` it's
    /// optional.
    pub slug: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::AuthorId",
        to = "super::user::Column::Id",
        on_delete = "Restrict"
    )]
    Author,
    #[sea_orm(has_many = "super::post_media::Entity")]
    Media,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Author.def()
    }
}

impl Related<super::post_media::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Media.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now: DateTimeWithTimeZone = chrono::Utc::now().into();
        if insert {
            self.created_at = Set(now);
        }
        self.updated_at = Set(now);
        Ok(self)
    }
}
