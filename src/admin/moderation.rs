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
//! Admin moderation endpoints.
//!
//! - `GET  /api/admin/moderation/comments` paginated list, newest first,
//!   includes author email + a body excerpt of the parent post so admins
//!   have context. `?include_deleted=true` surfaces previously moderated
//!   rows for audit; default is live-only.
//! - `POST /api/admin/moderation/comments/bulk-delete` soft-delete a batch
//!   by id. Returns the count of rows actually flipped from live to
//!   deleted (already-deleted rows count as no-ops so re-runs return 0).

use crate::admin::pagination::{Paginated, PaginationParams};
use crate::entities::{comment, post, user, Comment, Post, User};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::markdown;
use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, Order, PaginatorTrait, QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct ModerationState {
    pub db: DatabaseConnection,
}

pub fn moderation_routes() -> Router<ModerationState> {
    Router::new()
        .route(
            "/api/admin/moderation/comments",
            get(list_comments_for_moderation),
        )
        .route(
            "/api/admin/moderation/comments/bulk-delete",
            post(bulk_delete_comments),
        )
}

#[derive(Deserialize)]
pub struct ModerationQuery {
    #[serde(default)]
    pub page: Option<u64>,
    #[serde(default)]
    pub per_page: Option<u64>,
    /// When true, include rows whose `deleted_at` is set. Default false.
    #[serde(default)]
    pub include_deleted: bool,
}

#[derive(Serialize)]
pub struct ModerationCommentRow {
    pub id: Uuid,
    pub post_id: Uuid,
    /// First ~120 chars of the parent post body so admins can identify the
    /// thread without clicking through.
    pub post_excerpt: String,
    pub user_id: Uuid,
    /// Author's email. Surfacing this is fine here because the endpoint is
    /// admin-only; the public comment payload still hides email.
    pub author_email: Option<String>,
    pub author_display: Option<String>,
    pub body: String,
    pub body_html: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

async fn list_comments_for_moderation(
    State(state): State<ModerationState>,
    _user: AuthenticatedUser,
    Query(params): Query<ModerationQuery>,
) -> AppResult<Json<Paginated<ModerationCommentRow>>> {
    let pagination = PaginationParams {
        page: params.page.unwrap_or(1),
        per_page: params.per_page.unwrap_or(50),
    }
    .validate();

    let mut query = Comment::find().order_by(comment::Column::CreatedAt, Order::Desc);
    if !params.include_deleted {
        query = query.filter(comment::Column::DeletedAt.is_null());
    }

    let paginator = query.paginate(&state.db, pagination.per_page);
    let total = paginator.num_items().await?;
    let total_pages = paginator.num_pages().await?;
    let rows = paginator.fetch_page(pagination.page - 1).await?;

    // Batch-fetch authors and posts to avoid N+1.
    let user_ids: Vec<Uuid> = rows.iter().map(|c| c.user_id).collect();
    let post_ids: Vec<Uuid> = rows.iter().map(|c| c.post_id).collect();

    let authors: HashMap<Uuid, user::Model> = if user_ids.is_empty() {
        HashMap::new()
    } else {
        User::find()
            .filter(user::Column::Id.is_in(user_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect()
    };

    let posts: HashMap<Uuid, post::Model> = if post_ids.is_empty() {
        HashMap::new()
    } else {
        Post::find()
            .filter(post::Column::Id.is_in(post_ids))
            .all(&state.db)
            .await?
            .into_iter()
            .map(|p| (p.id, p))
            .collect()
    };

    let utc = Utc;
    let data: Vec<ModerationCommentRow> = rows
        .into_iter()
        .map(|row| {
            let author = authors.get(&row.user_id);
            let post_excerpt = posts
                .get(&row.post_id)
                .map(|p| excerpt(&p.body, 120))
                .unwrap_or_default();
            let body_html = markdown::render_to_html(&row.body);
            ModerationCommentRow {
                id: row.id,
                post_id: row.post_id,
                post_excerpt,
                user_id: row.user_id,
                author_email: author.map(|u| u.email.clone()),
                author_display: author.and_then(|u| u.display_name.clone()),
                body: row.body,
                body_html,
                created_at: row.created_at.with_timezone(&utc).to_rfc3339(),
                updated_at: row.updated_at.with_timezone(&utc).to_rfc3339(),
                deleted_at: row
                    .deleted_at
                    .map(|d| d.with_timezone(&utc).to_rfc3339()),
            }
        })
        .collect();

    Ok(Json(Paginated::new(
        data,
        total,
        pagination.page,
        pagination.per_page,
        total_pages,
    )))
}

#[derive(Deserialize)]
pub struct BulkDeleteRequest {
    pub comment_ids: Vec<Uuid>,
}

#[derive(Serialize)]
pub struct BulkDeleteResponse {
    /// Rows that flipped from live to soft-deleted by this call. Does not
    /// include rows that were already soft-deleted, which are silent no-ops.
    pub deleted: u64,
}

async fn bulk_delete_comments(
    State(state): State<ModerationState>,
    _user: AuthenticatedUser,
    Json(req): Json<BulkDeleteRequest>,
) -> AppResult<Json<BulkDeleteResponse>> {
    if req.comment_ids.is_empty() {
        return Err(AppError::ValidationError(
            "comment_ids cannot be empty".to_string(),
        ));
    }
    if req.comment_ids.len() > 200 {
        return Err(AppError::ValidationError(
            "At most 200 comment_ids per request".to_string(),
        ));
    }

    let now: sea_orm::prelude::DateTimeWithTimeZone = Utc::now().into();
    let result = Comment::update_many()
        .col_expr(
            comment::Column::DeletedAt,
            sea_orm::sea_query::Expr::value(now),
        )
        // Filter to live rows only so the response count reflects new
        // moderations and a re-run returns 0 instead of bumping the timestamp.
        .filter(comment::Column::DeletedAt.is_null())
        .filter(comment::Column::Id.is_in(req.comment_ids))
        .exec(&state.db)
        .await?;

    if result.rows_affected > 0 {
        crate::metrics::COMMENTS_TOTAL
            .with_label_values(&["soft_delete"])
            .inc_by(result.rows_affected);
    }
    Ok(Json(BulkDeleteResponse {
        deleted: result.rows_affected,
    }))
}

/// Trim a markdown source to its first `max_chars` characters, ending with
/// "..." when truncated. Iterates by chars (not bytes) so multi-byte content
/// is not sliced mid-character.
fn excerpt(body: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in body.chars().enumerate() {
        if i >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}
