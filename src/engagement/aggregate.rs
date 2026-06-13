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
//! Batched lookup of reaction counts, comment counts, and the caller's own
//! reactions for a set of posts. Used by the post/feed handlers to enrich
//! the response shape without N+1 queries.

use crate::entities::{
    comment, post_media_comment, post_media_reaction, reaction, Comment, PostMediaComment,
    PostMediaReaction, Reaction,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult,
    QueryFilter, QuerySelect, Statement,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Number of latest live comments to surface on each feed PostCard.
const TOP_COMMENTS_PER_POST: usize = 3;

/// Engagement counts and viewer state for one post.
#[derive(Default, Debug, Clone)]
pub struct PostEngagement {
    /// Map of `kind` to count. Kinds with zero count are absent.
    pub reaction_counts: HashMap<String, i64>,
    /// Kinds the caller has reacted with. Empty for anonymous callers and
    /// for callers who haven't reacted.
    pub viewer_reaction_kinds: Vec<String>,
    /// Live (non-soft-deleted) comments on this post.
    pub comment_count: i64,
    /// The latest live comments on this post (newest-first, up to
    /// `TOP_COMMENTS_PER_POST`). Used by the feed PostCard to surface
    /// inline conversation context. Author lookups happen at the call
    /// site so the response builder can hydrate display name + avatar.
    pub top_comments: Vec<comment::Model>,
}

/// Fetch engagement summaries for the given post IDs. The returned map only
/// contains entries for posts that have at least one reaction or comment;
/// callers should treat absence as zero counts.
pub async fn fetch_engagement_for_posts(
    db: &DatabaseConnection,
    post_ids: &[Uuid],
    viewer_id: Option<Uuid>,
) -> Result<HashMap<Uuid, PostEngagement>, DbErr> {
    if post_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut out: HashMap<Uuid, PostEngagement> = HashMap::new();

    // 1) reaction counts grouped by (post_id, kind)
    let counts: Vec<ReactionCountRow> = Reaction::find()
        .select_only()
        .column(reaction::Column::PostId)
        .column(reaction::Column::Kind)
        .column_as(reaction::Column::Id.count(), "count")
        .filter(reaction::Column::PostId.is_in(post_ids.to_vec()))
        .group_by(reaction::Column::PostId)
        .group_by(reaction::Column::Kind)
        .into_model::<ReactionCountRow>()
        .all(db)
        .await?;

    for row in counts {
        out.entry(row.post_id)
            .or_default()
            .reaction_counts
            .insert(row.kind, row.count);
    }

    // 2) viewer's own reactions across these posts
    if let Some(viewer) = viewer_id {
        let mine: Vec<ViewerReactionRow> = Reaction::find()
            .select_only()
            .column(reaction::Column::PostId)
            .column(reaction::Column::Kind)
            .filter(reaction::Column::PostId.is_in(post_ids.to_vec()))
            .filter(reaction::Column::UserId.eq(viewer))
            .into_model::<ViewerReactionRow>()
            .all(db)
            .await?;
        for row in mine {
            out.entry(row.post_id)
                .or_default()
                .viewer_reaction_kinds
                .push(row.kind);
        }
    }

    // 3) live comment counts
    let comment_counts: Vec<CommentCountRow> = Comment::find()
        .select_only()
        .column(comment::Column::PostId)
        .column_as(comment::Column::Id.count(), "count")
        .filter(comment::Column::PostId.is_in(post_ids.to_vec()))
        .filter(comment::Column::DeletedAt.is_null())
        .group_by(comment::Column::PostId)
        .into_model::<CommentCountRow>()
        .all(db)
        .await?;

    for row in comment_counts {
        out.entry(row.post_id).or_default().comment_count = row.count;
    }

    // 4) latest live comments per post, capped at TOP_COMMENTS_PER_POST each.
    // A LATERAL join reads only the top few rows per post via the existing
    // `idx_comments_post_thread` index (post_id, deleted_at, created_at, id)
    // instead of fetching every live comment on the page and trimming in
    // memory. Post IDs expand into rows with `unnest`; the cap is a const,
    // so it is inlined safely. The outer ORDER BY is explicit: a LATERAL
    // join does not guarantee output row order, and the caller relies on
    // each post's comments arriving newest-first.
    let backend = db.get_database_backend();
    let id_placeholders = (1..=post_ids.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT c.* \
         FROM unnest(ARRAY[{id_placeholders}]::uuid[]) AS p(post_id) \
         CROSS JOIN LATERAL ( \
            SELECT * FROM comments \
            WHERE post_id = p.post_id AND deleted_at IS NULL \
            ORDER BY created_at DESC, id DESC \
            LIMIT {TOP_COMMENTS_PER_POST} \
         ) c \
         ORDER BY c.created_at DESC, c.id DESC"
    );
    let values: Vec<sea_orm::Value> = post_ids.iter().map(|id| (*id).into()).collect();
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    let top_comments = Comment::find().from_raw_sql(stmt).all(db).await?;

    for row in top_comments {
        out.entry(row.post_id).or_default().top_comments.push(row);
    }

    Ok(out)
}

#[derive(FromQueryResult)]
struct ReactionCountRow {
    post_id: Uuid,
    kind: String,
    count: i64,
}

#[derive(FromQueryResult)]
struct ViewerReactionRow {
    post_id: Uuid,
    kind: String,
}

#[derive(FromQueryResult)]
struct CommentCountRow {
    post_id: Uuid,
    count: i64,
}

/// Engagement summary for one media item: per-kind reaction counts, the
/// caller's own reactions, and a live comment count. Mirrors
/// `PostEngagement` but keyed by `post_media_id` and without the
/// `top_comments` field, since media-level conversation is loaded lazily by
/// the lightbox rather than eagerly with the post payload.
#[derive(Default, Debug, Clone)]
pub struct MediaEngagement {
    pub reaction_counts: HashMap<String, i64>,
    pub viewer_reaction_kinds: Vec<String>,
    pub comment_count: i64,
}

#[derive(FromQueryResult)]
struct MediaReactionCountRow {
    post_media_id: Uuid,
    kind: String,
    count: i64,
}

#[derive(FromQueryResult)]
struct MediaViewerReactionRow {
    post_media_id: Uuid,
    kind: String,
}

#[derive(FromQueryResult)]
struct MediaCommentCountRow {
    post_media_id: Uuid,
    count: i64,
}

/// Batched lookup of reaction counts, viewer reactions, and live comment
/// counts for a list of media IDs. Lazy-loaded by the lightbox on open
/// rather than eagerly attached to feed/post payloads, so the per-feed
/// row volume stays bounded by the post count instead of growing with
/// the album photo count.
pub async fn fetch_engagement_for_media(
    db: &DatabaseConnection,
    media_ids: &[Uuid],
    viewer_id: Option<Uuid>,
) -> Result<HashMap<Uuid, MediaEngagement>, DbErr> {
    if media_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut out: HashMap<Uuid, MediaEngagement> = HashMap::new();

    let counts: Vec<MediaReactionCountRow> = PostMediaReaction::find()
        .select_only()
        .column(post_media_reaction::Column::PostMediaId)
        .column(post_media_reaction::Column::Kind)
        .column_as(post_media_reaction::Column::Id.count(), "count")
        .filter(post_media_reaction::Column::PostMediaId.is_in(media_ids.to_vec()))
        .group_by(post_media_reaction::Column::PostMediaId)
        .group_by(post_media_reaction::Column::Kind)
        .into_model::<MediaReactionCountRow>()
        .all(db)
        .await?;
    for row in counts {
        out.entry(row.post_media_id)
            .or_default()
            .reaction_counts
            .insert(row.kind, row.count);
    }

    if let Some(viewer) = viewer_id {
        let mine: Vec<MediaViewerReactionRow> = PostMediaReaction::find()
            .select_only()
            .column(post_media_reaction::Column::PostMediaId)
            .column(post_media_reaction::Column::Kind)
            .filter(post_media_reaction::Column::PostMediaId.is_in(media_ids.to_vec()))
            .filter(post_media_reaction::Column::UserId.eq(viewer))
            .into_model::<MediaViewerReactionRow>()
            .all(db)
            .await?;
        for row in mine {
            out.entry(row.post_media_id)
                .or_default()
                .viewer_reaction_kinds
                .push(row.kind);
        }
    }

    let comment_counts: Vec<MediaCommentCountRow> = PostMediaComment::find()
        .select_only()
        .column(post_media_comment::Column::PostMediaId)
        .column_as(post_media_comment::Column::Id.count(), "count")
        .filter(post_media_comment::Column::PostMediaId.is_in(media_ids.to_vec()))
        .filter(post_media_comment::Column::DeletedAt.is_null())
        .group_by(post_media_comment::Column::PostMediaId)
        .into_model::<MediaCommentCountRow>()
        .all(db)
        .await?;
    for row in comment_counts {
        out.entry(row.post_media_id).or_default().comment_count = row.count;
    }

    Ok(out)
}
