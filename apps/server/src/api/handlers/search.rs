//! Search operation handlers
//!
//! Handles FHIR search operations:
//! - Type-level search (GET/POST /{resource_type})
//! - System-level search (GET/POST /)
//! - Compartment search:
//!   - GET  /{compartment_type}/{compartment_id}/*{?params}
//!   - POST /{compartment_type}/{compartment_id}/_search{?params}
//!   - GET  /{compartment_type}/{compartment_id}/{resource_type}{?params}
//!   - POST /{compartment_type}/{compartment_id}/{resource_type}/_search{?params}

use crate::{
    api::{
        content_negotiation::ContentNegotiation, headers::extract_prefer_handling,
        resource_formatter::ResourceFormatter, url as api_url,
    },
    runtime_config::ConfigKey,
    state::AppState,
    Result,
};
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::collections::HashMap;

/// Common search handler logic shared across all search operations
///
/// Handles:
/// - Extracting base URL, method, and body from request
/// - Parsing search parameters from query string and POST body
/// - Executing the search via the provided closure
/// - Checking for unknown parameters
/// - Formatting the response with content negotiation
async fn handle_search<F, Fut>(
    _state: &AppState,
    headers: &HeaderMap,
    request: Request,
    resource_context: &str,
    default_format: &str,
    execute_search: F,
) -> Result<Response>
where
    F: FnOnce(Vec<(String, String)>, String, String) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value>>,
{
    // Extract base URL (scheme://host/fhir), honoring forwarding headers.
    let uri = request.uri();
    let raw_query = uri.query().map(|s| s.to_string());
    let base_url = api_url::base_url_from_headers(headers);

    // Extract method and body
    let method = request.method().clone();
    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| crate::Error::Validation(format!("Failed to read request body: {}", e)))?;

    // Extract and merge parameters from query string and POST body
    let items = extract_search_items(&method, raw_query.as_deref(), headers, &body_bytes).await?;
    let query_params = items_to_single_map_last(&items);

    // Build query string
    let query_string = build_query_string(&items);

    // Execute search via provided closure (pass owned values to avoid lifetime issues)
    let bundle_result = execute_search(items, query_string, base_url).await?;

    // Check for unknown parameters and handle based on Prefer header
    let bundle = check_unknown_params(bundle_result, headers, resource_context)?;

    // Format response with content negotiation
    let base_response = StatusCode::OK.into_response();
    format_search_response(
        bundle,
        &query_params,
        headers,
        default_format,
        base_response,
    )
}

/// Format and create search response with proper content negotiation
///
/// Returns a Response with:
/// - Correct Content-Type header for the negotiated format
/// - Formatted body (JSON, XML, etc.)
/// - 406 Not Acceptable if requested format is not supported
fn format_search_response(
    bundle: serde_json::Value,
    query_params: &HashMap<String, String>,
    headers: &HeaderMap,
    default_format: &str,
    base_response: Response,
) -> Result<Response> {
    // Extract content negotiation preferences
    let negotiation = ContentNegotiation::from_request(query_params, headers, default_format);

    // Check if requested format is supported
    if !negotiation.format.is_supported() {
        return Err(crate::Error::Validation(format!(
            "Unsupported format: {}. Supported formats: application/fhir+json, application/fhir+xml",
            negotiation.format.mime_type()
        )));
    }

    // Format the bundle
    let formatter = ResourceFormatter::new(negotiation);
    let formatted_body = formatter
        .format_resource(bundle)
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
    parts.headers.insert(
        "cache-control",
        "no-cache"
            .parse()
            .map_err(|e| crate::Error::Internal(format!("Invalid cache-control: {}", e)))?,
    );

    Ok(Response::from_parts(parts, Body::from(formatted_body)))
}

/// Search resources of a specific type (GET/POST /{resource_type})
///
/// Spec-compliant behavior:
/// - 200 OK with Bundle (type=searchset)
/// - Supports all FHIR search parameters
/// - Returns Bundle with search results and metadata
pub async fn search_type(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsTypeSearch,
        "search-type",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.search_service;
    let resource_type_clone = resource_type.clone();
    let default_format: String = state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await;

    handle_search(
        &state,
        &headers,
        request,
        &resource_type,
        &default_format,
        |items, query_string, base_url| async move {
            service
                .search_type(&resource_type_clone, &items, &query_string, &base_url)
                .await
        },
    )
    .await
}

