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

use riposte_social::{admin, app, crypto, database, email, errors, imports, metrics, middleware};

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

    // Seed an activated admin row with a known password for end-to-end
    // tests. Idempotent on email: existing rows are upserted to a known
    // password + activated state; missing rows are inserted. Compiled
    // only when the `e2e_testing` feature is enabled so the subcommand
    // cannot exist in the production binary at all; the runtime
    // `SEED_TEST_ADMIN=true` env guard remains for defense in depth
    // when the feature is on.
    #[cfg(feature = "e2e_testing")]
    if args.len() > 1 && args[1] == "seed-test-admin" {
        let email = args.get(2).ok_or_else(|| {
            anyhow::anyhow!("usage: cargo run -- seed-test-admin <email> <password>")
        })?;
        let password = args.get(3).ok_or_else(|| {
            anyhow::anyhow!("usage: cargo run -- seed-test-admin <email> <password>")
        })?;
        return seed_test_admin(email, password).await;
    }

    // Print an argon2 password hash for a given plaintext and exit.
    // Pure compute path: no database connection, no environment guards,
    // no encryption key required. Lets an operator hand-craft a SQL
    // insert against the dev DB without going through the seed
    // subcommand. Gated behind `e2e_testing` so the production binary
    // doesn't ship a tool that produces valid auth hashes.
    #[cfg(feature = "e2e_testing")]
    if args.len() > 1 && args[1] == "hash-password" {
        let password = args
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("usage: cargo run -- hash-password <password>"))?;
        let hash = riposte_social::admin::auth::hash_password_for_seed(password)?;
        println!("{}", hash);
        return Ok(());
    }

    // Validate encryption key is configured before accepting requests
    crypto::validate_encryption_key();

    // Register prometheus metrics
    metrics::register_metrics();

    // Create shared app state with database connection
    let state = AppState::new().await?;

    // Sweep any import_job rows left in `running` state from a prior
    // process. They could not have been reattached after restart, so we
    // mark them failed with an explanatory error. Run after AppState::new
    // so migrations are guaranteed to have applied.
    if let Err(e) = imports::sweep_stale_running_jobs(&state.db).await {
        tracing::error!("Boot-time import_job sweep failed: {}", e);
    }

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
    // Under e2e_testing + DEV_MODE=true the app runs over plain HTTP, so the
    // Secure flag must be off or cy.request() cookies won't propagate to the
    // browser for cy.visit() calls.
    #[cfg(feature = "e2e_testing")]
    let session_layer = if env::var("DEV_MODE").as_deref() == Ok("true") {
        session_layer.with_secure(false)
    } else {
        session_layer
    };

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
        // i18n catalogs (vite copies social-frontend/public/locales/* to
        // social-assets/locales/* on build). Loaded lazily by namespace
        // through i18next-http-backend at /locales/{lng}/{ns}.json. Cache
        // for an hour — short enough that a translation fix is visible
        // without forcing a hash-based busting scheme.
        .nest_service(
            "/locales",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=3600"),
                ))
                .service(ServeDir::new("./social-assets/locales").precompressed_gzip()),
        )
        // Social SPA fallback: any unmatched path serves index.html so React
        // Router handles client-side routing.
        .fallback(serve_social_spa);

    // Configure IP extraction strategy. Production builds always use
    // the reverse-proxy strategy: a release binary cannot honor a
    // DEV_MODE=true env var to fall back to socket-address extraction,
    // which would otherwise let an operator with env access neuter
    // the IP-based rate limiter and access logging by masking every
    // request behind the proxy IP.
    //
    // Under the `e2e_testing` feature the override is restored so the
    // test container (which doesn't run behind a proxy) can still use
    // socket-address extraction.
    let ip_strategy = ip_extraction_strategy();
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

/// Returns the reverse-proxy strategy in production builds. Under the
/// `e2e_testing` feature, honors `DEV_MODE=true` to fall back to
/// socket-address extraction so the test container (which doesn't run
/// behind a proxy) can still attribute requests to the calling host.
fn ip_extraction_strategy() -> IpExtractionStrategy {
    #[cfg(feature = "e2e_testing")]
    {
        if env::var("DEV_MODE").unwrap_or_default() == "true" {
            tracing::info!("DEV_MODE enabled: using socket address for IP extraction");
            return IpExtractionStrategy::SocketAddr;
        }
    }
    tracing::info!("Using X-Forwarded-For header for IP extraction");
    IpExtractionStrategy::default()
}

