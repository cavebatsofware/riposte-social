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
//! HTTP handlers for articles.
//!
//! Route map:
//! - `POST   /api/articles` create (JSON body); composer mints a draft on
//!   demand and the publish path uses `is_draft=false`.
//! - `GET    /api/articles` published article list, visibility filtered;
//!   drafts excluded.
//! - `GET    /api/articles/drafts` authed-only; current user's drafts.
//! - `GET    /api/articles/{id}` single article view.
//! - `PATCH  /api/articles/{id}` author/admin edit; flipping
//!   `is_draft=false` is the publish action.
//! - `DELETE /api/articles/{id}` author/admin soft delete.
//! - `GET    /api/users/{user_id}/articles` per-author published articles.
//!
//! Article inline images and the cover image upload through
//! `POST /api/articles/{id}/media`. The handler shells out to the shared
//! `posts::media::append_media` helper (the underlying storage is the
//! same `post_media` table, just gated on `kind='article'`).

use crate::articles::queries::{self, ArticleCategoryFilter, ArticlePageFilters};
use crate::articles::types::{
    build_article_response, build_article_summary, compute_reading_time_minutes, derive_excerpt,
    ArticleResponse, ArticleSummary, CreateArticleRequest, ListArticlesQuery, ListArticlesResponse,
    UpdateArticleRequest,
};
use crate::articles::ArticlesState;
use crate::engagement::{fetch_engagement_for_posts, PostEngagement};
use crate::entities::{article_details, post, post_media, user};
use crate::errors::{AppError, AppResult};
use crate::middleware::AuthenticatedUser;
use crate::posts::media as posts_media;
use crate::posts::types::PostMediaResponse;
use crate::posts::FeedTier;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Extension, Router,
};
use axum_login::AuthSession;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};
use uuid::Uuid;

pub fn article_write_routes() -> Router<ArticlesState> {
    Router::new()
        .route("/api/articles", post(create_article))
        .route(
            "/api/articles/{id}",
            axum::routing::patch(update_article).delete(delete_article),
        )
        .route("/api/articles/{id}/media", post(append_article_media))
}

async fn append_article_media(
    State(state): State<ArticlesState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<Vec<PostMediaResponse>>> {
    let rows = posts_media::append_media(
        &state.db,
        &state.s3,
        &state.settings,
        &user,
        id,
        post::KIND_ARTICLE,
        &mut multipart,
    )
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(PostMediaResponse::from_model)
            .collect(),
    ))
}

/// Authed-only read surface. Today this is just the caller's own drafts;
/// the handler relies on the `Extension<UserAuth>` injected by the
/// `require_authenticated` middleware applied at the router layer.
pub fn article_authed_read_routes() -> Router<ArticlesState> {
    Router::new().route("/api/articles/drafts", get(list_my_drafts))
}

pub fn article_read_routes() -> Router<ArticlesState> {
    Router::new()
        .route("/api/articles", get(list_articles))
        .route("/api/articles/{id}", get(get_article))
        .route("/api/users/{user_id}/articles", get(list_user_articles))
}

const ARTICLES_LIMIT_DEFAULT: u64 = 20;
const ARTICLES_LIMIT_MAX: u64 = 100;
const TITLE_MAX_LEN: usize = post::SLUG_MAX_LEN;
const SUBTITLE_MAX_LEN: usize = 240;
const EXCERPT_MAX_LEN: usize = 600;

