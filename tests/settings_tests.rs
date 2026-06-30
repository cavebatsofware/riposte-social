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
mod common;

use common::{
    build_test_server, create_verified_admin, get_csrf_token, login_as, test_email, TEST_PASSWORD,
};
use riposte_social::entities::user;
use riposte_social::settings::SettingsService;

use axum::http::StatusCode;
use sea_orm::{ActiveModelTrait, Set};

// ==================== Get all settings ====================

#[sqlx::test(migrations = false)]
async fn test_get_settings_unauthenticated_returns_401(pool: sqlx::PgPool) {
    let (server, _backend, _db) = build_test_server(pool).await;

    let response = server.get("/api/admin/settings").await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = false)]
async fn test_get_settings_viewer_returns_403(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("st-viewer");
    let admin = create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let mut active: user::ActiveModel = admin.into();
    active.role = Set("viewer".to_string());
    active.update(&db).await.unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/settings").await;

    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = response.json();
    assert_eq!(json["error"].as_str().unwrap(), "Insufficient permissions");
}

#[sqlx::test(migrations = false)]
async fn test_get_settings_returns_seeded_defaults(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("st-seeded");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/settings").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    assert!(json.is_array());
    let arr = json.as_array().unwrap();

    // Migrations seed: admin_registration_enabled, site_name, contact_email, from_email
    let keys: Vec<&str> = arr.iter().map(|s| s["key"].as_str().unwrap()).collect();
    assert!(keys.contains(&"admin_registration_enabled"));
    assert!(keys.contains(&"site_name"));
    assert!(keys.contains(&"contact_email"));
    assert!(keys.contains(&"from_email"));
}

#[sqlx::test(migrations = false)]
async fn test_get_settings_returns_inserted_settings(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("st-inserted");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Insert custom settings (use unique keys that don't conflict with seeds)
    let settings_service = SettingsService::new(db.clone());
    settings_service
        .set("custom_key_a", "value_a", Some("custom"), None)
        .await
        .unwrap();
    settings_service
        .set("custom_key_b", "value_b", Some("custom"), None)
        .await
        .unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/settings").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    let json: serde_json::Value = response.json();
    let arr = json.as_array().unwrap();

    // Find our inserted settings by key
    let custom_a = arr
        .iter()
        .find(|s| s["key"].as_str() == Some("custom_key_a"))
        .expect("custom_key_a should be present");
    assert!(custom_a["id"].is_string());
    assert_eq!(custom_a["value"].as_str().unwrap(), "value_a");
    assert_eq!(custom_a["category"].as_str().unwrap(), "custom");

    let custom_b = arr
        .iter()
        .find(|s| s["key"].as_str() == Some("custom_key_b"))
        .expect("custom_key_b should be present");
    assert_eq!(custom_b["value"].as_str().unwrap(), "value_b");
}

// ==================== Update setting ====================

#[sqlx::test(migrations = false)]
async fn test_update_setting_creates_new(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("st-create");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put("/api/admin/settings")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "key": "new_test_key",
            "value": "new_test_value",
            "category": "test_category"
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    // Verify via GET  find our new key in the list
    let get_response = server.get("/api/admin/settings").await;
    let json: serde_json::Value = get_response.json();
    let arr = json.as_array().unwrap();
    let created = arr
        .iter()
        .find(|s| s["key"].as_str() == Some("new_test_key"))
        .expect("new_test_key should be present");
    assert_eq!(created["value"].as_str().unwrap(), "new_test_value");
    assert_eq!(created["category"].as_str().unwrap(), "test_category");
}

#[sqlx::test(migrations = false)]
async fn test_update_setting_upserts_existing(pool: sqlx::PgPool) {
    let (server, backend, _db) = build_test_server(pool).await;
    let email = test_email("st-upsert");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    // Create initial setting with unique key
    let csrf = get_csrf_token(&server).await;
    let response1 = server
        .put("/api/admin/settings")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "key": "upsert_unique_key",
            "value": "first_value",
            "category": "test"
        }))
        .await;
    assert_eq!(response1.status_code(), StatusCode::OK);

    // Update with new value
    let csrf2 = get_csrf_token(&server).await;
    let response2 = server
        .put("/api/admin/settings")
        .add_header("x-csrf-token", &csrf2)
        .json(&serde_json::json!({
            "key": "upsert_unique_key",
            "value": "second_value",
            "category": "test"
        }))
        .await;
    assert_eq!(response2.status_code(), StatusCode::OK);

    // Verify only one entry exists for that key, and it has the updated value
    let get_response = server.get("/api/admin/settings").await;
    let json: serde_json::Value = get_response.json();
    let arr = json.as_array().unwrap();
    let matches: Vec<&serde_json::Value> = arr
        .iter()
        .filter(|s| s["key"].as_str() == Some("upsert_unique_key"))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "should be exactly one entry for upsert_unique_key"
    );
    assert_eq!(matches[0]["value"].as_str().unwrap(), "second_value");
}

