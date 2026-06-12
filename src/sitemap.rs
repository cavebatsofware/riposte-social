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
//! XML sitemaps for the social surface, generated from the database on
//! request.
//!
//! `robots.txt` advertises `/sitemap-index.xml`; the index references
//! one section sitemap per public content kind plus the static listing
//! pages:
//! - `/sitemaps/static.xml`   landing + public listing pages
//! - `/sitemaps/posts.xml`    `/post/{id}`
//! - `/sitemaps/albums.xml`   `/album/{id}`
//! - `/sitemaps/articles.xml` `/articles/{id}` (drafts excluded)
//! - `/sitemaps/profiles.xml` `/u/{handle}` for authors of public content
//!
//! Every content query runs through `ViewerCtx::anonymous` +
//! `feed_condition`, the same predicate the public API filters with, so
//! the sitemap can never list a URL an anonymous visitor would get a 404
//! for. The `public_feed_enabled` gate is honored the same way the read
//! handlers honor it: when off, the content sections are empty and the
//! index only references the static sitemap (which shrinks to `/`).

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Select,
};
use std::env;
use uuid::Uuid;

use crate::articles::queries::draft_post_ids_subquery;
use crate::entities::{post, user, Post, User};
use crate::errors::{AppError, AppResult};
use crate::settings::SettingsService;
use crate::visibility::ViewerCtx;

/// Sitemap-protocol cap on URLs per file. Beyond this the section needs
/// pagination; warn so the operator notices before crawlers do.
const URLS_PER_FILE: u64 = 50_000;

#[derive(Clone)]
pub struct SitemapState {
    pub db: DatabaseConnection,
    pub settings: SettingsService,
}

pub fn sitemap_routes() -> Router<SitemapState> {
    Router::new()
        .route("/sitemap-index.xml", get(serve_index))
        // Crawlers probe /sitemap.xml unprompted; alias it to the index
        // so the probe gets XML instead of the SPA fallback's HTML.
        .route("/sitemap.xml", get(serve_index))
        .route("/sitemaps/static.xml", get(serve_static))
        .route("/sitemaps/posts.xml", get(serve_posts))
        .route("/sitemaps/albums.xml", get(serve_albums))
        .route("/sitemaps/articles.xml", get(serve_articles))
        .route("/sitemaps/profiles.xml", get(serve_profiles))
}

/// Origin for every `<loc>`. Same variable `serve_robots` builds the
/// `Sitemap:` line from, so the two always agree.
fn site_url() -> String {
    env::var("SITE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn xml_response(body: String) -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/xml"),
            // Crawl frequency, not user latency, bounds freshness here;
            // an hour keeps DB load negligible without staling the index.
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
}

fn lastmod_value(dt: &chrono::DateTime<chrono::FixedOffset>) -> String {
    dt.with_timezone(&Utc).to_rfc3339()
}

/// Render a `<urlset>`. Entries are (absolute URL, optional lastmod).
fn urlset(entries: &[(String, Option<String>)]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for (loc, lastmod) in entries {
        out.push_str("  <url><loc>");
        out.push_str(&xml_escape(loc));
        out.push_str("</loc>");
        if let Some(lm) = lastmod {
            out.push_str("<lastmod>");
            out.push_str(lm);
            out.push_str("</lastmod>");
        }
        out.push_str("</url>\n");
    }
    out.push_str("</urlset>\n");
    out
}

async fn public_feed_enabled(state: &SitemapState) -> AppResult<bool> {
    state
        .settings
        .get_public_feed_enabled()
        .await
        .map_err(|e| AppError::InternalError(format!("settings read failed: {:#}", e)))
}

/// Base query for anonymous-visible, non-deleted rows of one kind. The
/// draft exclusion only matches `kind='article'` ids, so applying it
/// unconditionally is a no-op for the other kinds and keeps one shape.
async fn anon_visible_posts(db: &DatabaseConnection, kind: &str) -> AppResult<Select<Post>> {
    let ctx = ViewerCtx::anonymous(db)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    Ok(Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Kind.eq(kind))
        .filter(post::Column::Id.not_in_subquery(draft_post_ids_subquery()))
        .filter(ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        )))
}

/// (id, updated_at) of anonymous-visible rows of one kind, newest first.
async fn content_rows(
    db: &DatabaseConnection,
    kind: &str,
) -> AppResult<Vec<(Uuid, chrono::DateTime<chrono::FixedOffset>)>> {
    let rows = anon_visible_posts(db, kind)
        .await?
        .select_only()
        .column(post::Column::Id)
        .column(post::Column::UpdatedAt)
        .order_by_desc(post::Column::UpdatedAt)
        .limit(URLS_PER_FILE)
        .into_tuple::<(Uuid, chrono::DateTime<chrono::FixedOffset>)>()
        .all(db)
        .await?;
    if rows.len() as u64 == URLS_PER_FILE {
        tracing::warn!(
            "sitemap section '{}' hit the {}-URL protocol cap; older entries are omitted",
            kind,
            URLS_PER_FILE
        );
    }
    Ok(rows)
}

