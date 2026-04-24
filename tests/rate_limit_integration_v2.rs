use axum::{
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode},
    middleware::from_fn_with_state,
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_test::TestServer;
use basic_axum_rate_limit::{
    rate_limit_middleware, security_context_middleware, NoOpOnBlocked, RateLimitConfig, RateLimiter,
};
use std::net::SocketAddr;
use std::time::Duration;

// Test handlers
async fn handler_ok() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn handler_not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not Found")
}

async fn handler_cache(req: Request<Body>) -> impl IntoResponse {
    // HTTP headers are case-insensitive, but we need to use the standard name
    if req
        .headers()
        .get(axum::http::header::IF_NONE_MATCH)
        .is_some()
    {
        (StatusCode::NOT_MODIFIED, "")
    } else {
        (StatusCode::OK, "Content")
    }
}

// Helper to create test server
fn create_test_server(config: RateLimitConfig) -> TestServer {
    let rate_limiter = RateLimiter::new(config, NoOpOnBlocked);

    // Add MockConnectInfo layer to provide a fallback SocketAddr
    let socket_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    let app = Router::new()
        .route("/", get(handler_ok))
        .route("/notfound", get(handler_not_found))
        .route("/cache", get(handler_cache))
        .layer(from_fn_with_state(rate_limiter, rate_limit_middleware))
        .layer(axum::middleware::from_fn(security_context_middleware))
        .layer(MockConnectInfo(socket_addr));

    TestServer::new(app)
}

#[tokio::test]
async fn test_basic_rate_limiting() {
    let config = RateLimitConfig::new(5, Duration::from_secs(60)).with_grace_period(0);
    let server = create_test_server(config);

    // First 5 requests should succeed
    for i in 1..=5 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.1")
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "Request {} should succeed",
            i
        );
    }

    // 6th request should be rate limited
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.0.1")
        .await;
    assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_grace_period_burst() {
    let config = RateLimitConfig::new(10, Duration::from_secs(60)).with_grace_period(2);
    let server = create_test_server(config);

    // Send 20 requests rapidly (should all succeed during grace period)
    for i in 1..=20 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.2")
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "Request {} should succeed during grace period",
            i
        );
    }

    // Wait for grace period to expire
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Now normal rate limiting applies
    for i in 1..=10 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.2")
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "Request {} after grace should succeed",
            i
        );
    }

    // 11th should be blocked
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.0.2")
        .await;
    assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_cache_refund() {
    let config = RateLimitConfig::new(10, Duration::from_secs(60))
        .with_grace_period(0)
        .with_cache_refund_ratio(0.9);
    let server = create_test_server(config);

    // Simplified test: 10 normal requests would exhaust the bucket
    // But with cache refunds, we should be able to make many more

    // First request (costs 1.0 token, returns OK)
    let response = server
        .get("/cache")
        .add_header("X-Forwarded-For", "10.0.0.3")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // With 9 tokens left, and each cache hit costing 0.1 net (1.0 - 0.9 refund):
    // We should be able to make 90 cache requests (9 / 0.1 = 90)
    // Let's test with fewer to ensure it works
    for i in 1..=50 {
        let response = server
            .get("/cache")
            .add_header("X-Forwarded-For", "10.0.0.3")
            .add_header("If-None-Match", "etag123")
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NOT_MODIFIED,
            "Cache request {} should succeed (we have cache refund)",
            i
        );
    }

    // After 50 cache requests at 0.1 each = 5 tokens, we should have 4 tokens left
    // So 4 more normal requests should work
    for i in 1..=4 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.3")
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "Normal request {} should succeed",
            i
        );
    }

    // Next request should be blocked
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.0.3")
        .await;
    assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_error_penalty() {
    let config = RateLimitConfig::new(10, Duration::from_secs(60))
        .with_grace_period(0)
        .with_error_penalty(1.0);
    let server = create_test_server(config);

    // 5 404 errors should consume 10 tokens (5 * 2.0)
    for _ in 1..=5 {
        let response = server
            .get("/notfound")
            .add_header("X-Forwarded-For", "10.0.0.4")
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    // Next request should be rate limited
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.0.4")
        .await;
    assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_different_ips_independent() {
    let config = RateLimitConfig::new(3, Duration::from_secs(60)).with_grace_period(0);
    let server = create_test_server(config);

    // Exhaust quota for first IP
    for _ in 1..=3 {
        server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.10")
            .await;
    }

    // First IP should be blocked
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.0.10")
        .await;
    assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);

    // Second IP should still work
    for i in 1..=3 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.11")
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "Second IP request {} should succeed",
            i
        );
    }
}

#[tokio::test]
async fn test_mixed_traffic_pattern() {
    let config = RateLimitConfig::new(20, Duration::from_secs(60))
        .with_grace_period(0)
        .with_cache_refund_ratio(0.9)
        .with_error_penalty(1.0);
    let server = create_test_server(config);

    // Mixed pattern:
    // 10 successful requests (10 tokens)
    for _ in 1..=10 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.20")
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
    }

    // 5 cache hits (0.5 tokens total)
    for _ in 1..=5 {
        let response = server
            .get("/cache")
            .add_header("X-Forwarded-For", "10.0.0.20")
            .add_header("If-None-Match", "etag")
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_MODIFIED);
    }

    // 5 errors (10 tokens: 5 * 2.0 with error_penalty=1.0)
    for _ in 1..=5 {
        let response = server
            .get("/notfound")
            .add_header("X-Forwarded-For", "10.0.0.20")
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    // Total: 10 + 0.5 + 10 = 20.5 tokens (should be blocked)
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.0.20")
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::TOO_MANY_REQUESTS,
        "Should be rate limited after mixed traffic"
    );
}

#[tokio::test]
async fn test_x_forwarded_for_parsing() {
    let config = RateLimitConfig::new(2, Duration::from_secs(60)).with_grace_period(0);
    let server = create_test_server(config);

    // Single IP with default proxy_depth=1
    server
        .get("/")
        .add_header("X-Forwarded-For", "192.168.1.1")
        .await;
    server
        .get("/")
        .add_header("X-Forwarded-For", "192.168.1.1")
        .await;
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "192.168.1.1")
        .await;
    assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);

    // Multi-IP X-Forwarded-For is rejected (400) when proxy_depth=1
    // because the middleware validates the header contains exactly proxy_depth IPs
    let response = server
        .get("/")
        .add_header("X-Forwarded-For", "10.0.1.1, 10.0.1.2")
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::BAD_REQUEST,
        "Should reject multi-IP header when proxy_depth=1"
    );
}
