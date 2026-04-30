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
use axum::{
    http::{header, StatusCode},
    middleware::from_fn_with_state,
    response::IntoResponse,
    routing::get,
};
use std::{env, sync::Arc};
use time::Duration as TimeDuration;
use tower::ServiceBuilder;
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer, trace::TraceLayer};
use tower_sessions::{cookie::SameSite, ExpiredDeletion, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;

use riposte_social::{admin, app, crypto, database, email, errors, metrics, middleware};

use app::{AppState, RouterDeps};
use basic_axum_rate_limit::{
    rate_limit_middleware, security_context_middleware_with_config, IpExtractionStrategy,
    SecurityContextConfig,
};
use errors::{AppError, AppResult};
use middleware::access_log_middleware;

async fn health_check() -> &'static str {
    "OK"
}

async fn serve_robots() -> impl IntoResponse {
    let site_url = env::var("SITE_URL").expect("SITE_URL environment variable must be set");
    let robots_content = format!(
        "User-agent: *\nAllow: /\n\nSitemap: {}/sitemap-index.xml",
        site_url
    );

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain")],
        robots_content,
    )
}

async fn serve_favicon_png() -> AppResult<impl IntoResponse> {
    let content = tokio::fs::read("assets/icons/favicon.png")
        .await
        .map_err(AppError::FileSystem)?;

    let response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/png")],
        content,
    );

    Ok(response)
}

async fn serve_favicon_svg() -> AppResult<impl IntoResponse> {
    let content = tokio::fs::read("public-assets/favicon.svg")
        .await
        .map_err(AppError::FileSystem)?;
    let response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        content,
    );

    Ok(response)
}

async fn serve_admin_spa() -> AppResult<impl IntoResponse> {
    let html_content = tokio::fs::read_to_string("admin-assets/index.html")
        .await
        .map_err(AppError::FileSystem)?;

    let response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html_content,
    );

    Ok(response)
}