/// Bootstrap the first administrator. Inserts an inert admin row plus a
/// fresh invite, prints the invite URL. Refuses to run if any users
/// already exist.
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
    use riposte_social::{invites, profile};
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

    // `handle` is NOT NULL on `users` (Phase 8 profile schema). Mint a
    // unique handle from the email local-part
    let local = email.split('@').next().unwrap_or(email);
    let handle = profile::mint_unique_handle(&db, local).await?;

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
        handle: Set(handle),
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

/// Idempotently seed an activated administrator with a known password
/// for end-to-end tests. Used by `docker-compose.test.yml`'s app-test
/// service after migrations so Cypress (and humans poking the test
/// container) can sign in against a deterministic fixture.
///
/// Compiled only when the `e2e_testing` feature is enabled so the
/// function is absent from the production binary. The runtime
/// `SEED_TEST_ADMIN=true` env guard remains as defense in depth when
/// the feature is on.
///
/// Behavior:
/// - If a `users` row with this email exists, update it: rehash the
///   password, activate the row, mark email verified, clear any
///   pending invite/reset/MFA-lockout state, ensure role=administrator
///   and active=true.
/// - Otherwise insert a fresh row with handle derived from the email
///   local-part.
#[cfg(feature = "e2e_testing")]
async fn seed_test_admin(email: &str, password: &str) -> anyhow::Result<()> {
    use chrono::Utc;
    use riposte_social::admin::auth;
    use riposte_social::entities::{user, User};
    use riposte_social::profile;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use uuid::Uuid;

    if env::var("SEED_TEST_ADMIN").as_deref() != Ok("true") {
        anyhow::bail!(
            "seed-test-admin refused: SEED_TEST_ADMIN must be set to 'true' to run \
             this subcommand. This guard prevents accidentally provisioning a \
             known-credential admin in a production container."
        );
    }

    crypto::validate_encryption_key();

    let db = database::establish_connection().await?;
    let now = Utc::now().fixed_offset();
    let password_hash = auth::hash_password_for_seed(password)?;

    let existing = User::find()
        .filter(user::Column::Email.eq(email))
        .one(&db)
        .await?;

    let user_id = if let Some(model) = existing {
        let id = model.id;
        let mut active: user::ActiveModel = model.into();
        active.password_hash = Set(password_hash);
        active.email_verified = Set(true);
        active.role = Set(user::ROLE_ADMINISTRATOR.to_string());
        active.active = Set(true);
        active.activated_at = Set(Some(now));
        active.deactivated_at = Set(None);
        active.force_password_change = Set(false);
        active.password_reset_token = Set(None);
        active.password_reset_token_expires_at = Set(None);
        active.mfa_failed_attempts = Set(Some(0));
        active.mfa_locked_until = Set(None);
        // Force the row into a clean local-password-auth state. Without
        // these, an existing OIDC-linked row would still reject the
        // password path (see `authenticate_password`'s oidc_sub guard),
        // and an existing TOTP-enabled row would force `cy.login()`
        // through a second-factor it can't satisfy.
        active.oidc_sub = Set(None);
        active.totp_enabled = Set(Some(false));
        active.totp_secret = Set(None);
        active.totp_enabled_at = Set(None);
        active.update(&db).await?;
        println!("Updated existing test admin {} ({})", email, id);
        id
    } else {
        let new_id = Uuid::new_v4();
        let local = email.split('@').next().unwrap_or(email);
        let handle = profile::mint_unique_handle(&db, local).await?;
        let new_user = user::ActiveModel {
            id: Set(new_id),
            email: Set(email.to_string()),
            password_hash: Set(password_hash),
            email_verified: Set(true),
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
            display_name: Set(Some("Test Admin".to_string())),
            avatar_url: Set(None),
            last_login_at: Set(None),
            invite_code_id: Set(None),
            activated_at: Set(Some(now)),
            handle: Set(handle),
            bio: Set(None),
            pronouns: Set(None),
            avatar_s3_key: Set(None),
            locale: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        println!("Created test admin {} ({})", email, new_user.id);
        new_user.id
    };

    database::close_connection(db).await?;
    println!("Test admin user_id: {}", user_id);
    Ok(())
}