async fn create_article(
    State(state): State<ArticlesState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Json(req): Json<CreateArticleRequest>,
) -> AppResult<(StatusCode, Json<ArticleResponse>)> {
    let title = validate_title(&req.title)?;
    let subtitle = validate_optional_string("subtitle", req.subtitle.as_deref(), SUBTITLE_MAX_LEN)?;
    let supplied_excerpt =
        validate_optional_string("excerpt", req.excerpt.as_deref(), EXCERPT_MAX_LEN)?;
    let body_raw = req.body.unwrap_or_default();
    let body = body_raw.trim_end().to_string();

    let visibility = if req.is_draft {
        post::VISIBILITY_PRIVATE.to_string()
    } else {
        let v = req.visibility.as_deref().unwrap_or(post::VISIBILITY_PUBLIC);
        if !post::is_valid_visibility(v) {
            return Err(AppError::ValidationError(format!(
                "Invalid visibility '{}'",
                v
            )));
        }
        v.to_string()
    };

    if let Some(cid) = req.category_id {
        let cat = queries::find_category(&state.db, cid)
            .await?
            .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
        crate::visibility::ensure_can_compose_into_category(&state.db, &user, &cat).await?;
    }

    // Reject cover_media_id at create time: the row doesn't exist yet, so
    // the only check we could run ("belongs to some article you own") is
    // weaker than the PATCH-time invariant ("attached to this article") and
    // would let a caller create an article whose cover points at another
    // article's media. The composer flow mints a draft first, uploads to it,
    // then PATCHes cover_media_id, so this path was never exercised.
    if req.cover_media_id.is_some() {
        return Err(AppError::ValidationError(
            "cover_media_id is not accepted on create; set it via PATCH after uploading media to the article".to_string(),
        ));
    }

    let excerpt = supplied_excerpt.or_else(|| derive_excerpt(&body));
    let reading_time = compute_reading_time_minutes(&body);
    let now = Utc::now();

    let txn = state.db.begin().await?;
    let new_id = Uuid::new_v4();
    let post_active = post::ActiveModel {
        id: Set(new_id),
        author_id: Set(user.id),
        body: Set(body),
        visibility: Set(visibility),
        published_at: Set(now.into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(None),
        category_id: Set(req.category_id),
        kind: Set(post::KIND_ARTICLE.to_string()),
        slug: Set(Some(title.clone())),
        ..Default::default()
    };
    let post_row = post_active.insert(&txn).await?;

    let details_active = article_details::ActiveModel {
        post_id: Set(post_row.id),
        subtitle: Set(subtitle),
        cover_media_id: Set(None),
        excerpt: Set(excerpt),
        reading_time_minutes: Set(reading_time),
        is_draft: Set(req.is_draft),
        ..Default::default()
    };
    let details_row = details_active.insert(&txn).await?;
    txn.commit().await?;

    // Cover is always None on the freshly-created row (rejected above); set
    // it via PATCH after the author uploads media to this article.
    let cover: Option<post_media::Model> = None;
    let author = queries::find_user(&state.db, post_row.author_id).await?;
    let category = match post_row.category_id {
        Some(cid) => queries::find_category(&state.db, cid).await?,
        None => None,
    };
    Ok((
        StatusCode::CREATED,
        Json(build_article_response(
            post_row,
            details_row,
            author.as_ref(),
            cover.as_ref(),
            PostEngagement::default(),
            category.as_ref(),
        )),
    ))
}

async fn get_article(
    State(state): State<ArticlesState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ArticleResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;

    let row = posts_media::load_post(&state.db, id, post::KIND_ARTICLE).await?;
    let category = match row.category_id {
        Some(cid) => queries::find_category(&state.db, cid).await?,
        None => None,
    };
    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    if !ctx.permits_read(row.author_id, &row.visibility, category.as_ref()) {
        return Err(AppError::NotFound("Article not found".to_string()));
    }

    let details = queries::find_article_details(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Article not found".to_string()))?;

    // Draft permalink is author-only (and admin). update_article pins
    // visibility='private' on draft rows so the visibility check above
    // already filters them for other viewers, but this is the same kind of
    // belt-and-suspenders check that posts make for hidden rows: a draft
    // is invisible to everyone except its author regardless of the row's
    // visibility value.
    let viewer = auth_session.user().await;
    let viewer_id = viewer.as_ref().map(|u| u.id);
    let viewer_is_admin = viewer
        .as_ref()
        .is_some_and(|u| u.role == user::ROLE_ADMINISTRATOR);
    if details.is_draft && viewer_id != Some(row.author_id) && !viewer_is_admin {
        return Err(AppError::NotFound("Article not found".to_string()));
    }
    let cover = match details.cover_media_id {
        Some(media_id) => queries::find_cover_media(&state.db, media_id).await?,
        None => None,
    };
    let author = queries::find_user(&state.db, row.author_id).await?;
    let mut engagement_map =
        fetch_engagement_for_posts(&state.db, &[row.id], ctx.viewer_id).await?;
    let engagement = engagement_map.remove(&row.id).unwrap_or_default();
    Ok(Json(build_article_response(
        row,
        details,
        author.as_ref(),
        cover.as_ref(),
        engagement,
        category.as_ref(),
    )))
}

async fn update_article(
    State(state): State<ArticlesState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateArticleRequest>,
) -> AppResult<Json<ArticleResponse>> {
    let row = posts_media::load_owned_post(&state.db, id, post::KIND_ARTICLE, &user).await?;
    let details = queries::find_article_details(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Article not found".to_string()))?;

    if let Some(ref v) = req.visibility {
        if !post::is_valid_visibility(v) {
            return Err(AppError::ValidationError(format!(
                "Invalid visibility '{}'",
                v
            )));
        }
    }
    let new_title = match req.title.as_deref() {
        Some(t) => Some(validate_title(t)?),
        None => None,
    };
    let new_subtitle = match req.subtitle.as_deref() {
        Some(s) => Some(validate_optional_string(
            "subtitle",
            Some(s),
            SUBTITLE_MAX_LEN,
        )?),
        None => None,
    };
    let new_excerpt = match req.excerpt.as_deref() {
        Some(s) => Some(validate_optional_string(
            "excerpt",
            Some(s),
            EXCERPT_MAX_LEN,
        )?),
        None => None,
    };

    if let Some(cid) = req.category_id {
        if !req.clear_category {
            let cat = queries::find_category(&state.db, cid)
                .await?
                .ok_or_else(|| AppError::ValidationError("Category not found".to_string()))?;
            crate::visibility::ensure_can_compose_into_category(&state.db, &user, &cat).await?;
        }
    }

    if let Some(cover_id) = req.cover_media_id {
        if !req.clear_cover {
            let m = queries::find_cover_media(&state.db, cover_id)
                .await?
                .ok_or_else(|| AppError::ValidationError("cover_media_id not found".to_string()))?;
            if m.post_id != id {
                return Err(AppError::ValidationError(
                    "cover_media_id must refer to a media item attached to this article"
                        .to_string(),
                ));
            }
        }
    }

    // Publish toggle: a draft becoming non-draft bumps published_at to now,
    // and applies the submitted visibility tier (or VISIBILITY_PUBLIC default).
    // Republishing a published article doesn't reset published_at.
    let is_publishing = matches!(req.is_draft, Some(false)) && details.is_draft;

    let txn = state.db.begin().await?;
    let mut post_active: post::ActiveModel = row.clone().into();
    if let Some(ref t) = new_title {
        post_active.slug = Set(Some(t.clone()));
    }
    let new_body_value: Option<String> = req.body.clone();
    if let Some(body) = req.body {
        post_active.body = Set(body.trim_end().to_string());
    }
    // Draft rows must stay `visibility='private'` so the row-level visibility
    // predicate keeps them out of every listing for non-authors. Pin it
    // server-side regardless of the supplied value while the row remains a
    // draft after this PATCH; defense-in-depth against a client that PATCHes
    // visibility without also flipping `is_draft=false` to publish.
    let resulting_is_draft = req.is_draft.unwrap_or(details.is_draft);
    if resulting_is_draft {
        post_active.visibility = Set(post::VISIBILITY_PRIVATE.to_string());
    } else if let Some(v) = req.visibility.clone() {
        post_active.visibility = Set(v);
    } else if is_publishing {
        // Default new visibility on publish is public unless the caller
        // explicitly set one alongside is_draft=false.
        post_active.visibility = Set(post::VISIBILITY_PUBLIC.to_string());
    }
    if req.clear_category {
        post_active.category_id = Set(None);
    } else if let Some(cid) = req.category_id {
        post_active.category_id = Set(Some(cid));
    }
    if is_publishing {
        post_active.published_at = Set(Utc::now().into());
    }
    let updated_post: post::Model = post_active.update(&txn).await?;

    let mut details_active: article_details::ActiveModel = details.clone().into();
    if let Some(subtitle) = new_subtitle {
        details_active.subtitle = Set(subtitle);
    }
    if let Some(excerpt) = new_excerpt {
        details_active.excerpt = Set(excerpt);
    } else if new_body_value.is_some() && details.excerpt.is_none() {
        // Body changed and there's no author-set excerpt: refresh the
        // auto-derived one from the new body.
        details_active.excerpt = Set(derive_excerpt(&updated_post.body));
    }
    if new_body_value.is_some() {
        details_active.reading_time_minutes = Set(compute_reading_time_minutes(&updated_post.body));
    }
    if req.clear_cover {
        details_active.cover_media_id = Set(None);
    } else if let Some(cover_id) = req.cover_media_id {
        details_active.cover_media_id = Set(Some(cover_id));
    }
    if let Some(flag) = req.is_draft {
        // Going from published back to draft would create surprising side
        // effects (already-public links going 404, comments orphaned in a
        // private container). Disallow.
        if !details.is_draft && flag {
            return Err(AppError::ValidationError(
                "Cannot un-publish an article".to_string(),
            ));
        }
        details_active.is_draft = Set(flag);
    }
    let updated_details: article_details::Model = details_active.update(&txn).await?;
    txn.commit().await?;

    let cover = match updated_details.cover_media_id {
        Some(media_id) => queries::find_cover_media(&state.db, media_id).await?,
        None => None,
    };
    let author = queries::find_user(&state.db, updated_post.author_id).await?;
    let category = match updated_post.category_id {
        Some(cid) => queries::find_category(&state.db, cid).await?,
        None => None,
    };
    let mut engagement_map =
        fetch_engagement_for_posts(&state.db, &[updated_post.id], Some(user.id)).await?;
    let engagement = engagement_map.remove(&updated_post.id).unwrap_or_default();
    Ok(Json(build_article_response(
        updated_post,
        updated_details,
        author.as_ref(),
        cover.as_ref(),
        engagement,
        category.as_ref(),
    )))
}

async fn delete_article(
    State(state): State<ArticlesState>,
    Extension(user): Extension<crate::admin::UserAuth>,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let row = posts_media::load_owned_post(&state.db, id, post::KIND_ARTICLE, &user).await?;
    let mut active: post::ActiveModel = row.into();
    active.deleted_at = Set(Some(Utc::now().into()));
    active.update(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_articles(
    State(state): State<ArticlesState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Query(query): Query<ListArticlesQuery>,
) -> AppResult<Json<ListArticlesResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;
    let limit = query
        .limit
        .unwrap_or(ARTICLES_LIMIT_DEFAULT)
        .clamp(1, ARTICLES_LIMIT_MAX);

    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;

    let category_filter = match query.category.as_deref() {
        None => ArticleCategoryFilter::Any,
        Some(slug) => {
            let slug = slug.trim();
            if slug == "uncategorized" {
                ArticleCategoryFilter::Uncategorized
            } else {
                match queries::find_category_by_slug(&state.db, slug).await? {
                    Some(c) => ArticleCategoryFilter::InCategory(c.id),
                    None => {
                        return Ok(Json(ListArticlesResponse {
                            articles: vec![],
                            next_cursor: None,
                        }));
                    }
                }
            }
        }
    };

    let cursor = query.cursor.as_deref().and_then(parse_cursor);
    let filters = ArticlePageFilters {
        feed_condition: ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ),
        author: query.author,
        category_filter,
        cursor,
        fetch: limit + 1,
    };
    let rows = queries::list_article_posts(&state.db, filters).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<post::Model> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last().map(|last| {
            format!(
                "{}_{}",
                last.published_at.with_timezone(&Utc).to_rfc3339(),
                last.id
            )
        })
    } else {
        None
    };

    let summaries = hydrate_summaries(&state, page, ctx.viewer_id).await?;
    Ok(Json(ListArticlesResponse {
        articles: summaries,
        next_cursor,
    }))
}

async fn list_my_drafts(
    State(state): State<ArticlesState>,
    Extension(user): Extension<crate::admin::UserAuth>,
) -> AppResult<Json<ListArticlesResponse>> {
    let rows = queries::list_author_drafts(&state.db, user.id).await?;
    let summaries = hydrate_summaries(&state, rows, Some(user.id)).await?;
    Ok(Json(ListArticlesResponse {
        articles: summaries,
        next_cursor: None,
    }))
}

async fn list_user_articles(
    State(state): State<ArticlesState>,
    auth_session: AuthSession<crate::admin::UserAuthBackend>,
    Path(user_id): Path<Uuid>,
    Query(query): Query<ListArticlesQuery>,
) -> AppResult<Json<ListArticlesResponse>> {
    let tier = caller_tier(&auth_session).await;
    enforce_public_feed_gate(&state.settings, tier).await?;
    let limit = query
        .limit
        .unwrap_or(ARTICLES_LIMIT_DEFAULT)
        .clamp(1, ARTICLES_LIMIT_MAX);

    let ctx = crate::visibility::ViewerCtx::build(&state.db, &auth_session)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;

    let cursor = query.cursor.as_deref().and_then(parse_cursor);
    let filters = ArticlePageFilters {
        feed_condition: ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ),
        author: Some(user_id),
        category_filter: ArticleCategoryFilter::Any,
        cursor,
        fetch: limit + 1,
    };
    let rows = queries::list_article_posts(&state.db, filters).await?;
    let has_more = rows.len() as u64 > limit;
    let page: Vec<post::Model> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last().map(|last| {
            format!(
                "{}_{}",
                last.published_at.with_timezone(&Utc).to_rfc3339(),
                last.id
            )
        })
    } else {
        None
    };

    let summaries = hydrate_summaries(&state, page, ctx.viewer_id).await?;
    Ok(Json(ListArticlesResponse {
        articles: summaries,
        next_cursor,
    }))
}

