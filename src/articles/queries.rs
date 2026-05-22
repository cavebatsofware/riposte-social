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
//! Article-specific query helpers.
//!
//! Post/album code never reaches into this module: the `article_details`
//! relation is one-way (only the article entity advertises `Related<post>`),
//! and every join here is guarded by `kind='article'` on the WHERE side.

use crate::entities::{
    article_details, category, post, post_media, ArticleDetails, Category, Post, PostMedia, User,
};
use chrono::{DateTime, FixedOffset};
use sea_orm::sea_query::IntoCondition;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use std::collections::HashMap;
use uuid::Uuid;

pub async fn find_article_details<C: ConnectionTrait>(
    conn: &C,
    post_id: Uuid,
) -> Result<Option<article_details::Model>, DbErr> {
    ArticleDetails::find_by_id(post_id).one(conn).await
}

pub async fn find_cover_media<C: ConnectionTrait>(
    conn: &C,
    media_id: Uuid,
) -> Result<Option<post_media::Model>, DbErr> {
    PostMedia::find_by_id(media_id).one(conn).await
}

/// Batch-load article_details rows for a set of post ids. The caller has
/// already partitioned the post page by `kind='article'`; this only
/// projects detail rows for the article subset.
pub async fn load_article_details_for_posts<C: ConnectionTrait>(
    conn: &C,
    post_ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, article_details::Model>, DbErr> {
    if post_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = ArticleDetails::find()
        .filter(article_details::Column::PostId.is_in(post_ids))
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(|r| (r.post_id, r)).collect())
}

pub struct ArticlePageFilters<F>
where
    F: IntoCondition,
{
    pub feed_condition: F,
    pub author: Option<Uuid>,
    pub category_filter: ArticleCategoryFilter,
    pub cursor: Option<(DateTime<FixedOffset>, Uuid)>,
    pub fetch: u64,
}

pub enum ArticleCategoryFilter {
    Any,
    Uncategorized,
    InCategory(Uuid),
}

/// List published article posts (kind='article'), visibility-filtered.
/// Drafts have `visibility='private'`, so the existing private-tier rule
/// in `feed_condition` already excludes them for non-authors. To keep
/// the public `/api/articles` surface entirely draft-free even for the
/// author, we also exclude any row with an `article_details.is_draft=true`
/// row via a NOT EXISTS subquery.
pub async fn list_article_posts<C, F>(
    conn: &C,
    filters: ArticlePageFilters<F>,
) -> Result<Vec<post::Model>, DbErr>
where
    C: ConnectionTrait,
    F: IntoCondition,
{
    let mut q = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(post::KIND_ARTICLE))
        .filter(filters.feed_condition)
        .filter(
            sea_orm::sea_query::Expr::cust(
                "NOT EXISTS (SELECT 1 FROM article_details ad \
                 WHERE ad.post_id = posts.id AND ad.is_draft = true)",
            ),
        )
        .order_by_desc(post::Column::PublishedAt)
        .order_by_desc(post::Column::Id);

    if let Some(author_id) = filters.author {
        q = q.filter(post::Column::AuthorId.eq(author_id));
    }
    match filters.category_filter {
        ArticleCategoryFilter::Any => {}
        ArticleCategoryFilter::Uncategorized => {
            q = q.filter(post::Column::CategoryId.is_null());
        }
        ArticleCategoryFilter::InCategory(id) => {
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

/// Author-owned drafts, newest first. Always scoped to the supplied
/// author_id and joined to article_details on is_draft=true; the caller
/// is the only viewer allowed to see these rows.
pub async fn list_author_drafts<C: ConnectionTrait>(
    conn: &C,
    author_id: Uuid,
) -> Result<Vec<post::Model>, DbErr> {
    Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(post::KIND_ARTICLE))
        .filter(post::Column::AuthorId.eq(author_id))
        .filter(
            sea_orm::sea_query::Expr::cust(
                "EXISTS (SELECT 1 FROM article_details ad \
                 WHERE ad.post_id = posts.id AND ad.is_draft = true)",
            ),
        )
        .order_by_desc(post::Column::UpdatedAt)
        .order_by_desc(post::Column::Id)
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

pub async fn find_user<C: ConnectionTrait>(
    conn: &C,
    id: Uuid,
) -> Result<Option<crate::entities::user::Model>, DbErr> {
    User::find_by_id(id).one(conn).await
}
