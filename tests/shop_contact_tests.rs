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
//! The contact form is reachable on the shop server (no CSRF, like order
//! intake) and honors the shared Turnstile gate. Business-only.
#![cfg(feature = "business")]

mod common;

use axum::http::StatusCode;
use common::build_shop_test_server;

#[sqlx::test(migrations = false)]
async fn test_shop_contact_reachable_and_succeeds_without_captcha(pool: sqlx::PgPool) {
    let (server, _settings, _db) = build_shop_test_server(pool).await;

    let response = server
        .post("/api/contact")
        .json(&serde_json::json!({
            "name": "Pat Customer",
            "email": "pat@example.com",
            "subject": "Question about a table",
            "message": "Do you deliver to Plattsburgh?",
        }))
        .await;

    // Mounted (not 404) and accepted: no Turnstile secret configured, so the
    // captcha is skipped and the spy email transport reports success.
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], serde_json::Value::Bool(true));
}

#[sqlx::test(migrations = false)]
async fn test_shop_contact_rejects_missing_token_when_secret_set(pool: sqlx::PgPool) {
    let (server, settings, _db) = build_shop_test_server(pool).await;

    settings
        .set_encrypted("secret_turnstile", "test-secret", Some("business"), None)
        .await
        .expect("seeding the Turnstile secret should succeed");

    // Secret configured but no token in the payload: the verifier short-circuits
    // to false without a network call, so the submission is rejected.
    let response = server
        .post("/api/contact")
        .json(&serde_json::json!({
            "name": "Pat Customer",
            "email": "pat@example.com",
            "subject": "Question",
            "message": "Hello",
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], serde_json::Value::Bool(false));
}
