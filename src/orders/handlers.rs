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
use crate::crypto;
use crate::entities::order;
use crate::errors::{AppError, AppResult};
use crate::middleware::rate_limit::AccessLogEvent;
use crate::orders::phone;
use crate::orders::types::{OrderRequest, OrderResponse};
use crate::orders::OrderState;
use axum::{
    extract::State, http::StatusCode, response::IntoResponse, routing::post, Extension, Json,
    Router,
};
use basic_axum_rate_limit::SecurityContext;
use sea_orm::{ActiveModelTrait, Set};
use uuid::Uuid;

pub fn order_routes() -> Router<OrderState> {
    Router::new().route("/api/orders", post(submit_order))
}

fn reply(status: StatusCode, success: bool, order_id: String, message: &str) -> impl IntoResponse {
    (
        status,
        Json(OrderResponse {
            success,
            order_id,
            message: message.to_string(),
        }),
    )
}

/// Helper: trim, and treat an all-whitespace optional as absent.
fn non_empty(opt: Option<&str>) -> Option<&str> {
    opt.map(str::trim).filter(|s| !s.is_empty())
}

/// Cap on the serialized `details` object. It is freeform once it's an object,
/// so this keeps a form from pushing an oversized blob into storage.
const MAX_DETAILS_BYTES: usize = 8 * 1024;

