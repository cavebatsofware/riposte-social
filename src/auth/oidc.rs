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
use anyhow::Result;
use openidconnect::{
    core::{
        CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreErrorResponseType,
        CoreGenderClaim, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreRevocableToken, CoreTokenType,
    },
    AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken, EmptyExtraTokenFields,
    EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenFields, IssuerUrl, Nonce,
    OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl,
    RevocationErrorResponseType, Scope, StandardErrorResponse, StandardTokenIntrospectionResponse,
    StandardTokenResponse, TokenResponse,
};
use serde::{Deserialize, Serialize};
use std::env;

/// Additional claims from Keycloak ID tokens.
/// Captures the full JSON payload for flexible role extraction from custom claim paths.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct KeycloakClaims(pub serde_json::Value);

impl openidconnect::AdditionalClaims for KeycloakClaims {}

/// Client type with Keycloak claims and discovered OIDC provider endpoints.
type KeycloakClient = Client<
    KeycloakClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    StandardTokenResponse<
        IdTokenFields<
            KeycloakClaims,
            EmptyExtraTokenFields,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
        >,
        CoreTokenType,
    >,
    StandardTokenIntrospectionResponse<EmptyExtraTokenFields, CoreTokenType>,
    CoreRevocableToken,
    StandardErrorResponse<RevocationErrorResponseType>,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub role_claim: String,
    pub admin_role: String,
    pub poster_role: String,
}

impl OidcConfig {
    pub fn from_env() -> Self {
        let enabled = env::var("OIDC_ENABLED").unwrap_or_default().to_lowercase() == "true";

        Self {
            enabled,
            issuer_url: env::var("OIDC_ISSUER_URL").unwrap_or_default(),
            client_id: env::var("OIDC_CLIENT_ID").unwrap_or_default(),
            client_secret: crate::secret::load("OIDC_CLIENT_SECRET")
                .map(|mut s| std::mem::take(&mut *s))
                .unwrap_or_default(),
            redirect_uri: env::var("OIDC_REDIRECT_URI").unwrap_or_default(),
            scopes: env::var("OIDC_SCOPES")
                .unwrap_or_else(|_| "openid profile email".to_string())
                .split_whitespace()
                .map(String::from)
                .collect(),
            role_claim: env::var("OIDC_ROLE_CLAIM")
                .unwrap_or_else(|_| "realm_access.roles".to_string()),
            admin_role: env::var("OIDC_ADMIN_ROLE").unwrap_or_else(|_| "admin".to_string()),
            poster_role: env::var("OIDC_POSTER_ROLE").unwrap_or_else(|_| "poster".to_string()),
        }
    }
}

/// Resolve the IdP-claimed roles to our app's tier with most-privileged-wins.
/// This is used by the OIDC callback to surface a single `idp_tier` value into
/// `Credentials::Oidc` for drift validation against the DB role. It is never
/// used to assign role, only to validate.
pub fn resolve_idp_tier(roles: &[String], cfg: &OidcConfig) -> &'static str {
    use crate::entities::user;
    if roles.iter().any(|r| r == &cfg.admin_role) {
        user::ROLE_ADMINISTRATOR
    } else if roles.iter().any(|r| r == &cfg.poster_role) {
        user::ROLE_POSTER
    } else {
        user::ROLE_COMMENTER
    }
}

#[derive(Clone)]
pub struct OidcService {
    pub config: OidcConfig,
    client: Option<KeycloakClient>,
    http_client: reqwest::Client,
}

impl OidcService {
    pub async fn new(config: OidcConfig) -> Result<Self> {
        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        if !config.enabled {
            return Ok(Self {
                config,
                client: None,
                http_client,
            });
        }

        let issuer_url = IssuerUrl::new(config.issuer_url.clone())?;
        let provider_metadata =
            CoreProviderMetadata::discover_async(issuer_url, &http_client).await?;

        let client: KeycloakClient = Client::from_provider_metadata(
            provider_metadata,
            ClientId::new(config.client_id.clone()),
            Some(ClientSecret::new(config.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        tracing::info!("OIDC client initialized for issuer: {}", config.issuer_url);

        Ok(Self {
            config,
            client: Some(client),
            http_client,
        })
    }

    /// Generate the authorization URL + PKCE verifier + nonce for login redirect
    pub fn authorization_url(&self) -> Result<(url::Url, CsrfToken, Nonce, PkceCodeVerifier)> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OIDC not configured"))?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let mut auth_request = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);

        for scope in &self.config.scopes {
            if scope != "openid" {
                auth_request = auth_request.add_scope(Scope::new(scope.clone()));
            }
        }

        let (auth_url, csrf_token, nonce) = auth_request.url();

        Ok((auth_url, csrf_token, nonce, pkce_verifier))
    }

    /// Exchange auth code for tokens, validate ID token, return user info
    pub async fn exchange_code(
        &self,
        code: &str,
        pkce_verifier: PkceCodeVerifier,
        nonce: &Nonce,
    ) -> Result<OidcUserInfo> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OIDC not configured"))?;

        let token_response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))?
            .set_pkce_verifier(pkce_verifier)
            .request_async(&self.http_client)
            .await
            .map_err(|e| anyhow::anyhow!("Token exchange failed: {}", e))?;

