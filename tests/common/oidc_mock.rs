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
//! In-process mock Keycloak/OIDC provider for integration tests.
//!
//! Spins up a `wiremock::MockServer` that serves the three endpoints the
//! `openidconnect` client needs (`/.well-known/openid-configuration`,
//! `/jwks.json`, `/token`) plus stubs for `/authorize` and `/userinfo` so the
//! discovery document points somewhere valid.
//!
//! Tests stage a user via [`OidcMockServer::stage_token`] after extracting
//! the nonce from the `/login` redirect, then hit `/api/auth/oidc/callback`
//! as usual.

use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use chrono::{Duration, Utc};
use openidconnect::core::{
    CoreGenderClaim, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreRsaPrivateSigningKey,
};
use openidconnect::{
    Audience, EndUserEmail, IdToken, IdTokenClaims, IssuerUrl, JsonWebKeyId, Nonce, StandardClaims,
    SubjectIdentifier,
};
use riposte_social::auth::oidc::{KeycloakClaims, OidcConfig, OidcService};
use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const TEST_KEY_ID: &str = "test-key-1";
pub const TEST_CLIENT_ID: &str = "test-client";

/// A user staged for a single upcoming token exchange.
#[derive(Debug, Clone)]
pub struct OidcMockUser {
    pub email: String,
    pub email_verified: bool,
    pub subject: String,
    /// Roles placed at `role_claim_path` inside the ID token's additional
    /// claims. If empty and `roles_in_access_token` is also empty, no roles
    /// will be returned.
    pub roles: Vec<String>,
    /// Optional separate roles placed inside the access token. Used to
    /// exercise the Keycloak access-token fallback path.
    pub roles_in_access_token: Option<Vec<String>>,
}

impl OidcMockUser {
    pub fn new(email: impl Into<String>, roles: Vec<&str>) -> Self {
        let email = email.into();
        Self {
            subject: format!("mock-sub-{}", email),
            email,
            email_verified: true,
            roles: roles.into_iter().map(String::from).collect(),
            roles_in_access_token: None,
        }
    }

    pub fn with_access_token_roles(mut self, roles: Vec<&str>) -> Self {
        self.roles_in_access_token = Some(roles.into_iter().map(String::from).collect());
        self
    }

    /// Build a mock user with an explicit `sub`. Used by drift tests where
    /// the test wants to insert an active row with a known oidc_sub and have
    /// the mock IdP attest the same sub, so Flow B (find_by_oidc_sub) matches.
    pub fn with_sub(email: impl Into<String>, roles: Vec<&str>, sub: impl Into<String>) -> Self {
        let mut u = Self::new(email, roles);
        u.subject = sub.into();
        u
    }
}

#[derive(Debug, Clone)]
struct StagedToken {
    user: OidcMockUser,
    nonce: Nonce,
    role_claim_path: String,
}

/// Shared RSA keypair generated once per test process. RSA-2048 generation
/// costs ~100-200ms and we don't want to pay it per-test.
fn shared_key() -> &'static (Arc<RsaPrivateKey>, Arc<RsaPublicKey>, String) {
    static KEY: OnceLock<(Arc<RsaPrivateKey>, Arc<RsaPublicKey>, String)> = OnceLock::new();
    KEY.get_or_init(|| {
        // rsa 0.9 requires rand_core 0.6's CryptoRngCore, which is incompatible
        // with rand 0.10's RNG types. Use rsa's re-exported OsRng for compatibility.
        let mut rng = rsa::rand_core::OsRng;
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("RSA keygen");
        let public = RsaPublicKey::from(&private);
        let pem = private
            .to_pkcs1_pem(LineEnding::LF)
            .expect("pkcs1 pem")
            .to_string();
        (Arc::new(private), Arc::new(public), pem)
    })
}

pub struct OidcMockServer {
    server: MockServer,
    staged: Arc<Mutex<Option<StagedToken>>>,
}

impl OidcMockServer {
    pub async fn start() -> Self {
        // Touch the shared key early so the cost is paid before the mock
        // server's first request.
        let _ = shared_key();

        let server = MockServer::start().await;
        let staged: Arc<Mutex<Option<StagedToken>>> = Arc::new(Mutex::new(None));

        Self::mount_discovery(&server).await;
        Self::mount_jwks(&server).await;
        Self::mount_token(&server, staged.clone()).await;
        Self::mount_stubs(&server).await;

        Self { server, staged }
    }

    pub fn issuer_url(&self) -> String {
        self.server.uri()
    }

    /// Stage the user + nonce that the next call to `/token` will embed
    /// inside the signed ID token. The test is responsible for extracting
    /// `nonce` from the `/login` redirect URL before calling this.
    pub fn stage_token(&self, user: OidcMockUser, nonce: Nonce, role_claim_path: &str) {
        *self.staged.lock().unwrap() = Some(StagedToken {
            user,
            nonce,
            role_claim_path: role_claim_path.to_string(),
        });
    }

    /// Build an `OidcService` pointed at this mock server.
    pub async fn build_oidc_service(&self, client_id: &str) -> OidcService {
        self.build_oidc_service_with_role_claim(client_id, "realm_access.roles")
            .await
    }

