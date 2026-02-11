//! Metadata endpoint handler
//!
//! Handles the FHIR capabilities interaction (GET /metadata)

use crate::{
    api::{content_negotiation::ContentNegotiation, resource_formatter::ResourceFormatter},
    runtime_config::ConfigKey,
    services::metadata::CapabilityMode,
    state::AppState,
    Result,
};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;

/// Get server capability statement (GET /metadata)
///
/// Per FHIR spec (http://hl7.org/fhir/http.html#capabilities):
/// - Supports `mode` parameter: full (default), normative, terminology
/// - Returns CapabilityStatement or TerminologyCapabilities
/// - Should include ETag header
/// - Supports _summary and _elements parameters
/// - Should check for fhirVersion MIME-type parameter
pub async fn capability_statement(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsSystemCapabilities,
        "capabilities",
    )
    .await?;

    // Extract mode parameter
    let mode = params
        .get("mode")
        .and_then(|m| CapabilityMode::try_from_str(m))
        .unwrap_or(CapabilityMode::Full);

    // TODO: Implement normative mode
    if mode == CapabilityMode::Normative {
        return Err(crate::Error::Validation(format!(
            "Mode '{}' not yet supported. Supported modes: 'full', 'terminology'.",
            params.get("mode").unwrap_or(&"".to_string())
        )));
    }

    // Generate capability statement
    let base_url = crate::api::url::base_url_from_headers(&headers);
    let capability_statement = state
        .metadata_service
        .get_capability_statement(mode, &base_url)
        .await?;

    let default_format: String = state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await;

    // Build response with content negotiation
    let base_response = StatusCode::OK.into_response();
    let response = format_resource_response(
        capability_statement,
        &params,
        &headers,
        &default_format,
        base_response,
    )?;

    // Add ETag header
    // ETag changes when server capabilities change
    // For now, use a hash of the software version + config
    let etag = format!(
        "W/\"{}-{}\"",
        state.config.fhir.capability_statement.software_version, state.config.fhir.version
    );

    let (mut parts, body) = response.into_parts();
    parts.headers.insert(
        "etag",
        etag.parse()
            .map_err(|e| crate::Error::Internal(format!("Invalid ETag: {}", e)))?,
    );

    Ok(Response::from_parts(parts, body))
}

/// Helper function to format resource response with content negotiation
fn format_resource_response(
    resource: serde_json::Value,
    query_params: &HashMap<String, String>,
    headers: &HeaderMap,
    default_format: &str,
    base_response: Response,
) -> Result<Response> {
    let negotiation = ContentNegotiation::from_request(query_params, headers, default_format);

    // Check if requested format is supported
    if !negotiation.format.is_supported() {
        return Err(crate::Error::Validation(format!(
            "Unsupported format: {}. Supported formats: application/fhir+json, application/fhir+xml",
            negotiation.format.mime_type()
        )));
    }

    // Format the resource
    let formatter = ResourceFormatter::new(negotiation);
    let formatted_body = formatter
        .format_resource(resource)
        .map_err(|e| crate::Error::Internal(e.to_string()))?;

    // Build response with correct Content-Type
    let (mut parts, _) = base_response.into_parts();
    parts.headers.insert(
        "content-type",
        formatter
            .content_type()
            .parse()
            .map_err(|e| crate::Error::Internal(format!("Invalid content type: {}", e)))?,
    );

    Ok(Response::from_parts(parts, Body::from(formatted_body)))
}
