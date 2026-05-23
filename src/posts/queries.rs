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
//! DB query builders for posts. Includes the BM25-aware feed query and
//! the smaller `find_*`/`load_*` helpers handlers compose around.

use crate::articles::queries::draft_post_ids_subquery;
use crate::entities::{category, post, post_media, user, Category, Post, PostMedia, User};
use chrono::{DateTime, FixedOffset};
use sea_orm::sea_query::{Expr, IntoCondition};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr,
    EntityTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect, QueryTrait, Statement,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Slim projection of `post_media` returning the row id plus its
/// pre-generated thumbnail bytes and `created_at`. Used by the variants
/// batch endpoint so the query plan can ignore the rest of the row and
/// avoid pulling every other column into memory for items the caller
/// isn't asking about. `created_at` participates in the Last-Modified /
/// If-Modified-Since path: thumbnail bytes are immutable per row, so
/// `MAX(created_at)` across the batch is a stable response mtime.
#[derive(FromQueryResult)]
pub struct PostMediaThumbnailRow {
    pub id: Uuid,
    pub thumbnail_data: Option<Vec<u8>>,
    pub created_at: sea_orm::prelude::DateTimeWithTimeZone,
}

/// Project (id, thumbnail_data, created_at) for media rows whose `id` is
/// in `ids` AND whose `post_id` equals the supplied parent. Cross-parent
/// ids are dropped silently by the WHERE clause; callers compare result
/// count to the requested set to detect mismatches and reject the batch.
pub async fn list_post_media_thumbnails<C: ConnectionTrait>(
    conn: &C,
    post_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<PostMediaThumbnailRow>, DbErr> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    PostMedia::find()
        .select_only()
        .column(post_media::Column::Id)
        .column(post_media::Column::ThumbnailData)
        .column(post_media::Column::CreatedAt)
        .filter(post_media::Column::PostId.eq(post_id))
        .filter(post_media::Column::Id.is_in(ids.iter().copied()))
        .into_model::<PostMediaThumbnailRow>()
        .all(conn)
        .await
}

pub async fn find_user<C: ConnectionTrait>(
    conn: &C,
    id: Uuid,
) -> Result<Option<user::Model>, DbErr> {
    User::find_by_id(id).one(conn).await
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

pub async fn find_post_media<C: ConnectionTrait>(
    conn: &C,
    media_id: Uuid,
) -> Result<Option<post_media::Model>, DbErr> {
    PostMedia::find_by_id(media_id).one(conn).await
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

pub async fn save_post<C: ConnectionTrait>(
    conn: &C,
    active: post::ActiveModel,
) -> Result<post::Model, DbErr> {
    active.update(conn).await
}

pub async fn load_users_by_ids<C: ConnectionTrait>(
    conn: &C,
    ids: Vec<Uuid>,
) -> Result<Vec<user::Model>, DbErr> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    User::find()
        .filter(user::Column::Id.is_in(ids))
        .all(conn)
        .await
}

pub async fn load_categories_by_ids<C: ConnectionTrait>(
    conn: &C,
    ids: Vec<Uuid>,
) -> Result<Vec<category::Model>, DbErr> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Category::find()
        .filter(category::Column::Id.is_in(ids))
        .all(conn)
        .await
}

pub async fn load_media_for_posts<C: ConnectionTrait>(
    conn: &C,
    post_ids: Vec<Uuid>,
) -> Result<Vec<post_media::Model>, DbErr> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }
    PostMedia::find()
        .filter(post_media::Column::PostId.is_in(post_ids))
        .order_by_asc(post_media::Column::Ordinal)
        .all(conn)
        .await
}

pub async fn load_top_comment_authors<C: ConnectionTrait>(
    conn: &C,
    user_ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, user::Model>, DbErr> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = User::find()
        .filter(user::Column::Id.is_in(user_ids))
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(|u| (u.id, u)).collect())
}

pub enum PostCategoryFilter {
    Any,
    Uncategorized,
    InCategory(Uuid),
}

/// Which `kind` values participate in a feed page. The default is `All`,
/// which interleaves `post` and `article` rows (albums are excluded
/// regardless because they have their own listing surface).
#[derive(Clone, Copy)]
pub enum FeedKindFilter {
    /// Mixed feed: posts + articles, interleaved by published_at.
    All,
    /// Posts only (kind='post').
    PostsOnly,
    /// Articles only (kind='article').
    ArticlesOnly,
}

impl FeedKindFilter {
    pub fn from_query(value: Option<&str>) -> Result<Self, &'static str> {
        match value.map(str::trim).filter(|s| !s.is_empty()) {
            None | Some("all") => Ok(Self::All),
            Some("posts") => Ok(Self::PostsOnly),
            Some("articles") => Ok(Self::ArticlesOnly),
            Some(_) => Err("kind must be one of: all, posts, articles"),
        }
    }
}

