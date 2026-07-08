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
//! Open Graph / Twitter Card meta for the human-facing permalink pages
//! `/post/{id}` and `/articles/{id}`.
//!
//! These routes always return the SPA shell so the client can render the
//! interactive page (including a logged-in author viewing their own private
//! post via the authenticated API). What varies is whether per-content meta
//! is injected into `<head>`: it is emitted only for a row an anonymous
//! visitor may see, resolved through the same `ViewerCtx::anonymous` +
//! `feed_condition` predicate the public API and sitemap filter with, so a
//! share card can never leak non-public, draft, or missing content. When the
//! `public_feed_enabled` gate is off, or the id doesn't parse, or the row
//! isn't anonymously visible, the shell is served unchanged (no meta, no
//! 404, no redirect).

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use std::env;
use uuid::Uuid;

use crate::articles::queries::{draft_post_ids_subquery, find_article_details, find_cover_media};
use crate::articles::types::derive_excerpt;
use crate::entities::{post, post_media, Post, PostMedia};
use crate::errors::{AppError, AppResult};
use crate::settings::SettingsService;
use crate::visibility::ViewerCtx;

#[derive(Clone)]
pub struct OgState {
    pub db: DatabaseConnection,
    pub settings: SettingsService,
}

pub fn og_routes() -> Router<OgState> {
    Router::new()
        .route("/post/{id}", get(serve_post_og))
        .route("/articles/{id}", get(serve_article_og))
}

/// Origin for absolute og:url / og:image values. Mirrors sitemap.rs's
/// `site_url` so shared links and the sitemap agree on the canonical host.
fn site_url() -> String {
    env::var("SITE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Escape a value for interpolation into a double-quoted HTML attribute.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// One anonymously-visible, non-deleted, non-draft row by id. Mirrors
/// sitemap.rs's `anon_visible_posts`: `feed_condition` for an anonymous
/// viewer restricts to publicly reachable content, so a row returned here
/// is safe to expose in a share card.
async fn anon_visible_post(db: &DatabaseConnection, id: Uuid) -> AppResult<Option<post::Model>> {
    let ctx = ViewerCtx::anonymous(db)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    Ok(Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Id.eq(id))
        .filter(post::Column::Id.not_in_subquery(draft_post_ids_subquery()))
        .filter(ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ))
        .one(db)
        .await?)
}

/// Collected meta values for one piece of content. Absent fields are omitted
/// from the rendered `<head>` block.
struct OgMeta {
    title: String,
    description: Option<String>,
    image: Option<String>,
    url: String,
    site_name: String,
}

impl OgMeta {
    /// Render the `<meta>` block. `og:type` is `article` for both kinds
    /// (posts and articles are long-form-ish permalinks, not the site as a
    /// whole). `twitter:card` widens to a large image only when one exists.
    fn render(&self) -> String {
        let mut tags = String::new();
        let mut push = |property: &str, content: &str| {
            tags.push_str(&format!(
                "<meta property=\"{}\" content=\"{}\">",
                property,
                html_escape(content)
            ));
        };
        push("og:type", "article");
        push("og:title", &self.title);
        push("og:url", &self.url);
        push("og:site_name", &self.site_name);
        if let Some(desc) = &self.description {
            push("og:description", desc);
        }
        if let Some(image) = &self.image {
            push("og:image", image);
        }

        let card = if self.image.is_some() {
            "summary_large_image"
        } else {
            "summary"
        };
        tags.push_str(&format!(
            "<meta name=\"twitter:card\" content=\"{}\">",
            card
        ));
        tags.push_str(&format!(
            "<meta name=\"twitter:title\" content=\"{}\">",
            html_escape(&self.title)
        ));
        if let Some(desc) = &self.description {
            tags.push_str(&format!(
                "<meta name=\"twitter:description\" content=\"{}\">",
                html_escape(desc)
            ));
        }
        tags
    }
}

