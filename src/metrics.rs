use basic_axum_rate_limit::RateLimiter;
use prometheus::{Encoder, IntCounter, IntGauge, TextEncoder};
use sea_orm::DatabaseConnection;
use std::sync::LazyLock;

use crate::security_callbacks::AppRateLimitCallbacks;

// Request counters
pub static HTTP_REQUESTS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new(
        "http_requests_total",
        "Total number of HTTP requests received",
    )
    .unwrap()
});
pub static HTTP_RATE_LIMITED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new(
        "http_rate_limited_total",
        "Total number of rate-limited (429) responses",
    )
    .unwrap()
});
pub static HTTP_ERRORS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
    IntCounter::new(
        "http_errors_total",
        "Total number of error responses (4xx and 5xx, excluding 429)",
    )
    .unwrap()
});

// Gauges
pub static RATE_LIMIT_CACHE_ENTRIES: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "rate_limit_cache_entries",
        "Number of entries in the rate limiter cache",
    )
    .unwrap()
});
pub static RATE_LIMIT_BLOCKED_IPS: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "rate_limit_blocked_ips",
        "Number of currently blocked IPs in the rate limiter",
    )
    .unwrap()
});
pub static USERS_LOGGED_IN: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "users_logged_in",
        "Number of currently logged-in admin users (active sessions)",
    )
    .unwrap()
});
pub static APP_MEMORY_RSS_BYTES: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "app_memory_rss_bytes",
        "Application resident set size (RSS) in bytes",
    )
    .unwrap()
});
pub static NETWORK_RX_BYTES: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "network_rx_bytes_total",
        "Total bytes received across all network interfaces",
    )
    .unwrap()
});
pub static NETWORK_TX_BYTES: LazyLock<IntGauge> = LazyLock::new(|| {
    IntGauge::new(
        "network_tx_bytes_total",
        "Total bytes transmitted across all network interfaces",
    )
    .unwrap()
});

/// Register all metrics with the default prometheus registry.
/// Called once at startup to ensure metrics are initialized.
pub fn register_metrics() {
    let registry = prometheus::default_registry();
    let _ = registry.register(Box::new(HTTP_REQUESTS_TOTAL.clone()));
    let _ = registry.register(Box::new(HTTP_RATE_LIMITED_TOTAL.clone()));
    let _ = registry.register(Box::new(HTTP_ERRORS_TOTAL.clone()));
    let _ = registry.register(Box::new(RATE_LIMIT_CACHE_ENTRIES.clone()));
    let _ = registry.register(Box::new(RATE_LIMIT_BLOCKED_IPS.clone()));
    let _ = registry.register(Box::new(USERS_LOGGED_IN.clone()));
    let _ = registry.register(Box::new(APP_MEMORY_RSS_BYTES.clone()));
    let _ = registry.register(Box::new(NETWORK_RX_BYTES.clone()));
    let _ = registry.register(Box::new(NETWORK_TX_BYTES.clone()));
}

/// Read RSS memory from /proc/self/status.
/// Returns RSS in bytes, or 0 if unavailable.
fn read_rss_bytes() -> i64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                // Format: "    12345 kB"
                let trimmed = rest.trim();
                if let Some(kb_str) = trimmed.strip_suffix(" kB") {
                    if let Ok(kb) = kb_str.trim().parse::<i64>() {
                        return kb * 1024;
                    }
                }
            }
        }
    }
    0
}

/// Read network I/O from /proc/self/net/dev.
/// Returns (rx_bytes, tx_bytes) summed across all non-loopback interfaces.
fn read_network_io() -> (i64, i64) {
    let mut rx_total: i64 = 0;
    let mut tx_total: i64 = 0;

    if let Ok(content) = std::fs::read_to_string("/proc/self/net/dev") {
        for line in content.lines().skip(2) {
            let line = line.trim();
            if line.starts_with("lo:") {
                continue;
            }
            if let Some((_iface, stats)) = line.split_once(':') {
                let fields: Vec<&str> = stats.split_whitespace().collect();
                if fields.len() >= 10 {
                    if let Ok(rx) = fields[0].parse::<i64>() {
                        rx_total += rx;
                    }
                    if let Ok(tx) = fields[8].parse::<i64>() {
                        tx_total += tx;
                    }
                }
            }
        }
    }

    (rx_total, tx_total)
}

/// Count active (non-expired) sessions from the PostgreSQL session table.
async fn count_active_sessions(db: &DatabaseConnection) -> i64 {
    use sea_orm::ConnectionTrait;

    let result = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            "SELECT COUNT(*) as count FROM tower_sessions.session WHERE expiry_date > NOW()",
        ))
        .await;

    match result {
        Ok(Some(row)) => row.try_get::<i64>("", "count").unwrap_or(0),
        _ => 0,
    }
}

/// Update all gauge-type metrics that need periodic refresh.
/// Called from a background task every 15 seconds.
pub async fn refresh_system_metrics(
    rate_limiter: &RateLimiter<AppRateLimitCallbacks>,
    auth_rate_limiter: &RateLimiter<AppRateLimitCallbacks>,
    db: &DatabaseConnection,
) {
    // Rate limiter cache stats
    let (cache_entries, blocked_ips) = rate_limiter.get_cache_stats();
    let (auth_entries, auth_blocked) = auth_rate_limiter.get_cache_stats();
    RATE_LIMIT_CACHE_ENTRIES.set((cache_entries + auth_entries) as i64);
    RATE_LIMIT_BLOCKED_IPS.set((blocked_ips + auth_blocked) as i64);

    // Active sessions
    USERS_LOGGED_IN.set(count_active_sessions(db).await);

    // Application memory
    APP_MEMORY_RSS_BYTES.set(read_rss_bytes());

    // Network I/O
    let (rx, tx) = read_network_io();
    NETWORK_RX_BYTES.set(rx);
    NETWORK_TX_BYTES.set(tx);
}

/// Handler for /metrics endpoint.
/// Only accessible from loopback interfaces (127.0.0.1 or ::1).
pub async fn metrics_handler(
    connect_info: axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let peer_ip = connect_info.0.ip();

    // Reject if not loopback or if request came through a reverse proxy
    if !peer_ip.is_loopback() || headers.contains_key("x-forwarded-for") {
        return (
            StatusCode::FORBIDDEN,
            "Metrics only available from localhost",
        )
            .into_response();
    }

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();

    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        tracing::error!("Failed to encode metrics: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to encode metrics",
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, encoder.format_type())],
        buffer,
    )
        .into_response()
}
