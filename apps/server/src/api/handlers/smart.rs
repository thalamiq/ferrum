//! SMART on FHIR discovery endpoints.
//!
//! This is a minimal implementation of:
//! `<fhirBase>/.well-known/smart-configuration`
//!
//! It is intended to support clients discovering the external authorization server (IdP)
//! used to obtain access tokens.

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;

pub async fn smart_configuration(State(state): State<AppState>) -> Response {
    if !state.auth.enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }

    match state.auth.oidc_discovery().await {
        Ok(doc) => {
            let issuer = doc
                .issuer
                .or_else(|| state.config.auth.oidc.issuer_url.clone())
                .unwrap_or_default();

            let body = Json(json!({
                "issuer": issuer,
                "jwks_uri": doc.jwks_uri,
                "authorization_endpoint": doc.authorization_endpoint,
                "token_endpoint": doc.token_endpoint,
                // Keep this intentionally minimal; when SMART authz is fully implemented,
                // expand this list based on the deployed authorization server features.
                "capabilities": ["client-public"]
            }));

            let mut response = (StatusCode::OK, body).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            response
        }
        Err(e) => e.into_fhir_response(state.auth.required()),
    }
}
