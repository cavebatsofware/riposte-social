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
#![allow(dead_code, unused_imports)]

pub mod oidc_mock;
pub mod s3_mock;
pub mod ses_mock;

pub use riposte_social::tests::{test_db_from_pool, test_email};

use riposte_social::admin::UserAuthBackend;
use riposte_social::app::{AppState, RouterDeps, build_router};
use riposte_social::middleware::access_log_middleware;
use riposte_social::email::EmailService;
use riposte_social::entities::user;
use riposte_social::oidc::{OidcConfig, OidcService};
use riposte_social::s3::S3Service;
use riposte_social::security_callbacks::AppRateLimitCallbacks;
use riposte_social::settings::SettingsService;

use axum::extract::connect_info::MockConnectInfo;
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum_test::TestServer;
use basic_axum_rate_limit::{
    IpExtractionStrategy, RateLimitConfig, RateLimiter, SecurityContextConfig,
    security_context_middleware_with_config,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use totp_rs::{Algorithm, Secret, TOTP};
use tower_sessions::SessionManagerLayer;
use tower_sessions_sqlx_store::PostgresStore;

pub const TEST_PASSWORD: &str = "MyStr0ng!Password123";
pub const TEST_TOTP_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";

/// Optional service overrides for `build_test_server_with`. Any field left as
/// `None` falls back to the default construction path (real constructors that
/// either hit the network or are disabled — see the individual defaults).
#[derive(Default)]
pub struct TestServices {
    pub email: Option<Arc<EmailService>>,
    pub s3: Option<S3Service>,
    pub oidc: Option<OidcService>,
    pub enable_logging: Option<bool>,
    pub log_successful_attempts: Option<bool>,
}

pub async fn build_test_server(
    pool: sqlx::PgPool,
) -> (TestServer, UserAuthBackend, sea_orm::DatabaseConnection) {
    build_test_server_with(pool, TestServices::default()).await
}

pub async fn build_test_server_with(
    pool: sqlx::PgPool,
    services: TestServices,
) -> (TestServer, UserAuthBackend, sea_orm::DatabaseConnection) {
    dotenvy::dotenv().ok();

    let db = test_db_from_pool(pool.clone()).await;

    let enable_logging = services.enable_logging.unwrap_or(false);
    let log_successful = services.log_successful_attempts.unwrap_or(false);
    let callbacks = AppRateLimitCallbacks::new(db.clone(), enable_logging, log_successful);

    let config = RateLimitConfig::new(10000, Duration::from_secs(60));
    let rate_limiter = RateLimiter::new(config.clone(), callbacks.clone());
    let auth_rate_limiter = RateLimiter::new(config, callbacks.clone());

    let settings = SettingsService::new(db.clone());

    let s3 = match services.s3 {
        Some(s3) => s3,
        None => {
            let spy = s3_mock::S3Spy::new();
            s3_mock::build_test_s3_service(s3_mock::mock_s3_default(&spy))
        }
    };

    let oidc = match services.oidc {
        Some(oidc) => oidc,
        None => {
            let oidc_config = OidcConfig {
                enabled: false,
                issuer_url: String::new(),
                client_id: String::new(),
                client_secret: String::new(),
                redirect_uri: String::new(),
                scopes: vec!["openid".to_string()],
                role_claim: "realm_access.roles".to_string(),
                admin_role: "admin".to_string(),
            };
            OidcService::new(oidc_config)
                .await
                .expect("OidcService::new should succeed with enabled=false")
        }
    };

    let state = AppState {
        db: db.clone(),
        rate_limiter,
        auth_rate_limiter,
        callbacks,
        settings: settings.clone(),
        s3,
        oidc,
        enable_logging: services.enable_logging.unwrap_or(false),
        log_successful_attempts: services.log_successful_attempts.unwrap_or(false),
    };

    let admin_backend = UserAuthBackend::new(db.clone());
    let email_service = match services.email {
        Some(email) => email,
        None => {
            // Preserve legacy test behavior: build a real EmailService.
            // `SITE_URL` must be set in the test environment (e.g. via .env).
            Arc::new(
                EmailService::new(settings)
                    .await
                    .expect("EmailService::new should succeed in tests"),
            )
        }
    };

    let session_store = PostgresStore::new(pool);
    session_store
        .migrate()
        .await
        .expect("session table migration should succeed");
    let session_layer = SessionManagerLayer::new(session_store);

    let app_state_for_logging = state.clone();
    let deps = RouterDeps {
        state,
        admin_backend: admin_backend.clone(),
        email_service,
        session_layer,
    };
    let mut app = build_router(deps);

    // Add access log middleware when logging is enabled
    if enable_logging {
        app = app.layer(from_fn_with_state(
            app_state_for_logging,
            access_log_middleware,
        ));
    }

    // Add SecurityContext middleware + MockConnectInfo (needed by auth rate limiter)
    let security_config =
        SecurityContextConfig::new().with_ip_extraction(IpExtractionStrategy::SocketAddr);
    let socket_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    let app = app
        .layer(from_fn_with_state(
            security_config,
            security_context_middleware_with_config,
        ))
        .layer(MockConnectInfo(socket_addr));

    let server = TestServer::builder().save_cookies().build(app);

    (server, admin_backend, db)
}

pub async fn create_verified_admin(
    backend: &UserAuthBackend,
    email: &str,
    password: &str,
) -> user::Model {
    let (_admin, token) = backend.create_admin(email, password).await.unwrap();
    backend.verify_email(&token).await.unwrap()
}

pub async fn get_csrf_token(server: &TestServer) -> String {
    let response = server.get("/api/admin/csrf-token").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    json["token"].as_str().unwrap().to_string()
}

pub async fn login_as(server: &TestServer, email: &str, password: &str) {
    let token = get_csrf_token(server).await;
    let response = server
        .post("/api/admin/login")
        .add_header("x-csrf-token", &token)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
        }))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "Login should succeed for {} but got {}: {}",
        email,
        response.status_code(),
        response.text()
    );
}

pub fn generate_totp_code(secret: &str, email: &str) -> String {
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(secret.to_string())
            .to_bytes()
            .unwrap(),
        None,
        email.to_string(),
    )
    .unwrap();
    totp.generate_current().unwrap()
}

pub async fn login_as_with_mfa(server: &TestServer, email: &str, password: &str, totp_secret: &str) {
    login_as(server, email, password).await;
    let code = generate_totp_code(totp_secret, email);
    let csrf = get_csrf_token(server).await;
    let response = server
        .post("/api/admin/mfa/verify")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({ "code": code }))
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "MFA verify should succeed for {}: {}",
        email,
        response.text()
    );
}
