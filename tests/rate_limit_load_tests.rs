use axum::{
    extract::connect_info::MockConnectInfo, http::StatusCode, middleware::from_fn_with_state,
    response::IntoResponse, routing::get, Router,
};
use axum_test::TestServer;
use basic_axum_rate_limit::{
    rate_limit_middleware, security_context_middleware, NoOpOnBlocked, RateLimitConfig, RateLimiter,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

// Test handlers
async fn handler_ok() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn handler_not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Not Found")
}

// Helper to create test server
fn create_test_server(config: RateLimitConfig) -> TestServer {
    let rate_limiter = RateLimiter::new(config, NoOpOnBlocked);

    let socket_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    let app = Router::new()
        .route("/", get(handler_ok))
        .route("/notfound", get(handler_not_found))
        .layer(from_fn_with_state(rate_limiter, rate_limit_middleware))
        .layer(axum::middleware::from_fn(security_context_middleware))
        .layer(MockConnectInfo(socket_addr));

    TestServer::new(app)
}

#[tokio::test]
async fn test_load_1000_unique_ips_low_rate() {
    // 1000 unique IPs, each making 10 requests (well under limit)
    // All should succeed
    let config = RateLimitConfig::new(50, Duration::from_secs(60)).with_grace_period(0);
    let server = Arc::new(create_test_server(config));

    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));

    let mut tasks = JoinSet::new();

    for ip_num in 1..=1000 {
        let server = Arc::clone(&server);
        let success_count = Arc::clone(&success_count);
        let fail_count = Arc::clone(&fail_count);

        tasks.spawn(async move {
            let ip = format!("10.0.{}.{}", ip_num / 256, ip_num % 256);

            for _ in 0..10 {
                let response = server.get("/").add_header("X-Forwarded-For", &ip).await;

                if response.status_code() == StatusCode::OK {
                    success_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    fail_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    while tasks.join_next().await.is_some() {}

    let total_success = success_count.load(Ordering::Relaxed);
    let total_fail = fail_count.load(Ordering::Relaxed);

    println!(
        "Load test (1000 IPs x 10 req): {} success, {} failed",
        total_success, total_fail
    );

    // All 10,000 requests should succeed (well under 50 req/min limit per IP)
    assert_eq!(total_success, 10_000);
    assert_eq!(total_fail, 0);
}

#[tokio::test]
async fn test_load_single_ip_exceeds_limit() {
    // Single IP making 100 requests rapidly
    // Should block after 50
    let config = RateLimitConfig::new(50, Duration::from_secs(60)).with_grace_period(0);
    let server = create_test_server(config);

    let mut success_count = 0;
    let mut fail_count = 0;

    for _ in 0..100 {
        let response = server
            .get("/")
            .add_header("X-Forwarded-For", "10.0.0.1")
            .await;

        if response.status_code() == StatusCode::OK {
            success_count += 1;
        } else if response.status_code() == StatusCode::TOO_MANY_REQUESTS {
            fail_count += 1;
        }
    }

    println!(
        "Single IP spam test: {} success, {} rate limited",
        success_count, fail_count
    );

    // Should have exactly 50 successes and 50 rate limited
    assert_eq!(success_count, 50);
    assert_eq!(fail_count, 50);
}

#[tokio::test]
async fn test_load_scanner_simulation() {
    // Simulate a scanner hitting 404s with error penalties
    // With 50 token bucket and 2.0 cost per 404, should block after 25 requests
    let config = RateLimitConfig::new(50, Duration::from_secs(60))
        .with_grace_period(0)
        .with_error_penalty(1.0);
    let server = create_test_server(config);

    let mut success_404_count = 0;
    let mut blocked_count = 0;

    for _ in 0..50 {
        let response = server
            .get("/notfound")
            .add_header("X-Forwarded-For", "10.0.0.2")
            .await;

        if response.status_code() == StatusCode::NOT_FOUND {
            success_404_count += 1;
        } else if response.status_code() == StatusCode::TOO_MANY_REQUESTS {
            blocked_count += 1;
        }
    }

    println!(
        "Scanner simulation: {} 404s delivered, {} blocked",
        success_404_count, blocked_count
    );

    // Should deliver 25 404s (50 tokens / 2.0 cost = 25), then block the rest
    assert_eq!(success_404_count, 25);
    assert_eq!(blocked_count, 25);
}

#[tokio::test]
async fn test_load_mixed_traffic() {
    // 900 legitimate IPs (10 req each = 9000 req, all should succeed)
    // 100 malicious IPs (100 req each = 10000 req, most should be blocked)
    let config = RateLimitConfig::new(50, Duration::from_secs(60)).with_grace_period(0);
    let server = Arc::new(create_test_server(config));

    let legit_success = Arc::new(AtomicUsize::new(0));
    let legit_fail = Arc::new(AtomicUsize::new(0));
    let malicious_success = Arc::new(AtomicUsize::new(0));
    let malicious_blocked = Arc::new(AtomicUsize::new(0));

    let mut tasks = JoinSet::new();

    // 900 legitimate users
    for ip_num in 1..=900 {
        let server = Arc::clone(&server);
        let success = Arc::clone(&legit_success);
        let fail = Arc::clone(&legit_fail);

        tasks.spawn(async move {
            let ip = format!("192.168.{}.{}", ip_num / 256, ip_num % 256);

            for _ in 0..10 {
                let response = server.get("/").add_header("X-Forwarded-For", &ip).await;

                if response.status_code() == StatusCode::OK {
                    success.fetch_add(1, Ordering::Relaxed);
                } else {
                    fail.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    // 100 malicious users
    for ip_num in 1..=100 {
        let server = Arc::clone(&server);
        let success = Arc::clone(&malicious_success);
        let blocked = Arc::clone(&malicious_blocked);

        tasks.spawn(async move {
            let ip = format!("203.0.{}.{}", ip_num / 256, ip_num % 256);

            for _ in 0..100 {
                let response = server.get("/").add_header("X-Forwarded-For", &ip).await;

                if response.status_code() == StatusCode::OK {
                    success.fetch_add(1, Ordering::Relaxed);
                } else if response.status_code() == StatusCode::TOO_MANY_REQUESTS {
                    blocked.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    while tasks.join_next().await.is_some() {}

    let legit_ok = legit_success.load(Ordering::Relaxed);
    let legit_blocked = legit_fail.load(Ordering::Relaxed);
    let mal_ok = malicious_success.load(Ordering::Relaxed);
    let mal_blocked = malicious_blocked.load(Ordering::Relaxed);

    println!("Mixed traffic test:");
    println!(
        "  Legitimate (900 IPs x 10 req): {} success, {} blocked",
        legit_ok, legit_blocked
    );
    println!(
        "  Malicious (100 IPs x 100 req): {} success, {} blocked",
        mal_ok, mal_blocked
    );

    // All legitimate traffic should succeed
    assert_eq!(legit_ok, 9_000);
    assert_eq!(legit_blocked, 0);

    // Malicious traffic should have 50 success per IP, 50 blocked per IP
    assert_eq!(mal_ok, 5_000); // 100 IPs * 50 requests
    assert_eq!(mal_blocked, 5_000); // 100 IPs * 50 blocked
}

#[tokio::test]
async fn test_load_concurrent_burst_with_grace() {
    // Test grace period with 100 IPs making 30 concurrent requests each
    // Grace period should allow initial burst
    let config = RateLimitConfig::new(20, Duration::from_secs(60)).with_grace_period(2);
    let server = Arc::new(create_test_server(config));

    let success_count = Arc::new(AtomicUsize::new(0));
    let fail_count = Arc::new(AtomicUsize::new(0));

    let mut tasks = JoinSet::new();

    for ip_num in 1..=100 {
        let server = Arc::clone(&server);
        let success_count = Arc::clone(&success_count);
        let fail_count = Arc::clone(&fail_count);

        tasks.spawn(async move {
            let ip = format!("172.16.{}.{}", ip_num / 256, ip_num % 256);

            // Make 30 requests very quickly (grace period should allow all)
            for _ in 0..30 {
                let response = server.get("/").add_header("X-Forwarded-For", &ip).await;

                if response.status_code() == StatusCode::OK {
                    success_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    fail_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    while tasks.join_next().await.is_some() {}

    let total_success = success_count.load(Ordering::Relaxed);
    let total_fail = fail_count.load(Ordering::Relaxed);

    println!(
        "Grace period burst (100 IPs x 30 req): {} success, {} failed",
        total_success, total_fail
    );

    // All requests during grace period should succeed
    assert_eq!(total_success, 3_000);
    assert_eq!(total_fail, 0);
}
