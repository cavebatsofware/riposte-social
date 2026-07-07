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
use crate::admin::{self, UserAuthBackend};
use crate::albums;
use crate::articles;
use crate::auth;
use crate::auth::oidc::{OidcConfig, OidcService};
use crate::categories;
use crate::email::EmailService;
use crate::engagement;
use crate::entities::{access_code, AccessCode};
use crate::errors::{AppError, AppResult};
use crate::follows;
use crate::imports;
use crate::invites;
use crate::middleware::rate_limit::AppRateLimitCallbacks;
use crate::middleware::{
    csrf_middleware, require_admin, require_admin_or_poster, require_authenticated,
};
use crate::og;
use crate::posts;
use crate::profile;
use crate::s3::S3Service;
use crate::settings::SettingsService;
use crate::sitemap;
use crate::{contact, subscriptions};
use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::{
    middleware::{from_fn, from_fn_with_state},
    routing::get,
    Router,
};
use axum_login::AuthManagerLayerBuilder;
use basic_axum_rate_limit::{
    rate_limit_middleware, RateLimitConfig, RateLimiter, RequestScreener, ScreeningConfig,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::env;
use std::sync::Arc;
use tower_sessions::SessionManagerLayer;
use tower_sessions_sqlx_store::PostgresStore;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub rate_limiter: RateLimiter<AppRateLimitCallbacks>,
    pub auth_rate_limiter: RateLimiter<AppRateLimitCallbacks>,
    /// Strict, dedicated limiter for the paid phone-lookup action (business).
    pub phone_lookup_rate_limiter: RateLimiter<AppRateLimitCallbacks>,
    pub callbacks: AppRateLimitCallbacks,
    pub settings: SettingsService,
    pub s3: S3Service,
    pub oidc: OidcService,
    pub enable_logging: bool,
    pub log_successful_attempts: bool,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        let db = crate::database::establish_connection()
            .await
            .map_err(|e| anyhow::anyhow!("Database connection failed: {}", e))?;

        let rate_limit_per_minute = env::var("RATE_LIMIT_PER_MINUTE")
            .unwrap_or_else(|_| "120".to_string())
            .parse()
            .unwrap_or(120);

        let block_duration_minutes = env::var("BLOCK_DURATION_MINUTES")
            .unwrap_or_else(|_| "15".to_string())
            .parse()
            .unwrap_or(15);

        let rate_limit_grace_period_seconds = env::var("RATE_LIMIT_GRACE_PERIOD_SECONDS")
            .unwrap_or_else(|_| "1".to_string())
            .parse()
            .unwrap_or(1);

        let rate_limit_cache_refund_ratio = env::var("RATE_LIMIT_CACHE_REFUND_RATIO")
            .unwrap_or_else(|_| "0.75".to_string())
            .parse()
            .unwrap_or(0.75);

        let rate_limit_auth_refund_ratio = env::var("RATE_LIMIT_AUTH_REFUND_RATIO")
            .unwrap_or_else(|_| "0.5".to_string())
            .parse()
            .unwrap_or(0.5);

        let rate_limit_error_penalty = env::var("RATE_LIMIT_ERROR_PENALTY")
            .unwrap_or_else(|_| "2.0".to_string())
            .parse()
            .unwrap_or(2.0);

        let enable_logging = env::var("ENABLE_ACCESS_LOGGING")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let log_successful_attempts = env::var("LOG_SUCCESSFUL_ATTEMPTS")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);

        let config = RateLimitConfig::new(
            rate_limit_per_minute,
            std::time::Duration::from_secs(block_duration_minutes * 60),
        )
        .with_grace_period(rate_limit_grace_period_seconds)
        .with_cache_refund_ratio(rate_limit_cache_refund_ratio)
        .with_auth_refund_ratio(rate_limit_auth_refund_ratio)
        .with_error_penalty(rate_limit_error_penalty);

        let callbacks =
            AppRateLimitCallbacks::new(db.clone(), enable_logging, log_successful_attempts);

        let screening_config = ScreeningConfig::new()
            .with_path_patterns(vec![
                // PHP attacks
                r"\.php\d?$".to_string(),
                r"/vendor/".to_string(),
                r"/phpunit/".to_string(),
                r"eval-stdin".to_string(),
                // .NET attacks
                r"\.aspx?$".to_string(),
                r"\.axd$".to_string(),
                r"/Telerik\.".to_string(),
                r"\.ini$".to_string(),
                // Java attacks
                r"\.jsp$".to_string(),
                r"hjasperserver".to_string(),
                r"\.jar$".to_string(),
                // Git/config exposure
                r"/\.git/".to_string(),
                r"/\.env".to_string(),
                r"/\.aws/".to_string(),
                r"/\.ssh/".to_string(),
                // Windows/RDP
                r"/RDWeb/".to_string(),
                // Router/device admin panels
                r"/webfig/".to_string(),
                r"/ssi\.cgi".to_string(),
                r"\.cc$".to_string(),
                // Monitoring tools
                r"/zabbix/".to_string(),
                // WordPress
                r"/wp-admin".to_string(),
                r"/wp-content".to_string(),
                r"/wp-includes".to_string(),
                r"/xmlrpc\.php".to_string(),
                // Router exploits
                r"\.cgi$".to_string(),
                r"/CSCOL/".to_string(),
                r"/passwd/".to_string(),
                r"/sap/".to_string(),
                r"(\$|%24)(\{|%7B)".to_string(), // JNDI injection patterns
            ])
            .with_user_agent_patterns(vec![
                "libredtail-http".to_string(),
                "zgrab".to_string(),
                "masscan".to_string(),
                "nuclei".to_string(),
                "sqlmap".to_string(),
                "nikto".to_string(),
                "nmap".to_string(),
                "dirbuster".to_string(),
                "gobuster".to_string(),
                "wfuzz".to_string(),
                "ffuf".to_string(),
                r"\$\{\$\{:-j\}\$\{:-n\}\$\{:-d\}\$\{:-i\}".to_string(), // JNDI injection pattern
                "jndi".to_string(),
            ]);

        let screener =
            RequestScreener::new(&screening_config).expect("Failed to compile screening patterns");
        let rate_limiter = RateLimiter::new(config, callbacks.clone()).with_screener(screener);

        // Auth-specific rate limiter: 5 req/min, 30 min block
        // This is much stricter to protect against brute-force attacks
        let auth_rate_limit_per_minute = env::var("AUTH_RATE_LIMIT_PER_MINUTE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);

        let auth_block_duration_minutes = env::var("AUTH_BLOCK_DURATION_MINUTES")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30);

        let auth_rate_limit_grace_period_seconds = env::var("AUTH_RATE_LIMIT_GRACE_PERIOD_SECONDS")
            .unwrap_or_else(|_| "0".to_string())
            .parse()
            .unwrap_or(0);

        let auth_rate_limit_cache_refund_ratio = env::var("AUTH_RATE_LIMIT_CACHE_REFUND_RATIO")
            .unwrap_or_else(|_| "0.0".to_string())
            .parse()
            .unwrap_or(0.0);

        let auth_rate_limit_error_penalty = env::var("AUTH_RATE_LIMIT_ERROR_PENALTY")
            .unwrap_or_else(|_| "4.0".to_string())
            .parse()
            .unwrap_or(4.0);

        let auth_config = RateLimitConfig::new(
            auth_rate_limit_per_minute,
            std::time::Duration::from_secs(auth_block_duration_minutes * 60),
        )
        .with_grace_period(auth_rate_limit_grace_period_seconds)
        .with_cache_refund_ratio(auth_rate_limit_cache_refund_ratio)
        .with_error_penalty(auth_rate_limit_error_penalty);

        let auth_rate_limiter = RateLimiter::new(auth_config, callbacks.clone());

        // Strict, dedicated limiter for the paid phone-lookup action. Per-IP so
        // one client cannot enumerate many numbers and run up provider charges.
        let phone_lookup_rate_limit_per_minute = env::var("PHONE_LOOKUP_RATE_LIMIT_PER_MINUTE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);
        let phone_lookup_block_duration_minutes = env::var("PHONE_LOOKUP_BLOCK_DURATION_MINUTES")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .unwrap_or(60);
        let phone_lookup_config = RateLimitConfig::new(
            phone_lookup_rate_limit_per_minute,
            std::time::Duration::from_secs(phone_lookup_block_duration_minutes * 60),
        )
        .with_grace_period(0)
        .with_error_penalty(5.0);
        let phone_lookup_rate_limiter = RateLimiter::new(phone_lookup_config, callbacks.clone());

        let settings = SettingsService::new(db.clone());
        let s3 = S3Service::new().await?;

        let oidc_config = OidcConfig::from_env();
        let oidc = OidcService::new(oidc_config).await?;

        tracing::info!("Database connected and services initialized");
        tracing::info!(
            "Rate limit config: {}/min, block_duration={}min, grace={}s, cache_refund={}, auth_refund={}, error_penalty={}, logging_enabled={}, log_successful={}",
            rate_limit_per_minute,
            block_duration_minutes,
            rate_limit_grace_period_seconds,
            rate_limit_cache_refund_ratio,
            rate_limit_auth_refund_ratio,
            rate_limit_error_penalty,
            enable_logging,
            log_successful_attempts
        );
        tracing::info!(
            "Auth rate limit config: {}/min, block_duration={}min, grace={}s, refund={}, error_penalty={}",
            auth_rate_limit_per_minute,
            auth_block_duration_minutes,
            auth_rate_limit_grace_period_seconds,
            auth_rate_limit_cache_refund_ratio,
            auth_rate_limit_error_penalty
        );

        Ok(AppState {
            db,
            rate_limiter,
            auth_rate_limiter,
            phone_lookup_rate_limiter,
            callbacks,
            settings,
            s3,
            oidc,
            enable_logging,
            log_successful_attempts,
        })
    }

    /// Check if code is valid in database and increment usage count
    pub async fn is_valid_code(&self, code: &str) -> Result<bool> {
        // Check database
        let db_code = AccessCode::find()
            .filter(access_code::Column::Code.eq(code))
            .one(&self.db)
            .await?;

        if let Some(db_code) = db_code {
            // Check if expired
            if let Some(expires_at) = db_code.expires_at {
                if expires_at.with_timezone(&Utc) < Utc::now() {
                    return Ok(false); // Expired
                }
            }

            // Increment usage count and update last_used_at
            let current_count = db_code.usage_count;
            let mut active_code: access_code::ActiveModel = db_code.into();
            active_code.usage_count = Set(current_count + 1);
            active_code.last_used_at = Set(Some(Utc::now().into()));
            active_code.update(&self.db).await?;

            return Ok(true);
        }

        Ok(false)
    }

    /// Get access code by code string (without incrementing usage count)
    pub async fn get_access_code(&self, code: &str) -> Result<Option<access_code::Model>> {
        let db_code = AccessCode::find()
            .filter(access_code::Column::Code.eq(code))
            .one(&self.db)
            .await?;

        if let Some(db_code) = &db_code {
            // Check if expired
            if let Some(expires_at) = db_code.expires_at {
                if expires_at.with_timezone(&Utc) < Utc::now() {
                    return Ok(None); // Expired
                }
            }
        }

        Ok(db_code)
    }
}

