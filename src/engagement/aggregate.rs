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

use crate::entities::{comment, reaction, Comment, Reaction};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult, QueryFilter,
    QuerySelect,
};
use std::collections::HashMap;
use uuid::Uuid;

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