/// Read the SPA shell, apply the theme defaults via the shared
/// `shell::inject_theme_defaults` (the same substitution serve_spa uses in
/// main.rs), then inject `meta` immediately before `</head>`. An empty
/// `meta` yields the generic shell unchanged.
async fn serve_shell(settings: &SettingsService, meta: &str) -> AppResult<impl IntoResponse> {
    let html = tokio::fs::read_to_string("social-assets/index.html")
        .await
        .map_err(AppError::FileSystem)?;
    let colorway = settings.get_default_colorway().await.unwrap_or_default();
    let shade = settings.get_default_shade().await.unwrap_or_default();
    let html = crate::shell::inject_theme_defaults(html, &colorway, &shade);
    let html = if meta.is_empty() {
        html
    } else {
        html.replacen("</head>", &format!("{meta}</head>"), 1)
    };
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    ))
}

async fn public_feed_enabled(state: &OgState) -> AppResult<bool> {
    state
        .settings
        .get_public_feed_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))
}

async fn site_name(state: &OgState) -> String {
    state.settings.get_site_name().await.unwrap_or_default()
}

async fn serve_post_og(
    State(state): State<OgState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let meta = build_post_meta(&state, &id).await?;
    serve_shell(&state.settings, &meta).await
}

async fn serve_article_og(
    State(state): State<OgState>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let meta = build_article_meta(&state, &id).await?;
    serve_shell(&state.settings, &meta).await
}

/// Meta for `/post/{id}`. Empty string when the id doesn't parse, the feed
/// is gated off, or the row isn't anonymously visible.
async fn build_post_meta(state: &OgState, id: &str) -> AppResult<String> {
    if !public_feed_enabled(state).await? {
        return Ok(String::new());
    }
    let Ok(id) = Uuid::parse_str(id) else {
        return Ok(String::new());
    };
    let Some(row) = anon_visible_post(&state.db, id).await? else {
        return Ok(String::new());
    };
    // Albums and articles have their own routes; a wrong-kind id at
    // `/post/{id}` gets the generic shell rather than a card whose
    // `/post/{id}` canonical would 404 on click.
    if row.kind != post::KIND_POST {
        return Ok(String::new());
    }

    let name = site_name(state).await;
    let title = match row.slug.as_deref() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => format!("Post on {name}"),
    };
    let image = PostMedia::find()
        .filter(post_media::Column::PostId.eq(id))
        .order_by_asc(post_media::Column::Ordinal)
        .one(&state.db)
        .await?
        .map(|m| format!("{}/media/{}", site_url(), m.id));

    Ok(OgMeta {
        title,
        description: derive_excerpt(&row.body),
        image,
        url: format!("{}/post/{}", site_url(), id),
        site_name: name,
    }
    .render())
}

/// Meta for `/articles/{id}`. Article title lives on `post.slug`; the
/// author's excerpt wins over the derived one, and the cover media (when
/// set) becomes the og:image.
async fn build_article_meta(state: &OgState, id: &str) -> AppResult<String> {
    if !public_feed_enabled(state).await? {
        return Ok(String::new());
    }
    let Ok(id) = Uuid::parse_str(id) else {
        return Ok(String::new());
    };
    let Some(row) = anon_visible_post(&state.db, id).await? else {
        return Ok(String::new());
    };
    if row.kind != post::KIND_ARTICLE {
        return Ok(String::new());
    }

    let name = site_name(state).await;
    let title = match row.slug.as_deref() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => name.clone(),
    };
    let details = find_article_details(&state.db, id).await?;
    let description = details
        .as_ref()
        .and_then(|d| d.excerpt.clone())
        .or_else(|| derive_excerpt(&row.body));
    let image = match details.as_ref().and_then(|d| d.cover_media_id) {
        Some(cover_id) => find_cover_media(&state.db, cover_id)
            .await?
            .map(|m| format!("{}/media/{}", site_url(), m.id)),
        None => None,
    };

    Ok(OgMeta {
        title,
        description,
        image,
        url: format!("{}/articles/{}", site_url(), id),
        site_name: name,
    }
    .render())
}