async fn content_sitemap(
    state: &SitemapState,
    kind: &str,
    path_prefix: &str,
) -> AppResult<impl IntoResponse> {
    let entries = if public_feed_enabled(state).await? {
        let base = site_url();
        content_rows(&state.db, kind)
            .await?
            .into_iter()
            .map(|(id, updated)| {
                (
                    format!("{}/{}/{}", base, path_prefix, id),
                    Some(lastmod_value(&updated)),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(xml_response(urlset(&entries)))
}

async fn serve_posts(State(state): State<SitemapState>) -> AppResult<impl IntoResponse> {
    content_sitemap(&state, post::KIND_POST, "post").await
}

async fn serve_albums(State(state): State<SitemapState>) -> AppResult<impl IntoResponse> {
    content_sitemap(&state, post::KIND_ALBUM, "album").await
}

async fn serve_articles(State(state): State<SitemapState>) -> AppResult<impl IntoResponse> {
    content_sitemap(&state, post::KIND_ARTICLE, "articles").await
}

/// Profile pages for users who currently author anonymous-visible
/// content, lastmod'd by their newest such row. Members without public
/// content stay unlisted: their profile page renders nothing for a
/// crawler, and an invite-only network shouldn't enumerate its members.
async fn serve_profiles(State(state): State<SitemapState>) -> AppResult<impl IntoResponse> {
    if !public_feed_enabled(&state).await? {
        return Ok(xml_response(urlset(&[])));
    }

    let ctx = ViewerCtx::anonymous(&state.db)
        .await
        .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
    let authors = Post::find()
        .filter(post::Column::DeletedAt.is_null())
        .filter(post::Column::Id.not_in_subquery(draft_post_ids_subquery()))
        .filter(ctx.feed_condition(
            post::Column::Visibility,
            post::Column::AuthorId,
            post::Column::CategoryId,
        ))
        .select_only()
        .column(post::Column::AuthorId)
        .column_as(post::Column::UpdatedAt.max(), "last_updated")
        .group_by(post::Column::AuthorId)
        .into_tuple::<(Uuid, chrono::DateTime<chrono::FixedOffset>)>()
        .all(&state.db)
        .await?;

    let handles: std::collections::HashMap<Uuid, String> = User::find()
        .filter(user::Column::Id.is_in(authors.iter().map(|(id, _)| *id).collect::<Vec<_>>()))
        .filter(user::Column::Active.eq(true))
        .select_only()
        .column(user::Column::Id)
        .column(user::Column::Handle)
        .into_tuple::<(Uuid, String)>()
        .all(&state.db)
        .await?
        .into_iter()
        .collect();

    let base = site_url();
    let entries: Vec<(String, Option<String>)> = authors
        .iter()
        .filter_map(|(id, updated)| {
            handles
                .get(id)
                .map(|h| (format!("{}/u/{}", base, h), Some(lastmod_value(updated))))
        })
        .collect();
    Ok(xml_response(urlset(&entries)))
}

/// Landing page plus the public listing pages the SPA serves at fixed
/// routes. With the public feed off the listings render nothing for
/// anonymous visitors, so only `/` stays listed.
async fn serve_static(State(state): State<SitemapState>) -> AppResult<impl IntoResponse> {
    let base = site_url();
    let mut entries = vec![(format!("{}/", base), None)];
    if public_feed_enabled(&state).await? {
        for path in ["/articles", "/albums", "/categories"] {
            entries.push((format!("{}{}", base, path), None));
        }
    }
    Ok(xml_response(urlset(&entries)))
}

async fn serve_index(State(state): State<SitemapState>) -> AppResult<impl IntoResponse> {
    let base = site_url();
    let mut sections: Vec<(String, Option<String>)> =
        vec![(format!("{}/sitemaps/static.xml", base), None)];

    if public_feed_enabled(&state).await? {
        // One grouped scan yields each kind's newest visible row for the
        // index's lastmod hints. Profiles change exactly when content
        // does, so the newest timestamp overall covers that section.
        let ctx = ViewerCtx::anonymous(&state.db)
            .await
            .map_err(|e| AppError::InternalError(format!("viewer ctx: {:#}", e)))?;
        let kind_maxes: std::collections::HashMap<String, chrono::DateTime<chrono::FixedOffset>> =
            Post::find()
                .filter(post::Column::DeletedAt.is_null())
                .filter(post::Column::Id.not_in_subquery(draft_post_ids_subquery()))
                .filter(ctx.feed_condition(
                    post::Column::Visibility,
                    post::Column::AuthorId,
                    post::Column::CategoryId,
                ))
                .select_only()
                .column(post::Column::Kind)
                .column_as(post::Column::UpdatedAt.max(), "last_updated")
                .group_by(post::Column::Kind)
                .into_tuple::<(String, chrono::DateTime<chrono::FixedOffset>)>()
                .all(&state.db)
                .await?
                .into_iter()
                .collect();
        let newest_overall = kind_maxes.values().max().copied();

        for (kind, file) in [
            (post::KIND_POST, "posts.xml"),
            (post::KIND_ALBUM, "albums.xml"),
            (post::KIND_ARTICLE, "articles.xml"),
        ] {
            sections.push((
                format!("{}/sitemaps/{}", base, file),
                kind_maxes.get(kind).map(lastmod_value),
            ));
        }
        sections.push((
            format!("{}/sitemaps/profiles.xml", base),
            newest_overall.as_ref().map(lastmod_value),
        ));
    }

    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for (loc, lastmod) in &sections {
        out.push_str("  <sitemap><loc>");
        out.push_str(&xml_escape(loc));
        out.push_str("</loc>");
        if let Some(lm) = lastmod {
            out.push_str("<lastmod>");
            out.push_str(lm);
            out.push_str("</lastmod>");
        }
        out.push_str("</sitemap>\n");
    }
    out.push_str("</sitemapindex>\n");
    Ok(xml_response(out))
}
