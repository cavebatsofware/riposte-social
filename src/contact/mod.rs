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
use std::sync::Arc;

pub use handlers::contact_routes;

#[derive(Clone)]
pub struct ContactState {
    pub email_service: Arc<EmailService>,
    pub callbacks: AppRateLimitCallbacks,
    pub settings: SettingsService,
}