// ==================== Encrypted settings ====================

#[sqlx::test(migrations = false)]
async fn test_set_encrypted_roundtrips_via_get_encrypted(pool: sqlx::PgPool) {
    let (_server, _backend, db) = build_test_server(pool).await;
    let service = SettingsService::new(db);

    service
        .set_encrypted("secret_smtp_password", "hunter2", Some("smtp"), None)
        .await
        .unwrap();

    let decrypted = service
        .get_encrypted("secret_smtp_password", Some("smtp"), None)
        .await
        .unwrap();
    assert_eq!(decrypted.as_deref(), Some("hunter2"));
}

#[sqlx::test(migrations = false)]
async fn test_get_on_encrypted_row_returns_error(pool: sqlx::PgPool) {
    let (_server, _backend, db) = build_test_server(pool).await;
    let service = SettingsService::new(db);

    service
        .set_encrypted("secret_oidc_client_secret", "abc123", Some("oidc"), None)
        .await
        .unwrap();

    let err = service
        .get("secret_oidc_client_secret", Some("oidc"), None)
        .await
        .expect_err("get() must refuse an encrypted row");
    assert!(
        err.to_string().contains("encrypted"),
        "error should mention encryption: {}",
        err
    );
}

#[sqlx::test(migrations = false)]
async fn test_set_on_encrypted_row_returns_error(pool: sqlx::PgPool) {
    let (_server, _backend, db) = build_test_server(pool).await;
    let service = SettingsService::new(db);

    service
        .set_encrypted("secret_s3_secret_key", "first", Some("s3"), None)
        .await
        .unwrap();

    let err = service
        .set("secret_s3_secret_key", "plain", Some("s3"), None)
        .await
        .expect_err("set() must refuse to overwrite an encrypted row");
    assert!(
        err.to_string().contains("encryption mismatch"),
        "error should describe the mismatch: {}",
        err
    );
}

#[sqlx::test(migrations = false)]
async fn test_get_encrypted_returns_none_for_empty_placeholder(pool: sqlx::PgPool) {
    let (_server, _backend, db) = build_test_server(pool).await;
    let service = SettingsService::new(db);

    // The seed migration leaves secret_sendgrid_api_key as an encrypted row
    // with an empty stored value; the getter must report "not set" rather
    // than fail decryption.
    let key = service.get_sendgrid_api_key().await.unwrap();
    assert_eq!(key, None);

    service
        .set_encrypted("secret_sendgrid_api_key", "sg-key", Some("email"), None)
        .await
        .unwrap();
    let key = service.get_sendgrid_api_key().await.unwrap();
    assert_eq!(key.as_deref(), Some("sg-key"));
}