        let id_token = token_response
            .id_token()
            .ok_or_else(|| anyhow::anyhow!("No ID token in response"))?;

        let id_token_verifier = client.id_token_verifier();
        let claims = id_token.claims(&id_token_verifier, nonce)?;

        let email = claims
            .email()
            .ok_or_else(|| anyhow::anyhow!("No email claim in ID token"))?
            .to_string();

        let email_verified = claims.email_verified().unwrap_or(false);

        // OIDC `sub` is the stable, provider-issued user identifier. Required
        // for the unified UserAuthBackend's OIDC-linkage hard boundary.
        let sub = claims.subject().to_string();

        // Optional display name from `name` / `preferred_username` claims.
        let display_name = claims
            .name()
            .and_then(|localized| localized.get(None).map(|s| s.as_str().to_string()))
            .or_else(|| claims.preferred_username().map(|s| s.as_str().to_string()));

        // Try extracting roles from ID token first
        let additional = &claims.additional_claims().0;
        let mut roles = self.extract_roles_from_claims(additional);
        tracing::debug!("ID token additional claims: {:?}", additional);
        tracing::debug!("Roles from ID token: {:?}", roles);

        // Fallback: extract roles from access token (Keycloak includes realm_access
        // in the access token by default, but not always in the ID token)
        if roles.is_empty() {
            if let Some(access_token_roles) = Self::extract_roles_from_access_token(
                token_response.access_token().secret(),
                &self.config.role_claim,
            ) {
                tracing::info!(
                    "Roles not found in ID token, extracted from access token: {:?}",
                    access_token_roles
                );
                roles = access_token_roles;
            } else {
                tracing::warn!(
                    "No roles found in ID token or access token for claim path '{}'",
                    self.config.role_claim
                );
            }
        }

        Ok(OidcUserInfo {
            sub,
            email,
            email_verified,
            display_name,
            roles,
        })
    }

    /// Decode a Keycloak JWT access token and extract roles from its claims.
    /// Keycloak access tokens are JWTs that typically contain realm_access.roles.
    /// No signature verification needed since the ID token was already verified.
    fn extract_roles_from_access_token(
        access_token: &str,
        role_claim: &str,
    ) -> Option<Vec<String>> {
        let parts: Vec<&str> = access_token.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .ok()?;
        let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;

        let claim_parts: Vec<&str> = role_claim.split('.').collect();
        let mut current = &claims;
        for part in &claim_parts[..claim_parts.len().saturating_sub(1)] {
            current = current.get(part)?;
        }

        let last_part = claim_parts.last()?;
        let arr = current.get(last_part)?.as_array()?;
        let roles: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if roles.is_empty() {
            None
        } else {
            Some(roles)
        }
    }

    /// Extract roles from Keycloak token claims.
    /// Handles nested claim paths like "realm_access.roles".
    fn extract_roles_from_claims(&self, additional_claims: &serde_json::Value) -> Vec<String> {
        let parts: Vec<&str> = self.config.role_claim.split('.').collect();
        let mut current = additional_claims;

        for part in &parts[..parts.len().saturating_sub(1)] {
            match current.get(part) {
                Some(v) => current = v,
                None => return vec![],
            }
        }

        if let Some(last_part) = parts.last() {
            if let Some(arr) = current.get(last_part).and_then(|v| v.as_array()) {
                return arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
        }

        vec![]
    }
}

#[derive(Debug, Clone)]
pub struct OidcUserInfo {
    pub sub: String,
    pub email: String,
    pub email_verified: bool,
    pub display_name: Option<String>,
    pub roles: Vec<String>,
}
