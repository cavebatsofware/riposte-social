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
//! Integration tests for sitemap generation:
//! - the index references every section sitemap (and /sitemap.xml aliases it)
//! - content sections only list anonymous-visible rows (per-row tiers,
//!   category inheritance, soft-deletes, draft articles)
//! - the profiles section lists only authors of public content
//! - the `public_feed_enabled` gate empties the content sections

mod common;

use common::{build_test_server, create_verified_admin, test_email, TEST_PASSWORD};

use axum::http::StatusCode;
use riposte_social::entities::{article_details, category, post};
use riposte_social::settings::SettingsService;
use riposte_social::visibility::VISIBILITY_USER_LIST;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use uuid::Uuid;

async fn insert_row(
    db: &DatabaseConnection,
    author_id: Uuid,
    kind: &str,
    visibility: &str,
    category_id: Option<Uuid>,
    deleted: bool,
) -> post::Model {
    let now = chrono::Utc::now();
    post::ActiveModel {
        id: Set(Uuid::new_v4()),
        author_id: Set(author_id),
        body: Set("body".to_string()),
        visibility: Set(visibility.to_string()),
        published_at: Set(now.into()),
        import_source: Set(None),
        import_external_id: Set(None),
        deleted_at: Set(if deleted { Some(now.into()) } else { None }),
        category_id: Set(category_id),
        kind: Set(kind.to_string()),
        slug: Set(Some("title".to_string())),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn insert_article_details(db: &DatabaseConnection, post_id: Uuid, is_draft: bool) {
    article_details::ActiveModel {
        post_id: Set(post_id),
        subtitle: Set(None),
        cover_media_id: Set(None),
        excerpt: Set(None),
        reading_time_minutes: Set(1),
        is_draft: Set(is_draft),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

async fn insert_category(db: &DatabaseConnection, slug: &str, visibility: &str) -> category::Model {
    category::ActiveModel {
        id: Set(Uuid::new_v4()),
        name: Set(slug.to_string()),
        slug: Set(slug.to_string()),
        ordinal: Set(0),
        color: Set(None),
        visibility: Set(visibility.to_string()),
        created_by: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap()
}

async fn fetch_xml(server: &axum_test::TestServer, path: &str) -> String {
    let resp = server.get(path).await;
    assert_eq!(resp.status_code(), StatusCode::OK, "GET {}", path);
    let content_type = resp.headers().get("content-type").unwrap();
    assert_eq!(content_type, "application/xml", "GET {}", path);
    resp.text()
}

#[sqlx::test(migrations = false)]
async fn test_sitemap_index_lists_sections(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let body = fetch_xml(&server, "/sitemap-index.xml").await;
    assert!(body.contains("<sitemapindex"));
    for section in [
        "/sitemaps/static.xml",
        "/sitemaps/posts.xml",
        "/sitemaps/albums.xml",
        "/sitemaps/articles.xml",
        "/sitemaps/profiles.xml",
    ] {
        assert!(body.contains(section), "index missing {}", section);
    }

    // /sitemap.xml aliases the index so crawler probes don't hit the SPA
    // fallback.
    let alias = fetch_xml(&server, "/sitemap.xml").await;
    assert!(alias.contains("<sitemapindex"));
}

#[sqlx::test(migrations = false)]
async fn test_posts_sitemap_filters_to_anonymous_visible(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("sm-posts"), TEST_PASSWORD).await;

    let public = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PUBLIC,
        None,
        false,
    )
    .await;
    let commenters = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_COMMENTERS,
        None,
        false,
    )
    .await;
    let private = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PRIVATE,
        None,
        false,
    )
    .await;
    let deleted = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PUBLIC,
        None,
        true,
    )
    .await;
    let album = insert_row(
        &db,
        admin.id,
        post::KIND_ALBUM,
        post::VISIBILITY_PUBLIC,
        None,
        false,
    )
    .await;

    let body = fetch_xml(&server, "/sitemaps/posts.xml").await;
    assert!(body.contains(&format!("/post/{}", public.id)));
    assert!(body.contains("<lastmod>"));
    assert!(!body.contains(&commenters.id.to_string()));
    assert!(!body.contains(&private.id.to_string()));
    assert!(!body.contains(&deleted.id.to_string()));
    assert!(
        !body.contains(&album.id.to_string()),
        "albums have their own section"
    );

    let albums_body = fetch_xml(&server, "/sitemaps/albums.xml").await;
    assert!(albums_body.contains(&format!("/album/{}", album.id)));
    assert!(!albums_body.contains(&public.id.to_string()));
}