pub struct FeedPageFilters<F>
where
    F: IntoCondition,
{
    pub feed_condition: F,
    pub author: Option<Uuid>,
    pub category_filter: PostCategoryFilter,
    pub kind_filter: FeedKindFilter,
    pub cursor: Option<(DateTime<FixedOffset>, Uuid)>,
    pub limit: u64,
    pub search_term: Option<String>,
}

/// Returns one feed page. When `search_term` is set the order is BM25
/// relevance and pagination collapses to a single page; otherwise the
/// caller's cursor + (published_at, id) ordering applies.
pub async fn list_feed_page<F>(
    db: &DatabaseConnection,
    filters: FeedPageFilters<F>,
) -> Result<Vec<post::Model>, DbErr>
where
    F: IntoCondition,
{
    let mut q = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(filters.feed_condition);

    // Kind partitioning. `All` interleaves posts and articles but never
    // surfaces albums (they have their own listing). Drafts are
    // `kind='article' AND article_details.is_draft=true` with
    // `visibility='private'`. The visibility predicate keeps private rows
    // hidden from other viewers, but `feed_condition` includes an
    // author-override so the author themselves can see their own private
    // rows; without an explicit draft exclusion the author's drafts leak
    // into their own feed. NOT IN a subquery over `article_details` keeps
    // drafts out of all feed shapes (covers All and ArticlesOnly; posts
    // don't have detail rows so the predicate is a no-op for PostsOnly).
    match filters.kind_filter {
        FeedKindFilter::All => {
            q = q.filter(
                sea_orm::Condition::any()
                    .add(post::Column::Kind.eq(post::KIND_POST))
                    .add(post::Column::Kind.eq(post::KIND_ARTICLE)),
            );
        }
        FeedKindFilter::PostsOnly => {
            q = q.filter(post::Column::Kind.eq(post::KIND_POST));
        }
        FeedKindFilter::ArticlesOnly => {
            q = q.filter(post::Column::Kind.eq(post::KIND_ARTICLE));
        }
    }
    q = q.filter(post::Column::Id.not_in_subquery(draft_post_ids_subquery()));

    if let Some(author_id) = filters.author {
        q = q.filter(post::Column::AuthorId.eq(author_id));
    }
    match filters.category_filter {
        PostCategoryFilter::Any => {}
        PostCategoryFilter::Uncategorized => {
            q = q.filter(post::Column::CategoryId.is_null());
        }
        PostCategoryFilter::InCategory(id) => {
            q = q.filter(post::Column::CategoryId.eq(id));
        }
    }

    if let Some(term) = filters.search_term {
        // BM25 search via pg_search splices into the rendered SeaORM SQL
        // by hand. sea-query's cust_with_values doesn't merge cust
        // placeholder values into the prepared statement's bind vec in
        // this version, so we render the query (with the BM25 ORDER BY
        // but no LIMIT), string-insert ` AND <bm25-expr>` before ORDER
        // BY, append the term to the values vec, and tack on a clamped
        // LIMIT literal. LIMIT inline is safe because `limit` is a u64
        // already clamped server-side.
        let backend: DatabaseBackend = db.get_database_backend();
        let (rendered_sql, rendered_values) = q
            .order_by_desc(Expr::cust("paradedb.score(id)"))
            .order_by_desc(post::Column::Id)
            .as_query()
            .to_owned()
            .build_any(&*backend.get_query_builder());
        let n = rendered_values.0.len() + 1;
        let bm25_clause = format!(
            " AND id @@@ paradedb.boolean(should => ARRAY[\
             paradedb.match('body', ${n}, distance => 1), \
             paradedb.match('body_ngram', ${n}), \
             paradedb.match('slug', ${n}, distance => 1), \
             paradedb.match('slug_ngram', ${n})])"
        );
        let order_pos = rendered_sql.find(" ORDER BY").unwrap_or(rendered_sql.len());
        let mut final_sql = String::with_capacity(rendered_sql.len() + bm25_clause.len() + 32);
        final_sql.push_str(&rendered_sql[..order_pos]);
        final_sql.push_str(&bm25_clause);
        final_sql.push_str(&rendered_sql[order_pos..]);
        final_sql.push_str(&format!(" LIMIT {}", filters.limit));
        let mut all_values = rendered_values.0;
        all_values.push(sea_orm::Value::from(term));
        let stmt = Statement::from_sql_and_values(backend, final_sql, all_values);
        return Post::find().from_raw_sql(stmt).all(db).await;
    }

    q = q
        .order_by_desc(post::Column::PublishedAt)
        .order_by_desc(post::Column::Id);
    if let Some((cursor_published_at, cursor_id)) = filters.cursor {
        q = q.filter(
            sea_orm::Condition::any()
                .add(post::Column::PublishedAt.lt(cursor_published_at))
                .add(
                    sea_orm::Condition::all()
                        .add(post::Column::PublishedAt.eq(cursor_published_at))
                        .add(post::Column::Id.lt(cursor_id)),
                ),
        );
    }
    q.limit(filters.limit).all(db).await
}