async fn hydrate_summaries(
    state: &ArticlesState,
    page: Vec<post::Model>,
    viewer_id: Option<Uuid>,
) -> AppResult<Vec<ArticleSummary>> {
    if page.is_empty() {
        return Ok(Vec::new());
    }
    let author_ids: Vec<Uuid> = page.iter().map(|p| p.author_id).collect();
    let post_ids: Vec<Uuid> = page.iter().map(|p| p.id).collect();
    let category_ids: Vec<Uuid> = page.iter().filter_map(|p| p.category_id).collect();

    let authors = crate::posts::queries::load_users_by_ids(&state.db, author_ids).await?;
    let details_by_id =
        queries::load_article_details_for_posts(&state.db, post_ids.clone()).await?;
    let engagement_by_post = fetch_engagement_for_posts(&state.db, &post_ids, viewer_id).await?;
    let categories = crate::posts::queries::load_categories_by_ids(&state.db, category_ids).await?;

    let cover_ids: Vec<Uuid> = details_by_id
        .values()
        .filter_map(|d| d.cover_media_id)
        .collect();
    let covers = if cover_ids.is_empty() {
        Vec::new()
    } else {
        post_media::Entity::find()
            .filter(post_media::Column::Id.is_in(cover_ids))
            .all(&state.db)
            .await?
    };
    let cover_by_id: std::collections::HashMap<Uuid, post_media::Model> =
        covers.into_iter().map(|m| (m.id, m)).collect();
    let authors_by_id: std::collections::HashMap<Uuid, user::Model> =
        authors.into_iter().map(|a| (a.id, a)).collect();
    let categories_by_id: std::collections::HashMap<Uuid, crate::entities::category::Model> =
        categories.into_iter().map(|c| (c.id, c)).collect();

    let mut summaries = Vec::with_capacity(page.len());
    for row in page {
        let Some(details) = details_by_id.get(&row.id).cloned() else {
            // No detail row implies the integrity rule was violated. Skip
            // rather than 500 the whole page.
            continue;
        };
        let author = authors_by_id.get(&row.author_id);
        let cover = details
            .cover_media_id
            .as_ref()
            .and_then(|id| cover_by_id.get(id));
        let category = row
            .category_id
            .as_ref()
            .and_then(|id| categories_by_id.get(id));
        let engagement = engagement_by_post.get(&row.id).cloned().unwrap_or_default();
        summaries.push(build_article_summary(
            row,
            details,
            author,
            cover,
            &engagement,
            category,
        ));
    }
    Ok(summaries)
}

