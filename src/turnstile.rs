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
//! Cloudflare Turnstile token verification, shared by the order-intake
//! (business module) and contact-form endpoints. The contact form is not
//! feature-gated, so the verifier lives here rather than in `orders`.

/// Verify a Cloudflare Turnstile token via the siteverify endpoint. Returns
/// true only on a confirmed `success`; any network/parse error fails closed.
pub async fn verify(
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
