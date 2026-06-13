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
use crate::entities::{subscriber, Subscriber};
use crate::errors::{AppError, AppResult};
use crate::middleware::rate_limit::AccessLogEvent;
use crate::subscriptions::types::{SubscribeRequest, SubscribeResponse, VerifyQuery};
use crate::subscriptions::{is_valid_email, SubscribeState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Extension, Json, Router,
};
use basic_axum_rate_limit::SecurityContext;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

pub fn subscribe_routes() -> Router<SubscribeState> {
    Router::new()
        .route("/api/subscribe", post(subscribe))
        .route("/api/subscribe/verify", get(verify_subscription))
}

async fn subscribe(
    State(state): State<SubscribeState>,
    Extension(security_context): Extension<SecurityContext>,
    Json(payload): Json<SubscribeRequest>,
) -> AppResult<impl IntoResponse> {
    if !state
        .settings
        .get_subscriptions_enabled()
        .await
        .unwrap_or(true)
    {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(SubscribeResponse {
                success: false,
                message: "Not found".to_string(),
            }),
        ));
    }

    let email = payload.email.trim().to_lowercase();

    if !is_valid_email(&email) {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(SubscribeResponse {
                success: false,
                message: "Invalid email address.".to_string(),
            }),
        ));
    }

    let subscribe_key = format!("subscribe:{}", security_context.ip_address);

    let ip_addr = security_context
        .ip_address
        .parse::<std::net::IpAddr>()
        .map_err(|e| {
            tracing::error!("Failed to parse IP address: {}", e);
            AppError::InternalError("Invalid IP address".to_string())
        })?;

    let has_recent_subscription = state
        .callbacks
        .has_recent_subscription(ip_addr)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check subscription rate limit: {}", e);
            e
        })
        .unwrap_or(false);

    if has_recent_subscription {
        tracing::warn!(
            "Subscription rate limit exceeded for IP: {}",
            security_context.ip_address
        );
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            Json(SubscribeResponse {
                success: false,
                message: "You can only subscribe once every 24 hours.".to_string(),
            }),
        ));
    }

    let existing_subscriber = Subscriber::find()
        .filter(subscriber::Column::Email.eq(&email))
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error checking subscriber: {}", e);
            e
        })
        .unwrap_or(None);

    if let Some(existing) = existing_subscriber {
        if existing.verified {
            return Ok((
                StatusCode::OK,
                Json(SubscribeResponse {
                    success: true,
                    message: "You're already subscribed!".to_string(),
                }),
            ));
        } else {
            if let Some(token) = &existing.verification_token {
                let _ = state
                    .email_service
                    .send_subscription_confirmation(&email, token, None)
                    .await;
            }
            return Ok((
                StatusCode::OK,
                Json(SubscribeResponse {
                    success: true,
                    message: "Verification email resent. Please check your inbox.".to_string(),
                }),
            ));
        }
    }

    let verification_token = Uuid::new_v4().to_string();
    let now = Utc::now();

    let new_subscriber = subscriber::ActiveModel {
        id: Set(Uuid::new_v4()),
        email: Set(email.clone()),
        verified: Set(false),
        verification_token: Set(Some(verification_token.clone())),
        verified_at: Set(None),
        active: Set(true),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    };

    match new_subscriber.insert(&state.db).await {
        Ok(_) => {
            match state
                .email_service
                .send_subscription_confirmation(&email, &verification_token, None)
                .await
            {
                Ok(_) => {
                    let _ = state
                        .callbacks
                        .log_access_attempt(AccessLogEvent {
                            ip: Some(ip_addr),
                            user_agent: Some(security_context.user_agent.clone()),
                            access_code: subscribe_key.clone(),
                            action: "subscribe_submit".to_string(),
                            success: true,
                            tokens: 0.0,
                            admin_user_id: None,
                            admin_user_email: None,
                        })
                        .await;

                    tracing::info!("New subscription created for {}", email);
                    Ok((
                        StatusCode::OK,
                        Json(SubscribeResponse {
                            success: true,
                            message: "Subscription successful! Please check your email to confirm."
                                .to_string(),
                        }),
                    ))
                }
                Err(e) => {
                    tracing::error!("Failed to send subscription confirmation: {}", e);

                    let _ = state
                        .callbacks
                        .log_access_attempt(AccessLogEvent {
                            ip: Some(ip_addr),
                            user_agent: Some(security_context.user_agent.clone()),
                            access_code: subscribe_key,
                            action: "subscribe_submit".to_string(),
                            success: false,
                            tokens: 0.0,
                            admin_user_id: None,
                            admin_user_email: None,
                        })
                        .await;

                    Ok((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(SubscribeResponse {
                            success: false,
                            message: "Failed to send confirmation email. Please try again later."
                                .to_string(),
                        }),
                    ))
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to create subscriber: {}", e);
            Ok((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SubscribeResponse {
                    success: false,
                    message: "Failed to process subscription. Please try again later.".to_string(),
                }),
            ))
        }
    }
}

async fn verify_subscription(
    State(state): State<SubscribeState>,
    Query(query): Query<VerifyQuery>,
) -> Result<Redirect, Redirect> {
    if !state
        .settings
        .get_subscriptions_enabled()
        .await
        .unwrap_or(true)
    {
        return Err(Redirect::to("/?verified=invalid"));
    }

    let subscriber = Subscriber::find()
        .filter(subscriber::Column::VerificationToken.eq(&query.token))
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding subscriber: {}", e);
            e
        })
        .unwrap_or(None);

    match subscriber {
        Some(sub) => {
            if sub.verified {
                tracing::info!("Subscription already verified, redirecting to blog");
                return Ok(Redirect::to("/blog?verified=already"));
            }

            let created_at = sub.created_at.with_timezone(&Utc);
            if Utc::now().signed_duration_since(created_at) > Duration::days(7) {
                tracing::warn!("Verification token expired, redirecting to blog");
                return Err(Redirect::to("/blog?verified=expired"));
            }

            let mut active_sub: subscriber::ActiveModel = sub.into();
            active_sub.verified = Set(true);
            active_sub.verified_at = Set(Some(Utc::now().into()));
            active_sub.verification_token = Set(None);
            active_sub.updated_at = Set(Utc::now().into());

            match active_sub.update(&state.db).await {
                Ok(_) => {
                    tracing::info!("Subscription verified for token: {}", query.token);
                    Ok(Redirect::to("/blog?verified=success"))
                }
                Err(e) => {
                    tracing::error!("Failed to verify subscription: {}", e);
                    Err(Redirect::to("/blog?verified=error"))
                }
            }
        }
        None => {
            tracing::warn!("Invalid verification token, redirecting to blog");
            Err(Redirect::to("/blog?verified=invalid"))
        }
    }
}
