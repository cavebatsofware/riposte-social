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
//! Generic customer-order intake (business module). A public, unauthenticated
//! endpoint persists an order and notifies the business owner by email (and
//! optionally SMS). Modeled on the contact-form module.
pub mod handlers;
pub mod phone;
pub mod types;

use crate::email::EmailService;
use crate::middleware::rate_limit::AppRateLimitCallbacks;
use crate::settings::SettingsService;
use basic_axum_rate_limit::RateLimiter;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

pub use handlers::order_routes;

#[derive(Clone)]
pub struct OrderState {
    pub email_service: Arc<EmailService>,
    pub callbacks: AppRateLimitCallbacks,
    pub settings: SettingsService,
    pub db: DatabaseConnection,
    /// Shared HTTP client for the Turnstile siteverify + Twilio Lookup calls.
    pub http: reqwest::Client,
    /// Strict limiter for the paid Twilio Lookup call (per-IP).
    pub phone_lookup_rate_limiter: RateLimiter<AppRateLimitCallbacks>,
}
