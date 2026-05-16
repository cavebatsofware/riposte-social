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
pub mod handlers;
pub mod types;

use crate::email::EmailService;
use crate::middleware::rate_limit::AppRateLimitCallbacks;
use crate::settings::SettingsService;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

pub use handlers::subscribe_routes;

/// Validate email format: must have exactly one @, a non-empty local part,
/// and a domain with at least one dot separating non-empty labels.
pub fn is_valid_email(email: &str) -> bool {
    if email.is_empty() || email.len() > 254 {
        return false;
    }
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    if local.is_empty() || local.len() > 64 || domain.is_empty() {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2 && labels.iter().all(|l| !l.is_empty())
}

#[derive(Clone)]
pub struct SubscribeState {
    pub email_service: Arc<EmailService>,
    pub callbacks: AppRateLimitCallbacks,
    pub db: DatabaseConnection,
    pub settings: SettingsService,
}
