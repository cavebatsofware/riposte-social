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
use serde::{Deserialize, Serialize};

/// A customer order submitted by any intake form. Field names are camelCase on
/// the wire to match JavaScript clients. `details` is an arbitrary JSON object
/// of form-specific fields and must carry no PII.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub customer_name: String,
    pub customer_phone: String,
    #[serde(default)]
    pub customer_email: Option<String>,
    #[serde(default)]
    pub estimate_total: Option<f64>,
    #[serde(default)]
    pub details: serde_json::Value,
    /// Cloudflare Turnstile token; verified server-side when a secret is set.
    #[serde(default)]
    pub turnstile_token: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub success: bool,
    pub order_id: String,
    pub message: String,
}