#[sqlx::test(migrations = false)]
async fn test_get_email_provider_seeded_default_and_update(pool: sqlx::PgPool) {
    let (_server, _backend, db) = build_test_server(pool).await;
    let service = SettingsService::new(db);

    assert_eq!(service.get_email_provider().await.unwrap(), "ses");

    service
        .set("email_provider", "sendgrid", Some("email"), None)
        .await
        .unwrap();
    assert_eq!(service.get_email_provider().await.unwrap(), "sendgrid");
}

#[sqlx::test(migrations = false)]
async fn test_get_shop_link_label_default_and_update(pool: sqlx::PgPool) {
    let (_server, _backend, db) = build_test_server(pool).await;
    let service = SettingsService::new(db);

    assert_eq!(service.get_shop_link_label().await.unwrap(), "store");

    service
        .set("shop_link_label", "portfolio", Some("business"), None)
        .await
        .unwrap();
    assert_eq!(service.get_shop_link_label().await.unwrap(), "portfolio");
}

#[sqlx::test(migrations = false)]
async fn test_admin_get_settings_masks_encrypted_value(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("st-mask");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    let service = SettingsService::new(db);
    service
        .set_encrypted(
            "secret_third_party_token",
            "real-secret-value",
            Some("external"),
            None,
        )
        .await
        .unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let response = server.get("/api/admin/settings").await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let json: serde_json::Value = response.json();
    let arr = json.as_array().unwrap();
    let row = arr
        .iter()
        .find(|s| s["key"].as_str() == Some("secret_third_party_token"))
        .expect("secret_third_party_token should be present");

    assert_eq!(row["encrypted"].as_bool(), Some(true));
    assert_eq!(row["has_value"].as_bool(), Some(true));
    assert_eq!(row["value"].as_str(), Some(""));
    let serialized = serde_json::to_string(row).unwrap();
    assert!(
        !serialized.contains("real-secret-value"),
        "plaintext leaked into admin response: {}",
        serialized
    );
}

#[sqlx::test(migrations = false)]
async fn test_admin_put_secret_prefix_writes_encrypted_row(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("st-secret-put");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;
    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put("/api/admin/settings")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "key": "secret_smtp_password",
            "value": "hunter2",
            "category": "smtp"
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    let service = SettingsService::new(db);
    let decrypted = service
        .get_encrypted("secret_smtp_password", Some("smtp"), None)
        .await
        .unwrap();
    assert_eq!(decrypted.as_deref(), Some("hunter2"));

    let get_response = server.get("/api/admin/settings").await;
    let json: serde_json::Value = get_response.json();
    let arr = json.as_array().unwrap();
    let row = arr
        .iter()
        .find(|s| s["key"].as_str() == Some("secret_smtp_password"))
        .expect("secret_smtp_password should be present");
    assert_eq!(row["encrypted"].as_bool(), Some(true));
    assert_eq!(row["value"].as_str(), Some(""));
}

#[sqlx::test(migrations = false)]
async fn test_admin_put_encryption_mismatch_returns_400(pool: sqlx::PgPool) {
    let (server, backend, db) = build_test_server(pool).await;
    let email = test_email("st-mismatch-400");
    create_verified_admin(&backend, &email, TEST_PASSWORD).await;

    // Seed an encrypted row under a key that doesn't follow the
    // secret_* convention. This is a possible programmatic-seed
    // scenario; the API handler dispatches purely by key prefix, so
    // a PUT for this exact key routes to `set` (plaintext) and the
    // service-layer upsert rejects with EncryptionMismatch.
    let service = SettingsService::new(db);
    service
        .set_encrypted("non_prefixed_secret", "stored", Some("legacy"), None)
        .await
        .unwrap();

    login_as(&server, &email, TEST_PASSWORD).await;

    let csrf = get_csrf_token(&server).await;
    let response = server
        .put("/api/admin/settings")
        .add_header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "key": "non_prefixed_secret",
            "value": "plaintext-overwrite",
            "category": "legacy"
        }))
        .await;

    assert_eq!(
        response.status_code(),
        StatusCode::BAD_REQUEST,
        "encryption mismatch should surface as 400, not 500"
    );
}