async fn serve_social_spa() -> AppResult<impl IntoResponse> {
    let html_content = tokio::fs::read_to_string("social-assets/index.html")
        .await
        .map_err(AppError::FileSystem)?;

    let response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html_content,
    );

    Ok(response)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Check for migration command
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "migrate" {
        match run_migrations_sync().await {
            Ok(_) => {
                tracing::info!("Database migrations completed successfully");
                return Ok(());
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Database migration failed: {}", e));
            }
        }
    }

    // Bootstrap the first administrator. Resolves the chicken-and-egg of
    // OIDC mode (no existing admin to issue invites, /api/auth/register
    // gated off) and password mode alike. Inserts an inert admin row plus
    // an invite, prints the invite URL. Refuses to run if any user rows
    // already exist; subsequent admins are created via POST /api/admin/users.
    if args.len() > 1 && args[1] == "bootstrap-admin" {
        let email = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("usage: cargo run -- bootstrap-admin <email>"))?;
        return bootstrap_admin(email).await;
    }

    // Validate encryption key is configured before accepting requests
    crypto::validate_encryption_key();

    // Register prometheus metrics
    metrics::register_metrics();

    // Create shared app state with database connection
    let state = AppState::new().await?;

    // Setup PostgreSQL-backed session store for admin authentication
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let session_pool = sqlx::PgPool::connect(&database_url)
        .await
        .map_err(|e| anyhow::anyhow!("Session store pool connection failed: {}", e))?;
    let session_store = PostgresStore::new(session_pool);
    session_store
        .migrate()
        .await
        .map_err(|e| anyhow::anyhow!("Session table migration failed: {}", e))?;

    // Spawn background task to clean up expired sessions
    let _deletion_task = tokio::task::spawn(
        session_store
            .clone()
            .continuously_delete_expired(std::time::Duration::from_secs(60)),
    );

    // Session expiry: 1 day of inactivity for better security
    // SameSite::Lax is required for OIDC - the redirect back from the IdP is a
    // cross-site top-level navigation, and Strict would drop the session cookie.
    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(TimeDuration::days(1)))
        .with_same_site(SameSite::Lax);

    // Setup admin auth backend
    let admin_backend = admin::UserAuthBackend::new(state.db.clone());

    // Setup email service
    let email_service = Arc::new(email::EmailService::new(state.settings.clone()).await?);

    // Build API routes via the shared router builder
    let deps = RouterDeps {
        state: state.clone(),
        admin_backend: admin_backend.clone(),
        email_service: email_service.clone(),
        session_layer: session_layer.clone(),
    };
    let api_routes = app::build_router(deps);

    let app = api_routes
        // Stateless special routes
        .route("/favicon.png", get(serve_favicon_png))
        .route("/favicon.svg", get(serve_favicon_svg))
        .route("/robots.txt", get(serve_robots))
        .route("/health", get(health_check))
        .route("/metrics", get(metrics::metrics_handler))
        // Admin panel
        .nest_service(
            "/admin/assets",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=86400"), // 1 day
                ))
                .service(ServeDir::new("./admin-assets/assets").precompressed_gzip()),
        )
        .route("/admin", get(serve_admin_spa))
        .route("/admin/{*path}", get(serve_admin_spa))
        // Code-gated document assets (CSS, JS, icons). Will be retired in Phase 6
        // when the document-access feature is removed.
        .nest_service(
            "/assets",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=86400"), // 1 day
                ))
                .service(ServeDir::new("./assets").precompressed_gzip()),
        )
        // Social SPA bundle assets (vite output under social-assets/app/*).
        .nest_service(
            "/app",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=31536000, immutable"),
                ))
                .service(ServeDir::new("./social-assets/app").precompressed_gzip()),
        )
        // Social SPA fallback: any unmatched path serves index.html so React
        // Router handles client-side routing.
        .fallback(serve_social_spa);

    // Configure IP extraction strategy based on environment
    // DEV_MODE=true uses socket address (direct connections without proxy)
    // Production (default) expects X-Forwarded-For from a single proxy
    let ip_strategy = if env::var("DEV_MODE").unwrap_or_default() == "true" {
        tracing::info!("DEV_MODE enabled: using socket address for IP extraction");
        IpExtractionStrategy::SocketAddr
    } else {
        tracing::info!("Production mode: using X-Forwarded-For header");
        IpExtractionStrategy::default()
    };
    let security_config = SecurityContextConfig::new().with_ip_extraction(ip_strategy);

    let app = app.layer(
        ServiceBuilder::new()
            .layer(from_fn_with_state(
                security_config,
                security_context_middleware_with_config,
            ))
            .layer(from_fn_with_state(
                state.rate_limiter.clone(),
                rate_limit_middleware,
            ))
            .layer(from_fn_with_state(state.clone(), access_log_middleware))
            .layer(TraceLayer::new_for_http()),
    );

    let cache_cleanup_limiter = state.rate_limiter.clone();
    let auth_cache_cleanup_limiter = state.auth_rate_limiter.clone();
    let cache_cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            cache_cleanup_limiter.cleanup_cache();
            auth_cache_cleanup_limiter.cleanup_cache();
        }
    });

    // Metrics refresh task - updates system-level gauges periodically
    let metrics_limiter = state.rate_limiter.clone();
    let metrics_auth_limiter = state.auth_rate_limiter.clone();
    let metrics_db = state.db.clone();
    let metrics_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            metrics::refresh_system_metrics(&metrics_limiter, &metrics_auth_limiter, &metrics_db)
                .await;
        }
    });

    let db_cleanup_callbacks = state.callbacks.clone();
    let retention_days = env::var("ACCESS_LOG_RETENTION_DAYS")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<i64>()
        .unwrap_or(1);
    let db_cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;

            // Clean up database logs
            if let Err(e) = db_cleanup_callbacks
                .cleanup_database_logs(retention_days)
                .await
            {
                tracing::error!("Failed to cleanup database logs: {}", e);
            }
        }
    });

    // Determine the bind address
    let port = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);

    let addr = format!("0.0.0.0:{}", port);

    // Production environments will likely want to set RUST_LOG=warn
    // unless they want to see very verbose logs
    tracing::info!("Server starting on {}", addr);
    tracing::info!("Access at: http://localhost:{}/access/", port);
    tracing::info!("RUST_LOG environment variable: {:?}", env::var("RUST_LOG"));

    // Start the server with connection info support
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
        tracing::info!("Shutdown signal received, stopping server...");
    })
    .await
    .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    // Clean up background tasks
    tracing::info!("Shutting down background tasks...");
    cache_cleanup_task.abort();
    metrics_task.abort();
    db_cleanup_task.abort();

    Ok(())
}

