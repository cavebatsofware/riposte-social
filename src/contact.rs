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
use crate::{
    email::EmailService, errors::AppResult, security_callbacks::AccessLogEvent,
    settings::SettingsService,
};
use axum::{
    extract::State, http::StatusCode, response::IntoResponse, routing::post, Extension, Json,
    Router,
};
use basic_axum_rate_limit::SecurityContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct ContactState {
    pub email_service: Arc<EmailService>,
    pub callbacks: crate::security_callbacks::AppRateLimitCallbacks,
    pub settings: SettingsService,
}

#[derive(Deserialize)]
pub struct ContactFormRequest {
    name: String,
    email: String,
    subject: String,
    message: String,
}

#[derive(Serialize)]
pub struct ContactFormResponse {
    success: bool,
    message: String,
}

pub fn contact_routes() -> Router<ContactState> {
    Router::new().route("/api/contact", post(submit_contact_form))
}

async fn submit_contact_form(
    State(state): State<ContactState>,
    Extension(security_context): Extension<SecurityContext>,
    Json(payload): Json<ContactFormRequest>,
) -> AppResult<impl IntoResponse> {
    // Check if contact form feature is enabled
    if !state
        .settings
        .get_contact_form_enabled()
        .await
        .unwrap_or(true)
    {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(ContactFormResponse {
                success: false,
                message: "Not found".to_string(),
            }),
        ));
    }

    // Validate input lengths
    if payload.name.trim().is_empty()
        || payload.name.len() > 100
        || payload.email.trim().is_empty()
        || payload.email.len() > 254
        || payload.subject.trim().is_empty()
        || payload.subject.len() > 200
        || payload.message.trim().is_empty()
        || payload.message.len() > 5000
    {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(ContactFormResponse {
                success: false,
                message: "Invalid input. Please check your form fields.".to_string(),
            }),
        ));
    }

    // Email validation
    if !crate::subscribe::is_valid_email(&payload.email) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(ContactFormResponse {
                success: false,
                message: "Invalid email address.".to_string(),
            }),
        ));
    }

    // Check if this IP has submitted a contact form in the last 24 hours
    let contact_key = format!("contact_form:{}", security_context.ip_address);

    let ip_addr = security_context
        .ip_address
        .parse::<std::net::IpAddr>()
        .map_err(|e| {
            tracing::error!("Failed to parse IP address: {}", e);
            crate::errors::AppError::AuthError("Invalid IP address".to_string())
        })?;

    let has_recent_submission = state
        .callbacks
        .has_recent_contact_submission(ip_addr)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check contact form rate limit: {}", e);
            e
        })
        .unwrap_or(false);

    if has_recent_submission {
        tracing::warn!(
            "Contact form rate limit exceeded for IP: {}",
            security_context.ip_address
        );
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            Json(ContactFormResponse {
                success: false,
                message: "You have recently submitted this contact form.".to_string(),
            }),
        ));
    }

    // Send email
    match state
        .email_service
        .send_contact_form_email(
            payload.name.trim(),
            payload.email.trim(),
            payload.subject.trim(),
            payload.message.trim(),
        )
        .await
    {
        Ok(_) => {
            let _ = state
                .callbacks
                .log_access_attempt(AccessLogEvent {
                    ip: Some(ip_addr),
                    user_agent: Some(security_context.user_agent.clone()),
                    access_code: contact_key.clone(),
                    action: "contact_form_submit".to_string(),
                    success: true,
                    tokens: 0.0, // Not rate-limited
                    admin_user_id: None,
                    admin_user_email: None,
                })
                .await;

            tracing::info!(
                "Contact form submitted successfully from {} ({})",
                payload.email,
                security_context.ip_address
            );
            Ok((
                StatusCode::OK,
                Json(ContactFormResponse {
                    success: true,
                    message: "Thank you for your message! I'll get back to you soon.".to_string(),
                }),
            ))
        }
        Err(e) => {
            tracing::error!("Failed to send contact form email: {}", e);

            let _ = state
                .callbacks
                .log_access_attempt(AccessLogEvent {
                    ip: Some(ip_addr),
                    user_agent: Some(security_context.user_agent.clone()),
                    access_code: contact_key,
                    action: "contact_form_submit".to_string(),
                    success: false,
                    tokens: 0.0, // Not rate-limited
                    admin_user_id: None,
                    admin_user_email: None,
                })
                .await;

            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ContactFormResponse {
                    success: false,
                    message: "Failed to send message. Please try again later.".to_string(),
                }),
            ))
        }
    }
}