#[sqlx::test(migrations = false)]
async fn test_sitemap_honors_category_inherited_visibility(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("sm-cat"), TEST_PASSWORD).await;

    let public_cat = insert_category(&db, "sm-pub", post::VISIBILITY_PUBLIC).await;
    let posters_cat = insert_category(&db, "sm-posters", post::VISIBILITY_POSTERS).await;
    let acl_cat = insert_category(&db, "sm-acl", VISIBILITY_USER_LIST).await;

    // Row visibility says private, but the public category wins.
    let in_public = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PRIVATE,
        Some(public_cat.id),
        false,
    )
    .await;
    // Row visibility says public, but the gated categories win.
    let in_posters = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PUBLIC,
        Some(posters_cat.id),
        false,
    )
    .await;
    let in_acl = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PUBLIC,
        Some(acl_cat.id),
        false,
    )
    .await;

    let body = fetch_xml(&server, "/sitemaps/posts.xml").await;
    assert!(body.contains(&format!("/post/{}", in_public.id)));
    assert!(!body.contains(&in_posters.id.to_string()));
    assert!(!body.contains(&in_acl.id.to_string()));
}

#[sqlx::test(migrations = false)]
async fn test_articles_sitemap_excludes_drafts(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("sm-art"), TEST_PASSWORD).await;

    let published = insert_row(
        &db,
        admin.id,
        post::KIND_ARTICLE,
        post::VISIBILITY_PUBLIC,
        None,
        false,
    )
    .await;
    insert_article_details(&db, published.id, false).await;
    let draft = insert_row(
        &db,
        admin.id,
        post::KIND_ARTICLE,
        post::VISIBILITY_PUBLIC,
        None,
        false,
    )
    .await;
    insert_article_details(&db, draft.id, true).await;

    let body = fetch_xml(&server, "/sitemaps/articles.xml").await;
    assert!(body.contains(&format!("/articles/{}", published.id)));
    assert!(!body.contains(&draft.id.to_string()));
}

#[sqlx::test(migrations = false)]
async fn test_profiles_sitemap_lists_public_authors_only(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let author =
        create_verified_admin(&backend, &test_email("sm-prof-author"), TEST_PASSWORD).await;
    let lurker =
        create_verified_admin(&backend, &test_email("sm-prof-lurker"), TEST_PASSWORD).await;

    insert_row(
        &db,
        author.id,
        post::KIND_POST,
        post::VISIBILITY_PUBLIC,
        None,
        false,
    )
    .await;
    insert_row(
        &db,
        lurker.id,
        post::KIND_POST,
        post::VISIBILITY_PRIVATE,
        None,
        false,
    )
    .await;

    let body = fetch_xml(&server, "/sitemaps/profiles.xml").await;
    assert!(body.contains(&format!("/u/{}", author.handle)));
    assert!(!body.contains(&format!("/u/{}", lurker.handle)));
}

#[sqlx::test(migrations = false)]
async fn test_public_feed_gate_empties_content_sections(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let admin = create_verified_admin(&backend, &test_email("sm-gate"), TEST_PASSWORD).await;
    let public = insert_row(
        &db,
        admin.id,
        post::KIND_POST,
        post::VISIBILITY_PUBLIC,
        None,
        false,
    )
    .await;

    let settings = SettingsService::new(db.clone());
    settings
        .set("public_feed_enabled", "false", Some("features"), None)
        .await
        .unwrap();

    let body = fetch_xml(&server, "/sitemaps/posts.xml").await;
    assert!(!body.contains(&public.id.to_string()));

    let profiles = fetch_xml(&server, "/sitemaps/profiles.xml").await;
    assert!(!profiles.contains(&format!("/u/{}", admin.handle)));

    // Index shrinks to the static section; static shrinks to the landing
    // page.
    let index = fetch_xml(&server, "/sitemap-index.xml").await;
    assert!(index.contains("/sitemaps/static.xml"));
    assert!(!index.contains("/sitemaps/posts.xml"));

    let static_body = fetch_xml(&server, "/sitemaps/static.xml").await;
    assert!(!static_body.contains("/articles"));
}