async fn run_migrations_sync() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Running database migrations...");

    let db = database::establish_connection().await?;
    tracing::info!("Database connection established for migrations");

    database::run_migrations(&db).await?;
    tracing::info!("Database migrations completed successfully");

    database::close_connection(db).await?;
    Ok(())
}

/// Bootstrap the first administrator. Inserts an inert admin row plus a
/// fresh invite, prints the invite URL. Refuses to run if any users
/// already exist so it can't be misused to mint privileged accounts on a
/// live system.
///
/// The DB ordering inside the transaction works around the user/invite
/// FK pair: insert the user first (no invite_code_id), insert the invite
/// (created_by points at the new user.id), update the user with the
/// invite's id. The admin row stays inert (`activated_at IS NULL`,
/// `oidc_sub IS NULL`) until the operator opens the printed URL and
/// either signs in via OIDC or sets a password (Flow A.1 / C.1).
async fn bootstrap_admin(email: &str) -> anyhow::Result<()> {
    use riposte_social::admin::auth::placeholder_password_hash;
    use riposte_social::entities::{user, User};
    use riposte_social::invites;
    use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set, TransactionTrait};
    use uuid::Uuid;

    crypto::validate_encryption_key();

    let db = database::establish_connection().await?;

    let count = User::find().count(&db).await?;
    if count > 0 {
        anyhow::bail!(
            "Bootstrap requires an empty users table; found {} existing user(s). \
             Use POST /api/admin/users to create additional admins.",
            count
        );
    }

    let txn = db.begin().await?;
    let user_id = Uuid::new_v4();
    let placeholder = placeholder_password_hash()?;

    let new_user = user::ActiveModel {
        id: Set(user_id),
        email: Set(email.to_string()),
        password_hash: Set(placeholder),
        email_verified: Set(false),
        verification_token: Set(None),
        verification_token_expires_at: Set(None),
        totp_secret: Set(None),
        totp_enabled: Set(Some(false)),
        totp_enabled_at: Set(None),
        mfa_failed_attempts: Set(Some(0)),
        mfa_locked_until: Set(None),
        active: Set(true),
        deactivated_at: Set(None),
        force_password_change: Set(false),
        password_reset_token: Set(None),
        password_reset_token_expires_at: Set(None),
        role: Set(user::ROLE_ADMINISTRATOR.to_string()),
        oidc_sub: Set(None),
        display_name: Set(None),
        avatar_url: Set(None),
        last_login_at: Set(None),
        invite_code_id: Set(None),
        activated_at: Set(None),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    let (invite, plaintext) =
        invites::issue_invite_for_user(&txn, new_user.id, Some(email.to_string())).await?;

    let mut active: user::ActiveModel = new_user.into();
    active.invite_code_id = Set(Some(invite.id));
    let _ = active.update(&txn).await?;

    txn.commit().await?;

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let site_url = env::var("SITE_URL").unwrap_or_else(|_| format!("http://localhost:{}", port));
    let invite_url = format!("{}/invite/{}", site_url.trim_end_matches('/'), plaintext);

    println!();
    println!("Bootstrap admin row created.");
    println!("  email: {}", email);
    println!("  user_id: {}", user_id);
    println!("  invite_id: {}", invite.id);
    println!();
    println!("Open the invite URL on the recipient's device to activate the account:");
    println!("  {}", invite_url);
    println!();
    if site_url.starts_with("http://localhost") || site_url.starts_with("http://127") {
        println!("(SITE_URL points at localhost. The link is only useful from this machine.)");
    } else {
        println!("(The plaintext invite code is shown only once. Copy it now.)");
    }

    database::close_connection(db).await?;
    Ok(())
}
