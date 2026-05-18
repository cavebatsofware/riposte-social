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
    extract::connect_info::MockConnectInfo, http::StatusCode, middleware::from_fn_with_state,
    routing::get, Router,
};
use axum_test::TestServer;
use basic_axum_rate_limit::{
    rate_limit_middleware, security_context_middleware, AuthRefundCallback, NoOpOnBlocked,
    RateLimitConfig, RateLimiter,
};
use std::net::SocketAddr;
use std::time::Duration;

fn socket_addr() -> SocketAddr {
    "127.0.0.1:8080".parse().unwrap()
}

/// Handler that simulates `require_authenticated`: calls `AuthRefundCallback` on success.
async fn authenticated_handler(request: axum::extract::Request) -> StatusCode {
    if let Some(cb) = request.extensions().get::<AuthRefundCallback>() {
        (cb.0)();
    }
    StatusCode::OK
}

/// Handler that simulates an anonymous route: never calls the callback.
async fn anonymous_handler() -> StatusCode {
    StatusCode::OK
}

#[tokio::test]
async fn test_authenticated_burst_not_blocked() {
    // With auth_refund_ratio=0.5, each request that calls the callback costs
    // 0.5 net tokens. A 10-token bucket should sustain at least 15 requests.
    let config = RateLimitConfig::new(10, Duration::from_secs(60))
        .with_grace_period(0)
        .with_auth_refund_ratio(0.5);
    let limiter = RateLimiter::new(config, NoOpOnBlocked);

    let app = Router::new()
        .route("/test", get(authenticated_handler))
        .layer(from_fn_with_state(limiter, rate_limit_middleware))
        .layer(axum::middleware::from_fn(security_context_middleware))
        .layer(MockConnectInfo(socket_addr()));
    let server = TestServer::new(app);

    for i in 1..=15 {
        let resp = server
            .get("/test")
            .add_header("X-Forwarded-For", "10.99.1.1")
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::OK,
            "authenticated request {} should not be rate limited",
            i
        );
    }
}

#[tokio::test]
async fn test_anonymous_burst_blocked_at_limit() {
    // When the callback is never called (anonymous handler), full token cost
    // applies: 10 succeed, 11th is blocked.
    let config = RateLimitConfig::new(10, Duration::from_secs(60))
        .with_grace_period(0)
        .with_auth_refund_ratio(0.5);
    let limiter = RateLimiter::new(config, NoOpOnBlocked);

    let app = Router::new()
        .route("/test", get(anonymous_handler))
        .layer(from_fn_with_state(limiter, rate_limit_middleware))
        .layer(axum::middleware::from_fn(security_context_middleware))
        .layer(MockConnectInfo(socket_addr()));
    let server = TestServer::new(app);

    for i in 1..=10 {
        let resp = server
            .get("/test")
            .add_header("X-Forwarded-For", "10.99.2.1")
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::OK,
            "anonymous request {} should succeed",
            i
        );
    }
    let resp = server
        .get("/test")
        .add_header("X-Forwarded-For", "10.99.2.1")
        .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::TOO_MANY_REQUESTS,
        "11th anonymous request should be rate limited"
    );
}

#[tokio::test]
async fn test_auth_rate_limiter_unaffected() {
    // The auth endpoint limiter has no auth_refund_ratio, so no callback is
    // injected. Even a handler that would call it finds nothing in extensions.
    // 5-token bucket: 5 succeed, 6th is blocked.
    let auth_config = RateLimitConfig::new(5, Duration::from_secs(60)).with_grace_period(0);
    let auth_limiter = RateLimiter::new(auth_config, NoOpOnBlocked);

    let app = Router::new()
        .route("/login", get(authenticated_handler))
        .layer(from_fn_with_state(auth_limiter, rate_limit_middleware))
        .layer(axum::middleware::from_fn(security_context_middleware))
        .layer(MockConnectInfo(socket_addr()));
    let server = TestServer::new(app);

    for i in 1..=5 {
        let resp = server
            .get("/login")
            .add_header("X-Forwarded-For", "10.99.3.1")
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::OK,
            "auth-limiter request {} should succeed",
            i
        );
    }
    let resp = server
        .get("/login")
        .add_header("X-Forwarded-For", "10.99.3.1")
        .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::TOO_MANY_REQUESTS,
        "6th request should be blocked; no refund without auth_refund_ratio"
    );
}
