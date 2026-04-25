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
use crate::{app::AppState, middleware::UserInfo, security_callbacks::AccessLogEvent};
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use basic_axum_rate_limit::SecurityContext;

/// Access logging middleware that logs requests after they complete
/// Uses the SecurityContext and response status to determine success/failure
pub async fn access_log_middleware(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Get security context from request extensions
    let security_context = match request.extensions().get::<SecurityContext>() {
        Some(ctx) => ctx.clone(),
        None => {
            tracing::error!("SecurityContext not found in request extensions. security_middleware must run before access_log_middleware.");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Extract request information for logging
    let path = request.uri().path().to_string();
    let method = request.method().to_string();

    // Get tokens from request extensions (set by rate_limit_middleware) BEFORE moving request
    let tokens = request.extensions().get::<f64>().copied().unwrap_or(0.0);

    // Continue to the next middleware/handler
    let response = next.run(request).await;

    // Extract admin user info from response extensions (set by require_admin_auth)
    let (admin_user_id, admin_user_email) = response
        .extensions()
        .get::<UserInfo>()
        .map(|info| (Some(info.id), Some(info.email.clone())))
        .unwrap_or((None, None));

    // Determine if the request was successful based on status code
    let status = response.status();
    let success = status.is_success();

    // Update prometheus metrics
    crate::metrics::HTTP_REQUESTS_TOTAL.inc();
    if status == StatusCode::TOO_MANY_REQUESTS {
        crate::metrics::HTTP_RATE_LIMITED_TOTAL.inc();
    } else if status.is_client_error() || status.is_server_error() {
        crate::metrics::HTTP_ERRORS_TOTAL.inc();
    }

    // Determine action type based on path for filtering
    let action_type = determine_action_type(&path);

    // Only log if logging is enabled and meets criteria
    if should_log(&action_type, success, &state) {
        // Use special action prefix for admin-authenticated requests
        let action = if admin_user_id.is_some() {
            format!("admin:{}", method)
        } else {
            method.clone()
        };

        let ip_addr = security_context.ip_address.parse::<std::net::IpAddr>().ok();

        if let Err(e) = state
            .callbacks
            .log_access_attempt(AccessLogEvent {
                ip: ip_addr,
                user_agent: Some(security_context.user_agent.clone()),
                access_code: format!("{}:{}", method, path),
                action,
                success,
                tokens,
                admin_user_id,
                admin_user_email,
            })
            .await
        {
            tracing::error!("Failed to log access attempt: {}", e);
        }
    }

    response
}

/// Determine the action type based on the request path for filtering purposes
fn determine_action_type(path: &str) -> String {
    if path.starts_with("/assets/")
        || path.starts_with("/admin/assets")
        || path.starts_with("/site-assets")
    {
        "asset".to_string()
    } else if path.ends_with("favicon.png") || path.ends_with("favicon.svg") {
        "favicon".to_string()
    } else if path.starts_with("/admin") {
        "admin".to_string()
    } else if path == "/health" {
        "health".to_string()
    } else {
        "request".to_string()
    }
}

/// Determine if we should log this request based on configuration and context
fn should_log(action: &str, success: bool, state: &AppState) -> bool {
    // Don't log noisy requests that aren't meaningful for tracking
    // - health checks (monitoring pings)
    // - favicon requests (browser automatic requests)
    // - asset requests (CSS, JS, images - these are just page dependencies)
    if matches!(action, "health" | "favicon" | "asset") {
        return false;
    }

    if !state.enable_logging {
        return false;
    }

    if success && !state.log_successful_attempts {
        return false;
    }

    // Always log failed attempts if logging is enabled
    true
}