fn validate_title(input: &str) -> AppResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::ValidationError(
            "Article title is required".to_string(),
        ));
    }
    if trimmed.chars().count() > TITLE_MAX_LEN {
        return Err(AppError::ValidationError(format!(
            "Title exceeds {}-character limit",
            TITLE_MAX_LEN
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_optional_string(
    field: &str,
    input: Option<&str>,
    max_chars: usize,
) -> AppResult<Option<String>> {
    match input {
        None => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > max_chars {
                return Err(AppError::ValidationError(format!(
                    "{} exceeds {}-character limit",
                    field, max_chars
                )));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

async fn caller_tier(auth_session: &AuthSession<crate::admin::UserAuthBackend>) -> FeedTier {
    let user = auth_session.user().await;
    FeedTier::from_role(user.as_ref().map(|u| u.role.as_str()))
}

async fn enforce_public_feed_gate(
    settings: &crate::settings::SettingsService,
    tier: FeedTier,
) -> AppResult<()> {
    if !matches!(tier, FeedTier::Anonymous) {
        return Ok(());
    }
    let enabled = settings
        .get_public_feed_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))?;
    if enabled {
        return Ok(());
    }
    Err(AppError::NotFound("Not found".to_string()))
}

fn parse_cursor(cursor: &str) -> Option<(chrono::DateTime<chrono::FixedOffset>, Uuid)> {
    let (ts, id) = cursor.rsplit_once('_')?;
    let parsed_ts = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
    let parsed_id = Uuid::parse_str(id).ok()?;
    Some((parsed_ts, parsed_id))
}

#[allow(dead_code)]
type _Marker = AuthenticatedUser;
