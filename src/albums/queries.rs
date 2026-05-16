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
use crate::entities::{category, post, post_media, user, Category, Post, PostMedia, User};
use chrono::{DateTime, FixedOffset};
use sea_orm::sea_query::{Expr, IntoCondition};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, QuerySelect,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Per-album aggregate row used by `list_albums`. Computed in a single
/// GROUP BY against `post_media` so the list endpoint doesn't load every
/// media row just to surface count + cover.
#[derive(FromQueryResult)]
pub struct AlbumStatsRow {
    pub post_id: Uuid,
    pub photo_count: i64,
    pub cover_id: Option<Uuid>,
}

pub async fn find_user<C: ConnectionTrait>(
    conn: &C,
    id: Uuid,
) -> Result<Option<user::Model>, DbErr> {
    User::find_by_id(id).one(conn).await
}

pub async fn find_post_active<C: ConnectionTrait>(
    conn: &C,
    id: Uuid,
) -> Result<Option<post::Model>, DbErr> {
    Post::find_by_id(id)
        .filter(post::Column::DeletedAt.is_null())
        .one(conn)
        .await
}

pub async fn find_post_media<C: ConnectionTrait>(
    conn: &C,
    media_id: Uuid,
) -> Result<Option<post_media::Model>, DbErr> {
    PostMedia::find_by_id(media_id).one(conn).await
}

pub async fn find_post_media_in_post<C: ConnectionTrait>(
    conn: &C,
    media_id: Uuid,
    post_id: Uuid,
) -> Result<Option<post_media::Model>, DbErr> {
    PostMedia::find_by_id(media_id)
        .filter(post_media::Column::PostId.eq(post_id))
        .one(conn)
        .await
}

pub async fn list_post_media_ordered<C: ConnectionTrait>(
    conn: &C,
    post_id: Uuid,
) -> Result<Vec<post_media::Model>, DbErr> {
    PostMedia::find()
        .filter(post_media::Column::PostId.eq(post_id))
        .order_by_asc(post_media::Column::Ordinal)
        .all(conn)
        .await
}

pub async fn find_category<C: ConnectionTrait>(
    conn: &C,
    id: Uuid,
) -> Result<Option<category::Model>, DbErr> {
    Category::find_by_id(id).one(conn).await
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

pub async fn save_album<C: ConnectionTrait>(
    conn: &C,
    active: post::ActiveModel,
) -> Result<post::Model, DbErr> {
    active.update(conn).await
}

pub async fn save_album_media<C: ConnectionTrait>(
    conn: &C,
    active: post_media::ActiveModel,
) -> Result<post_media::Model, DbErr> {
    active.update(conn).await
}

pub async fn load_authors_by_ids<C: ConnectionTrait>(
    conn: &C,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, user::Model>, DbErr> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = User::find()
        .filter(user::Column::Id.is_in(ids))
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(|a| (a.id, a)).collect())
}

pub struct AlbumPageFilters<F>
where
    F: IntoCondition,
{
    pub feed_condition: F,
    pub author: Option<Uuid>,
    pub category_filter: AlbumCategoryFilter,
    pub cursor: Option<(DateTime<FixedOffset>, Uuid)>,
    pub fetch: u64,
}

pub enum AlbumCategoryFilter {
    Any,
    Uncategorized,
    InCategory(Uuid),
}

pub async fn list_album_posts<C, F>(
    conn: &C,
    filters: AlbumPageFilters<F>,
) -> Result<Vec<post::Model>, DbErr>
where
    C: ConnectionTrait,
    F: IntoCondition,
{
    let mut q = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(post::KIND_ALBUM))
        .filter(filters.feed_condition)
        .order_by_desc(post::Column::PublishedAt)
        .order_by_desc(post::Column::Id);

    if let Some(author_id) = filters.author {
        q = q.filter(post::Column::AuthorId.eq(author_id));
    }
    match filters.category_filter {
        AlbumCategoryFilter::Any => {}
        AlbumCategoryFilter::Uncategorized => {
            q = q.filter(post::Column::CategoryId.is_null());
        }
        AlbumCategoryFilter::InCategory(id) => {
            q = q.filter(post::Column::CategoryId.eq(id));
        }
    }
    if let Some((cursor_at, cursor_id)) = filters.cursor {
        q = q.filter(
            sea_orm::Condition::any()
                .add(post::Column::PublishedAt.lt(cursor_at))
                .add(
                    sea_orm::Condition::all()
                        .add(post::Column::PublishedAt.eq(cursor_at))
                        .add(post::Column::Id.lt(cursor_id)),
                ),
        );
    }

    q.limit(filters.fetch).all(conn).await
}

pub async fn aggregate_album_stats<C: ConnectionTrait>(
    conn: &C,
    album_ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, AlbumStatsRow>, DbErr> {
    if album_ids.is_empty() {
        return Ok(HashMap::new());
    }
    // Aggregate per album in one round trip: photo_count via COUNT(*)
    // and the implicit cover (lowest-ordinal media id) via Postgres'
    // ARRAY_AGG ordered subscript.
    let rows: Vec<AlbumStatsRow> = PostMedia::find()
        .select_only()
        .column(post_media::Column::PostId)
        .column_as(post_media::Column::Id.count(), "photo_count")
        .column_as(
            Expr::cust("(ARRAY_AGG(id ORDER BY ordinal ASC))[1]"),
            "cover_id",
        )
        .filter(post_media::Column::PostId.is_in(album_ids))
        .group_by(post_media::Column::PostId)
        .into_model::<AlbumStatsRow>()
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(|s| (s.post_id, s)).collect())
}
