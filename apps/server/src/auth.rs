//! Authentication / Authorization primitives.
//!
//! The server is designed to act as an OAuth2/OIDC *resource server*:
//! an external IdP (e.g. Keycloak) performs interactive login, while this
//! server validates access tokens on incoming requests.

use axum::{
    extract::{FromRequestParts, State},
    http::{header, request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, TokenData, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

use crate::{
    request_context::RequestContext, services::audit::HttpAuditInput, state::AppState, Config,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    pub scopes: Vec<String>,
    pub issuer: Option<String>,
    pub audience: Option<Vec<String>>,
    pub client_id: Option<String>,
    pub patient: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuthError {
    MissingToken,
    InvalidToken(String),
    Misconfigured(String),
    Upstream(String),
}

impl AuthError {
    fn status(&self, required: bool) -> StatusCode {
        match self {
            Self::MissingToken => {
                if required {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::OK
                }
            }
            Self::InvalidToken(_) => StatusCode::UNAUTHORIZED,
            Self::Misconfigured(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Upstream(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn diagnostics(&self) -> String {
        match self {
            Self::MissingToken => "Missing bearer token".to_string(),
            Self::InvalidToken(msg) => format!("Invalid bearer token: {msg}"),
            Self::Misconfigured(msg) => format!("Authentication misconfigured: {msg}"),
            Self::Upstream(msg) => format!("Authentication upstream error: {msg}"),
        }
    }

    fn www_authenticate(&self) -> Option<&'static str> {
        match self {
            Self::MissingToken | Self::InvalidToken(_) => Some("Bearer"),
            Self::Misconfigured(_) | Self::Upstream(_) => None,
        }
    }

    pub fn into_fhir_response(self, required: bool) -> Response {
        let status = self.status(required);
        let body = axum::Json(json!({
            "resourceType": "OperationOutcome",
            "issue": [{
                "severity": "error",
                "code": "login",
                "diagnostics": self.diagnostics()
            }]
        }));

        let mut response = (status, body).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/fhir+json; charset=utf-8"),
        );
        if let Some(www) = self.www_authenticate() {
            if let Ok(v) = header::HeaderValue::from_str(www) {
                response.headers_mut().insert(header::WWW_AUTHENTICATE, v);
            }
        }
        response
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub jwks_uri: String,
}

#[derive(Debug, Default)]
struct JwksCache {
    jwks_uri: Option<String>,
    jwks: Option<JwkSet>,
    fetched_at: Option<Instant>,
    discovery: Option<OidcDiscovery>,
    discovery_fetched_at: Option<Instant>,
}

#[derive(Clone)]
pub struct AuthManager {
    config: Arc<Config>,
    http: reqwest::Client,
    jwks_cache: Arc<RwLock<JwksCache>>,
}

impl AuthManager {
    pub fn new(config: Arc<Config>) -> Result<Self, AuthError> {
        let timeout = Duration::from_secs(config.auth.oidc.http_timeout_seconds);
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| AuthError::Misconfigured(format!("Failed to build HTTP client: {e}")))?;

        Ok(Self {
            config,
            http,
            jwks_cache: Arc::new(RwLock::new(JwksCache::default())),
        })
    }

    pub fn enabled(&self) -> bool {
        self.config.auth.enabled
    }

    pub fn required(&self) -> bool {
        self.config.auth.required
    }

    pub fn is_public_path(&self, path: &str) -> bool {
        self.config.auth.public_paths.iter().any(|p| p == path)
    }

    pub async fn authenticate_headers(
        &self,
        headers: &HeaderMap,
    ) -> Result<Option<Principal>, AuthError> {
        if !self.enabled() {
            return Ok(None);
        }

        let Some(authz) = headers.get(header::AUTHORIZATION) else {
            return if self.required() {
                Err(AuthError::MissingToken)
            } else {
                Ok(None)
            };
        };

        let authz = authz.to_str().map_err(|_| {
            AuthError::InvalidToken("Authorization header is not valid UTF-8".to_string())
        })?;

        let token = authz
            .strip_prefix("Bearer ")
            .or_else(|| authz.strip_prefix("bearer "))
            .ok_or_else(|| {
                AuthError::InvalidToken("Authorization header must be 'Bearer <token>'".to_string())
            })?;

        let issuer = self.config.auth.oidc.issuer_url.clone().ok_or_else(|| {
            AuthError::Misconfigured("auth.oidc.issuer_url is not set".to_string())
        })?;
        let audience =
            self.config.auth.oidc.audience.clone().ok_or_else(|| {
                AuthError::Misconfigured("auth.oidc.audience is not set".to_string())
            })?;

        let token_data = self
            .decode_and_validate_jwt(token, &issuer, &audience)
            .await?;
        Ok(Some(self.principal_from_claims(token_data.claims)))
    }

    pub async fn oidc_discovery(&self) -> Result<OidcDiscovery, AuthError> {
        let ttl = Duration::from_secs(self.config.auth.oidc.jwks_cache_ttl_seconds);

        {
            let cache = self.jwks_cache.read().await;
            if let (Some(doc), Some(fetched_at)) = (&cache.discovery, cache.discovery_fetched_at) {
                if fetched_at.elapsed() <= ttl {
                    return Ok(doc.clone());
                }
            }
        }

        let issuer = self.config.auth.oidc.issuer_url.clone().ok_or_else(|| {
            AuthError::Misconfigured("auth.oidc.issuer_url is not set".to_string())
        })?;

        let url = format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        );

        let res = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| AuthError::Upstream(format!("OIDC discovery failed: {e}")))?;
        if !res.status().is_success() {
            return Err(AuthError::Upstream(format!(
                "OIDC discovery returned HTTP {}",
                res.status()
            )));
        }

        let doc: OidcDiscovery = res
            .json()
            .await
            .map_err(|e| AuthError::Upstream(format!("OIDC discovery JSON parse failed: {e}")))?;

        let mut cache = self.jwks_cache.write().await;
        cache.jwks_uri = Some(doc.jwks_uri.clone());
        cache.discovery = Some(doc.clone());
        cache.discovery_fetched_at = Some(Instant::now());

        Ok(doc)
    }

    fn principal_from_claims(&self, claims: serde_json::Value) -> Principal {
        let subject = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let scopes = extract_scopes(&claims);
        let issuer = claims
            .get("iss")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let audience = match claims.get("aud") {
            Some(serde_json::Value::String(s)) => Some(vec![s.clone()]),
            Some(serde_json::Value::Array(arr)) => Some(
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
            ),
            _ => None,
        };
        let client_id = claims
            .get("azp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                claims
                    .get("client_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
        let patient = claims
            .get("patient")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Principal {
            subject,
            scopes,
            issuer,
            audience,
            client_id,
            patient,
        }
    }

    async fn decode_and_validate_jwt(
        &self,
        token: &str,
        issuer: &str,
        audience: &str,
    ) -> Result<TokenData<serde_json::Value>, AuthError> {
        let header = decode_header(token)
            .map_err(|e| AuthError::InvalidToken(format!("Failed to decode JWT header: {e}")))?;

        let kid = header
            .kid
            .clone()
            .ok_or_else(|| AuthError::InvalidToken("JWT header missing 'kid'".to_string()))?;

        // Default to RS256. This matches common IdP defaults (Keycloak, etc.) and avoids
        // algorithm confusion. Extend this when you need ES256, etc.
        let alg = header.alg;
        if alg != Algorithm::RS256 {
            return Err(AuthError::InvalidToken(format!(
                "Unsupported JWT alg '{alg:?}' (only RS256 is supported)"
            )));
        }

        let jwks = self.get_jwks().await?;
        let jwk = jwks
            .find(&kid)
            .ok_or_else(|| AuthError::InvalidToken(format!("No matching JWK for kid '{kid}'")))?;
        let decoding_key = decoding_key_from_jwk(jwk)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[issuer]);
        validation.set_audience(&[audience]);
        validation.leeway = 60;

        decode::<serde_json::Value>(token, &decoding_key, &validation)
            .map_err(|e| AuthError::InvalidToken(format!("{e}")))
    }

    async fn get_jwks(&self) -> Result<JwkSet, AuthError> {
        let ttl = Duration::from_secs(self.config.auth.oidc.jwks_cache_ttl_seconds);

        {
            let cache = self.jwks_cache.read().await;
            if let (Some(jwks), Some(fetched_at)) = (&cache.jwks, cache.fetched_at) {
                if fetched_at.elapsed() <= ttl {
                    return Ok(jwks.clone());
                }
            }
        }

        let jwks_uri = self.get_jwks_uri().await?;
        let jwks = self.fetch_jwks(&jwks_uri).await?;

        let mut cache = self.jwks_cache.write().await;
        cache.jwks = Some(jwks.clone());
        cache.jwks_uri = Some(jwks_uri);
        cache.fetched_at = Some(Instant::now());
        Ok(jwks)
    }

    async fn get_jwks_uri(&self) -> Result<String, AuthError> {
        if let Some(uri) = self.config.auth.oidc.jwks_url.clone() {
            return Ok(uri);
        }

        {
            let cache = self.jwks_cache.read().await;
            if let Some(uri) = cache.jwks_uri.clone() {
                return Ok(uri);
            }
        }

        Ok(self.oidc_discovery().await?.jwks_uri)
    }

    async fn fetch_jwks(&self, jwks_uri: &str) -> Result<JwkSet, AuthError> {
        let res = self
            .http
            .get(jwks_uri)
            .send()
            .await
            .map_err(|e| AuthError::Upstream(format!("JWKS fetch failed: {e}")))?;
        if !res.status().is_success() {
            return Err(AuthError::Upstream(format!(
                "JWKS fetch returned HTTP {}",
                res.status()
            )));
        }
        res.json::<JwkSet>()
            .await
            .map_err(|e| AuthError::Upstream(format!("JWKS JSON parse failed: {e}")))
    }
}

fn decoding_key_from_jwk(jwk: &jsonwebtoken::jwk::Jwk) -> Result<DecodingKey, AuthError> {
    match &jwk.algorithm {
        AlgorithmParameters::RSA(rsa) => DecodingKey::from_rsa_components(&rsa.n, &rsa.e)
            .map_err(|e| AuthError::InvalidToken(format!("Failed to build RSA decoding key: {e}"))),
        _ => Err(AuthError::InvalidToken(
            "Unsupported JWK type (only RSA keys are supported)".to_string(),
        )),
    }
}

fn extract_scopes(claims: &serde_json::Value) -> Vec<String> {
    // SMART/OAuth typically uses `scope` as a space-delimited string.
    if let Some(scope_str) = claims.get("scope").and_then(|v| v.as_str()) {
        return scope_str
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
    }

    // Some providers use `scp` as an array.
    if let Some(arr) = claims.get("scp").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }

    Vec::new()
}

/// Extractor for the authenticated principal attached by middleware.
///
/// Use `Option<AuthenticatedPrincipal>` in handlers for optional auth.
#[derive(Debug, Clone)]
pub struct AuthenticatedPrincipal(pub Principal);

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthenticatedPrincipal
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Principal>()
            .cloned()
            .map(AuthenticatedPrincipal)
            .ok_or_else(|| AuthError::MissingToken.into_fhir_response(true))
    }
}

/// Middleware for attaching `Principal` (or rejecting) on protected routes.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if !state.auth.enabled() {
        return next.run(req).await;
    }

    let path = req.uri().path();
    if state.auth.is_public_path(path) || req.method() == axum::http::Method::OPTIONS {
        return next.run(req).await;
    }

    match state.auth.authenticate_headers(req.headers()).await {
        Ok(Some(principal)) => {
            req.extensions_mut().insert::<Principal>(principal);
            next.run(req).await
        }
        Ok(None) => next.run(req).await,
        Err(err) => {
            let required = state.auth.required();
            let status = err.status(required);
            let diagnostics = err.diagnostics();

            if state.audit_service.enabled().await {
                let method = req.method().as_str().to_string();
                // This middleware runs inside the nested `/fhir` router, so paths typically
                // do not include the `/fhir` prefix.
                let path_for_metrics = format!("/fhir{}", req.uri().path());
                let interaction = crate::metrics::extract_operation(&method, &path_for_metrics)
                    .unwrap_or_else(|| "operation".to_string());
                let action = match interaction.as_str() {
                    "create" => "C",
                    "read" | "vread" | "history" => "R",
                    "update" | "patch" => "U",
                    "delete" => "D",
                    _ => "E",
                }
                .to_string();

                if !state
                    .audit_service
                    .should_audit_interaction(&interaction)
                    .await
                    || !state
                        .audit_service
                        .should_audit_status(status.as_u16())
                        .await
                {
                    return err.into_fhir_response(required);
                }

                let target = {
                    let segments: Vec<&str> = req
                        .uri()
                        .path()
                        .split('/')
                        .filter(|s| !s.is_empty())
                        .collect();
                    if segments.len() >= 2
                        && !segments[0].starts_with('_')
                        && !segments[0].starts_with('$')
                        && segments[0] != "metadata"
                    {
                        Some((segments[0].to_string(), segments[1].to_string()))
                    } else {
                        None
                    }
                };

                let request_id = req
                    .extensions()
                    .get::<RequestContext>()
                    .map(|c| c.request_id.clone());

                let client_ip = req
                    .headers()
                    .get("x-forwarded-for")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.split(',').next())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        req.headers()
                            .get("x-real-ip")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    });

                let user_agent = req
                    .headers()
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                state
                    .audit_service
                    .enqueue_http(HttpAuditInput {
                        method,
                        interaction,
                        action,
                        status: status.as_u16(),
                        request_id,
                        principal: None,
                        client_ip,
                        user_agent,
                        target,
                        patient_id: None,
                        query_base64: None,
                        query_harmonized: None,
                        operation_outcome: Some(serde_json::json!({
                            "resourceType": "OperationOutcome",
                            "issue": [{
                                "severity": "error",
                                "code": "login",
                                "diagnostics": diagnostics,
                            }]
                        })),
                    })
                    .await;
            }

            err.into_fhir_response(required)
        }
    }
}