/// Search across all resource types (GET/POST /)
///
/// Spec-compliant behavior:
/// - 200 OK with Bundle (type=searchset)
/// - Requires _type parameter to specify resource types
/// - Returns Bundle with search results from multiple types
pub async fn search_system(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsSystemSearch,
        "search-system",
    )
    .await?;

    let service = &state.search_service;
    let default_format: String = state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await;

    handle_search(
        &state,
        &headers,
        request,
        "system",
        &default_format,
        |items, query_string, base_url| async move {
            service
                .search_system(&items, &query_string, &base_url)
                .await
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
pub(crate) struct CompartmentSearchPath {
    compartment_type: String,
    compartment_id: String,
    #[serde(default)]
    resource_type: Option<String>,
}

/// Search resources within a compartment
///
/// Spec-compliant behavior:
/// - 200 OK with Bundle (type=searchset)
/// - Searches only resources accessible within the compartment
///
/// Notes:
/// - For "all-types" compartment search, FHIR uses a literal `*` segment in the URL:
///   `GET /{compartment_type}/{compartment_id}/*`
/// - For POST-based "all-types" searches, FHIR uses the `_search` literal:
///   `POST /{compartment_type}/{compartment_id}/_search`
/// - If the compartment instance does not exist, the search engine currently returns an empty Bundle.
pub(crate) async fn search_compartment(
    State(state): State<AppState>,
    Path(path): Path<CompartmentSearchPath>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsCompartmentSearch,
        "search-compartment",
    )
    .await?;

    let service = &state.search_service;

    let CompartmentSearchPath {
        compartment_type,
        compartment_id,
        resource_type,
    } = path;

    crate::api::fhir_access::ensure_resource_type_supported(&state, &compartment_type)?;

    // In FHIR, `*` is a literal path segment meaning "all resource types in this compartment".
    // Internally we represent that with `None`.
    let resource_type = match resource_type {
        Some(rt) if rt == "*" => None,
        other => other,
    };

    if let Some(rt) = &resource_type {
        crate::api::fhir_access::ensure_resource_type_supported(&state, rt)?;
    }

    // Determine resource context for unknown params checking
    let default_context = format!("{}/{}", compartment_type, compartment_id);
    let resource_context = resource_type.as_deref().unwrap_or(&default_context);

    let compartment_type_clone = compartment_type.clone();
    let compartment_id_clone = compartment_id.clone();
    let resource_type_clone = resource_type.clone();
    let default_format: String = state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await;

    handle_search(
        &state,
        &headers,
        request,
        resource_context,
        &default_format,
        |items, query_string, base_url| async move {
            service
                .search_compartment(
                    &compartment_type_clone,
                    &compartment_id_clone,
                    resource_type_clone.as_deref(),
                    &items,
                    &query_string,
                    &base_url,
                )
                .await
        },
    )
    .await
}

/// Extract and merge search parameters from query string and POST body.
///
/// Per FHIR spec:
/// - GET: Parameters from query string only
/// - POST: Parameters from query string AND body (application/x-www-form-urlencoded)
async fn extract_search_items(
    method: &Method,
    raw_query: Option<&str>,
    headers: &HeaderMap,
    body_bytes: &[u8],
) -> Result<Vec<(String, String)>> {
    let mut items = Vec::new();

    // Query string items (already percent-decoded).
    if let Some(q) = raw_query {
        items.extend(parse_form_urlencoded(q)?);
    }

    // For POST requests, extract parameters from body
    if method == Method::POST && !body_bytes.is_empty() {
        // Check Content-Type
        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Parse based on content type
        if content_type.contains("application/x-www-form-urlencoded") {
            // Parse form-urlencoded body
            let body_str = std::str::from_utf8(body_bytes).map_err(|_| {
                crate::Error::Validation("Invalid UTF-8 in request body".to_string())
            })?;
            items.extend(parse_form_urlencoded(body_str)?);
        } else if !content_type.is_empty()
            && !content_type.contains("application/x-www-form-urlencoded")
        {
            return Err(crate::Error::UnsupportedMediaType(format!(
                "POST search requires Content-Type: application/x-www-form-urlencoded, got: {}",
                content_type
            )));
        }
    }

    Ok(items)
}

fn parse_form_urlencoded(s: &str) -> Result<Vec<(String, String)>> {
    // `url::form_urlencoded` implements `application/x-www-form-urlencoded` semantics (including '+' = space).
    Ok(url::form_urlencoded::parse(s.as_bytes())
        .into_owned()
        .collect())
}

fn items_to_single_map_last(items: &[(String, String)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in items {
        map.insert(k.clone(), v.clone());
    }
    map
}

/// Build query string from (key, value) items.
fn build_query_string(items: &[(String, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in items {
        serializer.append_pair(k, v);
    }
    serializer.finish()
}

/// Check for unknown parameters and handle based on Prefer header
///
/// Per FHIR spec on unknown/unsupported parameters:
/// - If Prefer: handling=strict, returns error for unknown params
/// - If Prefer: handling=lenient (default), silently ignores them
/// - Removes the temporary _unknown_params field from Bundle
fn check_unknown_params(
    mut bundle: serde_json::Value,
    headers: &HeaderMap,
    resource_type: &str,
) -> Result<serde_json::Value> {
    let handling = extract_prefer_handling(headers);

    if let Some(bundle_obj) = bundle.as_object_mut() {
        if let Some(unknown_params) = bundle_obj.remove("_unknown_params") {
            if let Some(unknown_array) = unknown_params.as_array() {
                if !unknown_array.is_empty()
                    && handling == crate::api::headers::PreferHandling::Strict
                {
                    let unknown_list: Vec<String> = unknown_array
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();

                    return Err(crate::Error::Validation(format!(
                        "Unknown or unsupported search parameters for {}: {}",
                        resource_type,
                        unknown_list.join(", ")
                    )));
                }
            }
        }
    }

    Ok(bundle)
}