async fn submit_order(
    State(state): State<OrderState>,
    Extension(security_context): Extension<SecurityContext>,
    Json(payload): Json<OrderRequest>,
) -> AppResult<impl IntoResponse> {
    // Live gate (on top of the compile-time `business` feature).
    if !state.settings.get_business_enabled().await.unwrap_or(false) {
        return Ok(reply(
            StatusCode::NOT_FOUND,
            false,
            String::new(),
            "Not found",
        ));
    }

    // `details` must be a JSON object (the admin UI renders it with
    // Object.entries, which breaks on a string/array/scalar) and stays under a
    // size cap since it is otherwise freeform.
    let details_json = payload.details.to_string();

    // Validate the human-readable fields: reject empties and string bombs.
    if payload.kind.trim().is_empty()
        || payload.kind.len() > 64
        || payload.title.trim().is_empty()
        || payload.title.len() > 200
        || payload.summary.trim().is_empty()
        || payload.summary.len() > 5000
        || payload.customer_name.trim().is_empty()
        || payload.customer_name.len() > 200
        || payload.customer_phone.trim().is_empty()
        || payload.customer_phone.len() > 50
        || !payload.details.is_object()
        || details_json.len() > MAX_DETAILS_BYTES
    {
        return Ok(reply(
            StatusCode::BAD_REQUEST,
            false,
            String::new(),
            "Invalid input. Please check your form fields.",
        ));
    }

    // Phone must look plausibly real. Keep this range in sync with
    // normalize_e164, which accepts 8-15 digits (8-15 assumed to already carry a
    // country code); a narrower gate would reject inputs the normalizer accepts.
    let phone_digits = payload
        .customer_phone
        .chars()
        .filter(char::is_ascii_digit)
        .count();
    if !(8..=15).contains(&phone_digits) {
        return Ok(reply(
            StatusCode::BAD_REQUEST,
            false,
            String::new(),
            "Please enter a valid phone number.",
        ));
    }

    // Email is required: the customer receives their order + BOM, deposit
    // record, and build-status updates there.
    let customer_email = payload.customer_email.trim();
    if customer_email.is_empty() || !crate::subscriptions::is_valid_email(customer_email) {
        return Ok(reply(
            StatusCode::BAD_REQUEST,
            false,
            String::new(),
            "Please enter a valid email address.",
        ));
    }

    let ip_addr = security_context.ip_address.parse::<std::net::IpAddr>().ok();

    // CAPTCHA: when a Turnstile secret is configured (encrypted setting),
    // require a valid token. No secret configured (e.g. local dev) skips it.
    // A read/decryption failure (e.g. SECURE_VALUES_KEY rotated) is a server
    // misconfiguration, not "disabled": fail closed so a broken secret can't
    // silently drop CAPTCHA enforcement.
    match state.settings.get_turnstile_secret().await {
        Ok(Some(secret)) => {
            let ok = match non_empty(payload.turnstile_token.as_deref()) {
                Some(token) => verify_turnstile(&state.http, &secret, token, ip_addr).await,
                None => false,
            };
            if !ok {
                return Ok(reply(
                    StatusCode::BAD_REQUEST,
                    false,
                    String::new(),
                    "Captcha verification failed. Please try again.",
                ));
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Failed to read Turnstile secret: {e:#}");
            return Err(AppError::InternalError(
                "captcha configuration error".to_string(),
            ));
        }
    }

    // Phone verification via Twilio Lookup. Runs only when enabled and both
    // creds are set; captcha (above) and the strict per-IP limiter gate the paid
    // call. A definitive invalid number is rejected; a provider outage fails open
    // (records nothing, allows the order, since the owner calls to confirm).
    let mut phone_verified: Option<bool> = None;
    let mut phone_line_type: Option<String> = None;
    if state
        .settings
        .get_phone_verification_enabled()
        .await
        .unwrap_or(false)
    {
        if let (Ok(Some(sid)), Ok(Some(token))) = (
            state.settings.get_twilio_account_sid().await,
            state.settings.get_twilio_auth_token().await,
        ) {
            let Some(e164) = phone::normalize_e164(payload.customer_phone.trim()) else {
                return Ok(reply(
                    StatusCode::BAD_REQUEST,
                    false,
                    String::new(),
                    "Please enter a valid phone number.",
                ));
            };
            let hmac =
                crypto::hmac_phone(&e164).map_err(|e| AppError::InternalError(e.to_string()))?;

            let check: Option<phone::PhoneCheck> =
                match phone::cached_fresh(&state.db, &hmac).await.ok().flatten() {
                    Some(row) => Some(phone::PhoneCheck {
                        valid: row.valid,
                        line_type: row.line_type,
                    }),
                    None => {
                        let key = format!("phlookup:{}", security_context.ip_address);
                        let (allowed, _, _) = state
                            .phone_lookup_rate_limiter
                            .check_rate_limit(&key, &security_context, "/api/orders")
                            .await;
                        if !allowed {
                            return Ok(reply(
                                StatusCode::TOO_MANY_REQUESTS,
                                false,
                                String::new(),
                                "Too many attempts. Please try again in a bit.",
                            ));
                        }
                        match phone::verify_phone(&state.http, &sid, &token, &e164).await {
                            Ok(c) => {
                                if let Err(e) = phone::cache_result(&state.db, &hmac, &c).await {
                                    tracing::error!("Failed to cache phone verification: {}", e);
                                }
                                Some(c)
                            }
                            Err(e) => {
                                tracing::error!("Twilio lookup failed (allowing order): {}", e);
                                None
                            }
                        }
                    }
                };

            if let Some(c) = &check {
                if !c.valid {
                    return Ok(reply(
                        StatusCode::BAD_REQUEST,
                        false,
                        String::new(),
                        "Please enter a valid phone number.",
                    ));
                }
            }
            phone_verified = check.as_ref().map(|c| c.valid);
            phone_line_type = check.and_then(|c| c.line_type);
        }
    }

    // Encrypt PII at rest (AES-256-GCM via SECURE_VALUES_KEY). A failure here
    // means the key is missing/invalid: a server misconfiguration, so 500.
    let enc =
        |v: &str| crypto::encrypt_value(v).map_err(|e| AppError::InternalError(e.to_string()));
    let name_ct = enc(payload.customer_name.trim())?;
    let phone_ct = enc(payload.customer_phone.trim())?;
    let email_ct = enc(customer_email)?;

    // Start in the first configured workflow status (falling back to "placed"
    // if the list can't be read) so a new order always lands on a status the
    // admin dropdown knows about.
    let status = state
        .settings
        .get_order_statuses()
        .await
        .ok()
        .and_then(|s| s.into_iter().next())
        .unwrap_or_else(|| "placed".to_string());

    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let model = order::ActiveModel {
        id: Set(id),
        kind: Set(payload.kind.trim().to_string()),
        title: Set(payload.title.trim().to_string()),
        summary: Set(payload.summary.trim().to_string()),
        customer_name: Set(name_ct),
        customer_phone: Set(phone_ct),
        customer_email: Set(Some(email_ct)),
        estimate_total: Set(payload.estimate_total),
        total: Set(None),
        deposit: Set(None),
        details: Set(details_json),
        status: Set(status),
        phone_verified: Set(phone_verified),
        phone_line_type: Set(phone_line_type),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    };

    if let Err(e) = model.insert(&state.db).await {
        tracing::error!("Failed to persist order: {}", e);
        log_attempt(&state, &security_context, ip_addr, id, false).await;
        return Ok(reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            false,
            String::new(),
            "Failed to submit order. Please try again later.",
        ));
    }

    // Notify the owner. Email is the primary channel; an email failure is
    // logged but the order is already saved, so we still report success.
    if let Err(e) = state
        .email_service
        .send_order_notification(
            payload.title.trim(),
            payload.summary.trim(),
            payload.customer_name.trim(),
            payload.customer_phone.trim(),
            Some(customer_email),
            payload.estimate_total,
        )
        .await
    {
        tracing::error!("Order {} saved but notification email failed: {}", id, e);
    }

    // Confirmation to the customer: their order + BOM, and the
    // promise of a call to confirm before any deposit.
    if let Err(e) = state
        .email_service
        .send_order_confirmation(
            customer_email,
            payload.customer_name.trim(),
            payload.title.trim(),
            payload.summary.trim(),
            payload.estimate_total,
        )
        .await
    {
        tracing::error!("Order {} saved but customer confirmation failed: {}", id, e);
    }

    // SMS/RCS notifications are deferred to a dedicated messaging feature; the
    // owner is notified by email above. The order_sms_* settings remain seeded
    // and reserved for that work.

    log_attempt(&state, &security_context, ip_addr, id, true).await;

    let short = id.simple().to_string();
    let order_id = format!("ORD-{}", short[..8].to_uppercase());
    tracing::info!("Order {} received ({})", order_id, payload.kind.trim());

    Ok(reply(StatusCode::OK, true, order_id, "Order received."))
}

/// Verify a Cloudflare Turnstile token via the siteverify endpoint. Returns
/// true only on a confirmed `success`; any network/parse error fails closed.
async fn verify_turnstile(
    http: &reqwest::Client,
    secret: &str,
    token: &str,
    ip: Option<std::net::IpAddr>,
) -> bool {
    let mut form = vec![
        ("secret", secret.to_string()),
        ("response", token.to_string()),
    ];
    if let Some(ip) = ip {
        form.push(("remoteip", ip.to_string()));
    }
    match http
        .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
        .form(&form)
        .send()
        .await
    {
        Ok(resp) => match resp.text().await {
            Ok(body) => serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("success").and_then(|s| s.as_bool()))
                .unwrap_or(false),
            Err(e) => {
                tracing::error!("Turnstile siteverify read failed: {}", e);
                false
            }
        },
        Err(e) => {
            tracing::error!("Turnstile siteverify request failed: {}", e);
            false
        }
    }
}

/// Best-effort audit entry; failures to log are swallowed.
async fn log_attempt(
    state: &OrderState,
    ctx: &SecurityContext,
    ip: Option<std::net::IpAddr>,
    id: Uuid,
    success: bool,
) {
    let _ = state
        .callbacks
        .log_access_attempt(AccessLogEvent {
            ip,
            user_agent: Some(ctx.user_agent.clone()),
            access_code: format!("order:{}", id),
            action: "order_submit".to_string(),
            success,
            tokens: 0.0,
            admin_user_id: None,
            admin_user_email: None,
        })
        .await;
}