/// Serve the HTML landing page for a valid access code.
async fn serve_access(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Html<String>> {
    if !state
        .settings
        .get_access_codes_enabled()
        .await
        .unwrap_or(true)
    {
        return Err(AppError::InvalidAccess);
    }

    if !state.is_valid_code(&code).await.unwrap_or(false) {
        return Err(AppError::InvalidAccess);
    }

    tracing::info!("Valid access code used: {}", code);

    let html_bytes =
        state.s3.get_file(&code, "index.html").await.map_err(|e| {
            AppError::FileSystem(std::io::Error::new(std::io::ErrorKind::NotFound, e))
        })?;

    let html_content = String::from_utf8(html_bytes).map_err(|e| {
        AppError::FileSystem(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;

    Ok(Html(html_content))
}

/// Serve the downloadable document for a valid access code.
async fn download_access(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<impl IntoResponse> {
    if !state
        .settings
        .get_access_codes_enabled()
        .await
        .unwrap_or(true)
    {
        return Err(AppError::InvalidAccess);
    }

    // Get access code details to retrieve custom filename
    let access_code = state
        .get_access_code(&code)
        .await
        .map_err(|e| AppError::FileSystem(std::io::Error::new(std::io::ErrorKind::NotFound, e)))?
        .ok_or(AppError::InvalidAccess)?;

    tracing::info!("Valid access code used for download: {}", code);

    let docx_content = state
        .s3
        .get_file(&code, "Document.docx")
        .await
        .map_err(|e| AppError::FileSystem(std::io::Error::new(std::io::ErrorKind::NotFound, e)))?;

    // Use custom filename if set, otherwise use default
    let filename = access_code
        .download_filename
        .unwrap_or_else(|| "Grant_DeFayette_Document".to_string());

    let content_disposition = format!("attachment; filename=\"{}.docx\"", filename);

    let response = (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    .to_owned(),
            ),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        docx_content,
    );

    Ok(response)
}

#[derive(Clone)]
pub struct RouterDeps {
    pub state: AppState,
    pub admin_backend: UserAuthBackend,
    pub email_service: Arc<EmailService>,
    pub session_layer: SessionManagerLayer<PostgresStore>,
}

pub fn build_router(deps: RouterDeps) -> Router {
    let RouterDeps {
        state,
        admin_backend,
        email_service,
        session_layer,
    } = deps;

    let auth_layer =
        AuthManagerLayerBuilder::new(admin_backend.clone(), session_layer.clone()).build();

    // Derive OIDC config flags
    let oidc_enabled = state.oidc.config.enabled;
    let oidc_account_url = if oidc_enabled {
        Some(format!(
            "{}/account",
            state.oidc.config.issuer_url.trim_end_matches('/')
        ))
    } else {
        None
    };

    // Admin routes
    let admin_state = admin::handlers::AdminState {
        auth_backend: admin_backend.clone(),
        email_service: email_service.clone(),
        settings: state.settings.clone(),
        oidc_enabled,
        oidc_account_url,
    };
    let admin_routes = admin::handlers::admin_api_routes(state.auth_rate_limiter.clone())
        .with_state(admin_state)
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // OIDC routes  shared across all user tiers (admin, poster, commenter).
    let oidc_state = auth::handlers::OidcState {
        oidc_service: state.oidc.clone(),
        db: state.db.clone(),
        settings: state.settings.clone(),
    };
    let oidc_routes = Router::new()
        .route("/api/auth/oidc/login", get(auth::handlers::oidc_login))
        .route(
            "/api/auth/oidc/callback",
            get(auth::handlers::oidc_callback),
        )
        .with_state(oidc_state)
        .layer(from_fn_with_state(
            state.auth_rate_limiter.clone(),
            rate_limit_middleware,
        ))
        .layer(auth_layer.clone());

    // Access code management routes
    let access_code_state = admin::access_codes::AccessCodeState {
        db: state.db.clone(),
        s3: state.s3.clone(),
    };
    let access_code_routes = admin::access_codes::access_code_routes()
        .with_state(access_code_state)
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Access log management routes
    let access_log_state = admin::access_logs::AccessLogState {
        db: state.db.clone(),
    };
    let access_log_routes = admin::access_logs::access_log_routes()
        .with_state(access_log_state)
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Order admin read routes (business module). Merged unconditionally below;
    // an empty router in non-business builds keeps the merge chain cfg-free.
    #[cfg(feature = "business")]
    let orders_admin_routes = admin::orders::orders_admin_routes()
        .with_state(admin::orders::OrdersAdminState {
            db: state.db.clone(),
            settings: state.settings.clone(),
        })
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    #[cfg(not(feature = "business"))]
    let orders_admin_routes = Router::new();

    // Admin user management routes
    let admin_user_state = admin::users::AdminUserState {
        db: state.db.clone(),
        auth_backend: admin_backend.clone(),
        email_service: email_service.clone(),
    };
    let admin_user_routes = admin::users::admin_user_routes()
        .with_state(admin_user_state)
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Moderation routes (admin-only): list comments + bulk soft-delete.
    let moderation_state = admin::moderation::ModerationState {
        db: state.db.clone(),
    };
    let moderation_routes = admin::moderation::moderation_routes()
        .with_state(moderation_state)
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Import job routes (admin-only): POST FB archive, list jobs, get job
    // status. The POST handler streams the upload to a tempfile then
    // spawns a tokio worker  see `src/imports/routes.rs` for the full
    // lifecycle and `src/imports/facebook.rs` for the worker.
    let imports_state = imports::handlers::ImportsState {
        db: state.db.clone(),
        s3: state.s3.clone(),
        settings: state.settings.clone(),
    };
    let imports_routes = imports::handlers::imports_routes()
        .with_state(imports_state)
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Invite-code admin routes (admin-on-others, gated on require_admin).
    let invite_state = invites::InviteState {
        db: state.db.clone(),
        auth_backend: admin_backend.clone(),
        oidc_enabled,
        settings: state.settings.clone(),
    };
    let admin_invite_routes = invites::admin_invite_routes()
        .with_state(invite_state.clone())
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Public invite endpoints (no auth required, used by the social SPA
    // pre-login). CSRF-protected on the POSTs since they change cookie state.
    let public_invite_routes = invites::public_invite_routes()
        .with_state(invite_state.clone())
        .layer(from_fn(csrf_middleware))
        .layer(session_layer.clone());

    // Password-mode invite acceptance (auth-tier rate-limited, session-
    // establishing). Goes alongside the other auth-tier endpoints.
    let auth_invite_routes = invites::auth_invite_routes(state.auth_rate_limiter.clone())
        .with_state(invite_state)
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Posts: write endpoints require authentication + admin/poster role; the
    // PATCH/DELETE handlers additionally check author-or-admin. Read
    // endpoints are public; visibility is filtered against the caller's tier.
    let posts_state = posts::PostsState {
        db: state.db.clone(),
        s3: state.s3.clone(),
        settings: state.settings.clone(),
    };
    let post_write_routes = posts::post_write_routes()
        .with_state(posts_state.clone())
        .layer(from_fn(require_admin_or_poster))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    let post_read_routes = posts::post_read_routes()
        .with_state(posts_state)
        .layer(auth_layer.clone());

    // Albums: media-only collections, parallel to posts but
    // never merged into the feed. Author/admin gate on the write side
    // mirrors posts; reads are public with the same visibility predicate.
    let albums_state = albums::AlbumsState {
        db: state.db.clone(),
        s3: state.s3.clone(),
        settings: state.settings.clone(),
    };
    let album_write_routes = albums::album_write_routes()
        .with_state(albums_state.clone())
        .layer(from_fn(require_admin_or_poster))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    let album_read_routes = albums::album_read_routes()
        .with_state(albums_state)
        .layer(auth_layer.clone());

    // Articles: long-form markdown content. Sit on the shared `posts`
    // table with `kind='article'` and a 1-to-1 `article_details` row.
    // Writes mirror posts (admin/poster + author/admin), reads are public
    // with the same visibility predicate. Inline image uploads and the
    // cover image flow through the article-specific
    // `POST /api/articles/{id}/media` (kind-gated on `KIND_ARTICLE`)
    // registered on `article_write_routes`.
    let articles_state = articles::ArticlesState {
        db: state.db.clone(),
        s3: state.s3.clone(),
        settings: state.settings.clone(),
    };
    let article_write_routes = articles::article_write_routes()
        .with_state(articles_state.clone())
        .layer(from_fn(require_admin_or_poster))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    // Drafts listing requires authentication (it returns the caller's own
    // drafts). The rest of the read surface is public with visibility
    // filtering applied per-row.
    let article_authed_read_routes = articles::article_authed_read_routes()
        .with_state(articles_state.clone())
        .layer(from_fn(require_authenticated))
        .layer(auth_layer.clone());
    let article_read_routes = articles::article_read_routes()
        .with_state(articles_state)
        .layer(auth_layer.clone());

    // Categories. Public list is readable by anyone (used by
    // the social-frontend's left rail); CRUD is admin-only.
    let categories_state = categories::CategoriesState {
        db: state.db.clone(),
        settings: state.settings.clone(),
    };
    let public_category_routes = categories::public_category_routes()
        .with_state(categories_state.clone())
        .layer(auth_layer.clone());
    // Category management is open to admin OR poster (subject to the
    // `poster_category_management_enabled` gate). The route layer just
    // enforces "must be logged in"; per-row permission checks happen
    // inside each handler against `can_manage_category`.
    let category_management_routes = categories::category_management_routes()
        .with_state(categories_state)
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Profile: self-service writes for the authenticated viewer; public
    // reads for profile pages and avatar serving.
    let profile_state = profile::ProfileState {
        db: state.db.clone(),
        s3: state.s3.clone(),
        settings: state.settings.clone(),
    };
    let me_profile_routes = profile::me_profile_routes()
        .with_state(profile_state.clone())
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    let public_profile_routes = profile::public_profile_routes()
        .with_state(profile_state)
        .layer(auth_layer.clone());

    // Follows: directed follower-graph edges. Writes are CSRF-gated and
    // require authentication. Reads are auth-only too because the graph
    // isn't part of the public-feed surface today.
    let follows_state = follows::FollowsState {
        db: state.db.clone(),
    };
    let follows_write_routes = follows::follows_write_routes()
        .with_state(follows_state.clone())
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    let follows_read_routes = follows::follows_read_routes()
        .with_state(follows_state)
        .layer(from_fn(require_authenticated))
        .layer(auth_layer.clone());

    // Engagement: reactions and comments. Writes require authentication
    // (any tier  commenter, poster, administrator). Reads are public; the
    // visibility-tier check against the parent post happens inline.
    let engagement_state = engagement::EngagementState {
        db: state.db.clone(),
        settings: state.settings.clone(),
    };
    let engagement_write_routes = engagement::engagement_write_routes()
        .with_state(engagement_state.clone())
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());
    let engagement_read_routes = engagement::engagement_read_routes()
        .with_state(engagement_state)
        .layer(auth_layer.clone());

    // Settings management routes
    let settings_state = admin::settings::SettingsState {
        settings: state.settings.clone(),
    };
    let settings_routes = admin::settings::settings_routes()
        .with_state(settings_state)
        .layer(from_fn(require_admin))
        .layer(from_fn(require_authenticated))
        .layer(from_fn(csrf_middleware))
        .layer(auth_layer.clone());

    // Contact routes (need session_layer for CSRF token validation)
    let contact_state = contact::ContactState {
        email_service: email_service.clone(),
        callbacks: state.callbacks.clone(),
        settings: state.settings.clone(),
        http: reqwest::Client::new(),
    };
    let contact_routes = contact::contact_routes()
        .with_state(contact_state)
        .layer(from_fn(csrf_middleware))
        .layer(session_layer.clone());

    // Subscribe routes (need session_layer for CSRF token validation)
    let subscribe_state = subscriptions::SubscribeState {
        email_service: email_service.clone(),
        callbacks: state.callbacks.clone(),
        db: state.db.clone(),
        settings: state.settings.clone(),
    };
    let subscribe_routes = subscriptions::subscribe_routes()
        .with_state(subscribe_state)
        .layer(from_fn(csrf_middleware))
        .layer(session_layer.clone());

    // Sitemaps: anonymous-only read surface, no session or CSRF. Lives
    // here (not main.rs) because generation queries the database.
    let sitemap_routes = sitemap::sitemap_routes().with_state(sitemap::SitemapState {
        db: state.db.clone(),
        settings: state.settings.clone(),
    });

    // Open Graph shells: anonymous-only, DB-backed, no session/CSRF. The
    // permalink pages serve the SPA shell with per-content share meta
    // injected only for content an anonymous visitor may see.
    let og_routes = og::og_routes().with_state(og::OgState {
        db: state.db.clone(),
        settings: state.settings.clone(),
    });

    // Public access-code serving routes (need AppState)
    let access_serving_routes = Router::new()
        .route("/access/{code}", get(serve_access))
        .route("/access/{code}/download", get(download_access))
        .route("/document/{code}", get(serve_access))
        .route("/document/{code}/download", get(download_access))
        .with_state(state.clone());

    // Merge all route groups
    Router::new()
        .merge(admin_routes)
        .merge(oidc_routes)
        .merge(access_code_routes)
        .merge(access_log_routes)
        .merge(orders_admin_routes)
        .merge(admin_user_routes)
        .merge(moderation_routes)
        .merge(imports_routes)
        .merge(admin_invite_routes)
        .merge(public_invite_routes)
        .merge(auth_invite_routes)
        .merge(post_write_routes)
        .merge(post_read_routes)
        .merge(album_write_routes)
        .merge(album_read_routes)
        .merge(article_write_routes)
        .merge(article_authed_read_routes)
        .merge(article_read_routes)
        .merge(public_category_routes)
        .merge(category_management_routes)
        .merge(me_profile_routes)
        .merge(public_profile_routes)
        .merge(follows_write_routes)
        .merge(follows_read_routes)
        .merge(engagement_write_routes)
        .merge(engagement_read_routes)
        .merge(settings_routes)
        .merge(contact_routes)
        .merge(subscribe_routes)
        .merge(sitemap_routes)
        .merge(og_routes)
        .merge(access_serving_routes)
}

/// Build the shop server's API router (business module). This is a SEPARATE
/// axum app from `build_router`'s social server: it is bound to its own port so
/// the storefront and social site can run on different hosts while sharing this
/// crate's infrastructure (DB, settings, email/SMS, rate limiter). It carries
/// the order intake endpoint; the storefront static assets and base middleware
/// are attached by the caller. Public + unauthenticated like the contact form,
/// with no CSRF (an anonymous intake has no session to forge).
#[cfg(feature = "business")]
pub fn build_shop_router(state: &AppState, email_service: Arc<EmailService>) -> Router {
    let http = reqwest::Client::new();
    let order_state = crate::orders::OrderState {
        email_service: email_service.clone(),
        callbacks: state.callbacks.clone(),
        settings: state.settings.clone(),
        db: state.db.clone(),
        http: http.clone(),
        phone_lookup_rate_limiter: state.phone_lookup_rate_limiter.clone(),
    };

    // Contact form alongside order intake: public, unauthenticated, no CSRF
    // (an anonymous intake has no session to forge). Turnstile guards it when a
    // secret is configured, same as orders.
    let contact_state = contact::ContactState {
        email_service,
        callbacks: state.callbacks.clone(),
        settings: state.settings.clone(),
        http,
    };

    crate::orders::order_routes()
        .with_state(order_state)
        .merge(contact::contact_routes().with_state(contact_state))
}