    pub async fn build_oidc_service_with_role_claim(
        &self,
        client_id: &str,
        role_claim: &str,
    ) -> OidcService {
        let config = OidcConfig {
            enabled: true,
            issuer_url: self.issuer_url(),
            client_id: client_id.to_string(),
            client_secret: "test-secret".to_string(),
            redirect_uri: "http://localhost:3000/api/auth/oidc/callback".to_string(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            role_claim: role_claim.to_string(),
            admin_role: "admin".to_string(),
            poster_role: "poster".to_string(),
        };
        OidcService::new(config).await.expect("build OidcService")
    }

    // ----- internal: route mounts ---------------------------------------

    async fn mount_discovery(server: &MockServer) {
        let issuer = server.uri();
        let body = json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{}/authorize", issuer),
            "token_endpoint": format!("{}/token", issuer),
            "jwks_uri": format!("{}/jwks.json", issuer),
            "userinfo_endpoint": format!("{}/userinfo", issuer),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
            "scopes_supported": ["openid", "profile", "email"],
            "token_endpoint_auth_methods_supported": ["client_secret_basic", "client_secret_post"],
            "claims_supported": ["sub", "iss", "email", "email_verified"]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mount_jwks(server: &MockServer) {
        let (_, public, _) = shared_key();
        let n_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public.n().to_bytes_be());
        let e_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public.e().to_bytes_be());

        let body = json!({
            "keys": [{
                "kty": "RSA",
                "alg": "RS256",
                "use": "sig",
                "kid": TEST_KEY_ID,
                "n": n_b64,
                "e": e_b64,
            }]
        });

        Mock::given(method("GET"))
            .and(path("/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mount_token(server: &MockServer, staged: Arc<Mutex<Option<StagedToken>>>) {
        let issuer = server.uri();
        let responder = TokenResponder { issuer, staged };
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(responder)
            .mount(server)
            .await;
    }

    async fn mount_stubs(server: &MockServer) {
        // These endpoints are referenced by the discovery document but are
        // not actually hit during `exchange_code`. Mount no-op handlers so
        // any stray call returns 200 instead of a wiremock verification
        // failure during teardown.
        Mock::given(method("GET"))
            .and(path("/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/userinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(server)
            .await;
    }
}

struct TokenResponder {
    issuer: String,
    staged: Arc<Mutex<Option<StagedToken>>>,
}

impl Respond for TokenResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let staged = self.staged.lock().unwrap().take();
        let Some(staged) = staged else {
            return ResponseTemplate::new(400).set_body_json(json!({"error": "no_staged_token"}));
        };

        let id_token_str = sign_id_token(&self.issuer, &staged);
        let access_token_str = build_access_token(&staged);

        ResponseTemplate::new(200).set_body_json(json!({
            "access_token": access_token_str,
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": id_token_str,
        }))
    }
}

fn sign_id_token(issuer: &str, staged: &StagedToken) -> String {
    let (_, _, pem) = shared_key();

    let signing_key =
        CoreRsaPrivateSigningKey::from_pem(pem, Some(JsonWebKeyId::new(TEST_KEY_ID.to_string())))
            .expect("CoreRsaPrivateSigningKey::from_pem");

    let now = Utc::now();
    let expiration = now + Duration::hours(1);

    // Build KeycloakClaims (transparent wrapper over serde_json::Value) with
    // roles placed at the configured claim path.
    let mut additional_value = serde_json::Map::new();
    if !staged.user.roles.is_empty() {
        insert_at_path(
            &mut additional_value,
            &staged.role_claim_path,
            json!(staged.user.roles),
        );
    }
    let additional_claims = KeycloakClaims(Value::Object(additional_value));

    let standard_claims = StandardClaims::new(SubjectIdentifier::new(staged.user.subject.clone()))
        .set_email(Some(EndUserEmail::new(staged.user.email.clone())))
        .set_email_verified(Some(staged.user.email_verified));

    let claims: IdTokenClaims<KeycloakClaims, CoreGenderClaim> = IdTokenClaims::new(
        IssuerUrl::new(issuer.to_string()).expect("issuer url"),
        vec![Audience::new(TEST_CLIENT_ID.to_string())],
        expiration,
        now,
        standard_claims,
        additional_claims,
    )
    .set_nonce(Some(staged.nonce.clone()));

    let id_token: IdToken<
        KeycloakClaims,
        CoreGenderClaim,
        CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm,
    > = IdToken::new(
        claims,
        &signing_key,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        None,
        None,
    )
    .expect("sign id token");

    id_token.to_string()
}

/// Build a fake access token. The production code base64-decodes the
/// middle segment without verifying the signature (see
/// `extract_roles_from_access_token` in `src/oidc.rs`), so an unsigned
/// `{header}.{payload}.` is sufficient.
fn build_access_token(staged: &StagedToken) -> String {
    let header_json = r#"{"typ":"JWT","alg":"none"}"#;

    let mut payload_obj = serde_json::Map::new();
    if let Some(roles) = &staged.user.roles_in_access_token {
        insert_at_path(&mut payload_obj, &staged.role_claim_path, json!(roles));
    }
    let payload_json = serde_json::to_string(&Value::Object(payload_obj)).unwrap();

    let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header_json);
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload_json);

    format!("{}.{}.", header_b64, payload_b64)
}

/// Insert `value` at the dotted `path` inside `obj`, creating intermediate
/// objects as needed. Example: `insert_at_path(obj, "realm_access.roles",
/// json!(["admin"]))` produces `{"realm_access": {"roles": ["admin"]}}`.
fn insert_at_path(obj: &mut serde_json::Map<String, Value>, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        obj.insert(parts[0].to_string(), value);
        return;
    }

    let first = parts[0].to_string();
    let rest: String = parts[1..].join(".");
    let entry = obj
        .entry(first)
        .or_insert_with(|| Value::Object(Default::default()));
    if let Value::Object(nested) = entry {
        insert_at_path(nested, &rest, value);
    }
}

/// Extract the `state` query parameter from a `Location` header produced by
/// `/api/auth/oidc/login`. Test helper.
pub fn extract_query_param(url: &str, key: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}
