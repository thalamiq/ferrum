//! URL helpers for building absolute FHIR base URLs.

use axum::http::HeaderMap;

/// Build the FHIR base URL (`{scheme}://{host}/fhir`) using forwarding headers when present.
///
/// This is important for correct Bundle links and CapabilityStatement URLs when running behind
/// reverse proxies.
pub fn base_url_from_headers(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .or_else(|| headers.get("x-forwarded-scheme"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");

    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");

    format!("{}://{}/fhir", scheme, host)
}
