//! CRUD operation handlers

use crate::{
    api::{
        content_negotiation::ContentNegotiation,
        extractors::FhirBody,
        headers::{
            extract_if_match, extract_if_modified_since, extract_if_none_exist,
            extract_if_none_match, extract_prefer_handling, extract_prefer_return, format_etag,
            get_prefer_header, FhirResponseHeaders, PreferReturn,
        },
        resource_formatter::ResourceFormatter,
        url as api_url,
    },
    models::{is_known_resource_type, HistoryMethod, ResourceOperation, UpdateParams},
    runtime_config::ConfigKey,
    services::conditional::parse_if_none_match_for_conditional_update,
    state::AppState,
    Result,
};
use axum::{
    body::Body,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use json_patch::PatchErrorKind;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};

fn parse_form_urlencoded(s: &str) -> Result<Vec<(String, String)>> {
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

async fn runtime_default_format(state: &AppState) -> String {
    state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await
}

async fn runtime_default_prefer_return(state: &AppState) -> String {
    state
        .runtime_config_cache
        .get(ConfigKey::FormatDefaultPreferReturn)
        .await
}

fn build_base_url(headers: &HeaderMap, request: &Request) -> String {
    // Prefer forwarding headers when present.
    let mut base_url = api_url::base_url_from_headers(headers);

    // If the request URI contains an explicit scheme (uncommon), prefer it.
    if let Some(scheme) = request.uri().scheme_str() {
        if let Some(rest) = base_url.split_once("://").map(|(_, rest)| rest) {
            base_url = format!("{}://{}", scheme, rest);
        }
    }

    base_url
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistorySort {
    LastUpdatedDesc,
    LastUpdatedAsc,
    None,
}

#[derive(Debug, Clone)]
struct HistoryQuery {
    count: Option<i32>,
    since: Option<DateTime<Utc>>,
    at: Option<DateTime<Utc>>,
    sort: HistorySort,
    query_params: HashMap<String, String>,
    raw_query: Option<String>,
}

fn parse_history_query(raw_query: Option<&str>) -> Result<HistoryQuery> {
    let raw_query_owned = raw_query.map(|s| s.to_string());
    let items = raw_query
        .map(parse_form_urlencoded)
        .transpose()?
        .unwrap_or_default();

    let mut seen: HashSet<String> = HashSet::new();
    let mut count: Option<i32> = None;
    let mut since: Option<DateTime<Utc>> = None;
    let mut at: Option<DateTime<Utc>> = None;
    let mut sort = HistorySort::LastUpdatedDesc;

    for (k, v) in &items {
        // History parameters SHALL NOT appear more than once.
        if !seen.insert(k.clone()) {
            return Err(crate::Error::Validation(format!(
                "History parameter '{}' must not appear more than once",
                k
            )));
        }

        match k.as_str() {
            "_count" => {
                let parsed: i32 = v.parse().map_err(|_| {
                    crate::Error::Validation(format!("Invalid _count value: {}", v))
                })?;
                if parsed < 0 {
                    return Err(crate::Error::Validation(
                        "_count must be a non-negative integer".to_string(),
                    ));
                }
                count = Some(parsed);
            }
            "_since" => {
                let parsed = chrono::DateTime::parse_from_rfc3339(v)
                    .map_err(|_| {
                        crate::Error::Validation(format!("Invalid _since instant: {}", v))
                    })?
                    .with_timezone(&Utc);
                since = Some(parsed);
            }
            "_sort" => {
                sort = match v.as_str() {
                    "-_lastUpdated" => HistorySort::LastUpdatedDesc,
                    "_lastUpdated" => HistorySort::LastUpdatedAsc,
                    "none" => HistorySort::None,
                    other => {
                        return Err(crate::Error::Validation(format!(
                            "Invalid _sort value for history: {}",
                            other
                        )));
                    }
                };
            }
            "_at" => {
                let parsed = chrono::DateTime::parse_from_rfc3339(v)
                    .map_err(|_| {
                        crate::Error::Validation(format!("Invalid _at instant: {}", v))
                    })?
                    .with_timezone(&Utc);
                at = Some(parsed);
            }
            "_list" => {
                return Err(crate::Error::NotImplemented(
                    "History parameter '_list' is not yet supported".to_string(),
                ));
            }
            "_format" | "_pretty" => {
                // handled via ContentNegotiation
            }
            other => {
                return Err(crate::Error::Validation(format!(
                    "Unsupported history parameter: {}",
                    other
                )));
            }
        }
    }

    if since.is_some() && at.is_some() {
        return Err(crate::Error::Validation(
            "History parameters '_since' and '_at' cannot be used together".to_string(),
        ));
    }

    Ok(HistoryQuery {
        count,
        since,
        at,
        sort,
        query_params: items_to_single_map_last(&items),
        raw_query: raw_query_owned,
    })
}

fn status_line(code: StatusCode) -> String {
    match code.canonical_reason() {
        Some(r) => format!("{} {}", code.as_u16(), r),
        None => code.as_u16().to_string(),
    }
}

fn build_history_entry(
    base_url: &str,
    method: HistoryMethod,
    resource_type: &str,
    id: &str,
    version_id: i32,
    last_updated: &chrono::DateTime<Utc>,
    resource: Option<JsonValue>,
) -> JsonValue {
    let method_str = match method {
        HistoryMethod::Post => "POST",
        HistoryMethod::Put => "PUT",
        HistoryMethod::Delete => "DELETE",
    };

    let response_status = match method {
        HistoryMethod::Delete => status_line(StatusCode::NO_CONTENT),
        _ => status_line(StatusCode::OK),
    };

    let mut entry = serde_json::json!({
        "fullUrl": format!("{}/{}/{}/_history/{}", base_url, resource_type, id, version_id),
        "request": {
            "method": method_str,
            "url": format!("{}/{}", resource_type, id)
        },
        "response": {
            "status": response_status,
            "etag": format_etag(version_id),
            "lastModified": last_updated.to_rfc3339()
        }
    });

    if let Some(resource) = resource {
        entry["resource"] = resource;
    }

    entry
}

/// Determine the effective Prefer header return value
///
/// Uses client's Prefer header if present, otherwise uses configured default.
/// Per FHIR spec: "In the absence of the header, servers may choose whether
/// to return the full resource or not."
fn get_effective_prefer_return(headers: &HeaderMap, default_config: &str) -> PreferReturn {
    // If client specified Prefer header, use that
    if get_prefer_header(headers).is_some() {
        return extract_prefer_return(headers);
    }

    // Otherwise use configured default
    match default_config.to_lowercase().as_str() {
        "minimal" => PreferReturn::Minimal,
        "operationoutcome" => PreferReturn::OperationOutcome,
        _ => PreferReturn::Representation,
    }
}

/// Format and create response with proper content negotiation
///
/// Returns a Response with:
/// - Correct Content-Type header for the negotiated format
/// - Formatted body (JSON, XML, etc.) with summary filtering applied
/// - 406 Not Acceptable if requested format is not supported
fn format_resource_response(
    resource: JsonValue,
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

/// Create a new resource (POST /[resourceType])
///
/// Spec-compliant behavior:
/// - 201 Created with Location header
/// - ETag and Last-Modified headers
/// - Prefer: return=minimal/representation
/// - If-None-Exist conditional create
pub async fn create_resource(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    FhirBody(resource): FhirBody,
) -> Result<Response> {
    let service = &state.crud_service;
    let base_url = api_url::base_url_from_headers(&headers);
    let mut resource = resource;

    let default_format = runtime_default_format(&state).await;
    let default_prefer_return = runtime_default_prefer_return(&state).await;

    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsTypeCreate,
        "create",
    )
    .await?;

    if !is_known_resource_type(&resource_type) {
        return Err(crate::Error::Validation(format!(
            "Invalid resource type: {}",
            resource_type
        )));
    }

    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let body_resource_type = resource
        .get("resourceType")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::Error::InvalidResource("Missing resourceType field".to_string()))?;
    if body_resource_type != resource_type {
        return Err(crate::Error::InvalidResource(format!(
            "Resource type mismatch: expected {}, got {}",
            resource_type, body_resource_type
        )));
    }

    if let Some(if_none_exist_raw) = extract_if_none_exist(&headers) {
        crate::api::fhir_access::ensure_interaction_enabled_runtime(
            &state,
            ConfigKey::InteractionsTypeConditionalCreate,
            "conditional-create",
        )
        .await?;

        let query = if_none_exist_raw.trim().trim_start_matches('?');
        let query_items = parse_form_urlencoded(query)?;

        let strict_handling =
            extract_prefer_handling(&headers) == crate::api::headers::PreferHandling::Strict;

        let conditional = state.conditional_service.clone();

        match conditional
            .conditional_create(
                &resource_type,
                &query_items,
                Some(&base_url),
                strict_handling,
            )
            .await?
        {
            crate::services::conditional::ConditionalCreateResult::NoMatch => { /* proceed */ }
            crate::services::conditional::ConditionalCreateResult::MatchFound { id } => {
                let existing = service.read_resource(&resource_type, &id).await?;

                let response_headers = FhirResponseHeaders::for_create_update(
                    &resource_type,
                    &existing.id,
                    existing.version_id,
                    &existing.last_updated,
                );

                let prefer_return = get_effective_prefer_return(&headers, &default_prefer_return);

                match prefer_return {
                    PreferReturn::Minimal => {
                        let response = StatusCode::OK.into_response();
                        return Ok(response_headers.apply_to_response(response));
                    }
                    PreferReturn::OperationOutcome => {
                        let operation_outcome = serde_json::json!({
                            "resourceType": "OperationOutcome",
                            "issue": [{
                                "severity": "information",
                                "code": "informational",
                                "diagnostics": format!(
                                    "Resource matched existing resource with ID {}",
                                    existing.id
                                )
                            }]
                        });
                        let base_response = StatusCode::OK.into_response();
                        let response = format_resource_response(
                            operation_outcome,
                            &params,
                            &headers,
                            &default_format,
                            base_response,
                        )?;
                        return Ok(response_headers.apply_to_response(response));
                    }
                    PreferReturn::Representation => {
                        let base_response = StatusCode::OK.into_response();
                        let response = format_resource_response(
                            existing.resource,
                            &params,
                            &headers,
                            &default_format,
                            base_response,
                        )?;
                        return Ok(response_headers.apply_to_response(response));
                    }
                }
            }
        }
    }

    state
        .conditional_reference_resolver
        .resolve(&mut resource, Some(&base_url))
        .await?;

    let result = service
        .create_resource(&resource_type, resource, None)
        .await?;

    // Build response headers
    let response_headers = FhirResponseHeaders::for_create_update(
        &resource_type,
        &result.resource.id,
        result.resource.version_id,
        &result.resource.last_updated,
    );

    // Build response based on operation type
    let status = match result.operation {
        ResourceOperation::Created => StatusCode::CREATED,
        ResourceOperation::NoOp => StatusCode::OK,
        _ => StatusCode::CREATED,
    };

    // Honor Prefer header - determine what to return
    let prefer_return = get_effective_prefer_return(&headers, &default_prefer_return);

    match prefer_return {
        PreferReturn::Minimal => {
            // Minimal response - no body, just headers
            let response = status.into_response();
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::OperationOutcome => {
            // Return OperationOutcome with hints about the operation
            let operation_outcome = serde_json::json!({
                "resourceType": "OperationOutcome",
                "issue": [{
                    "severity": "information",
                    "code": "informational",
                    "diagnostics": format!(
                        "Resource {} successfully with ID {}",
                        match result.operation {
                            ResourceOperation::Created => "created",
                            ResourceOperation::NoOp => "matched existing resource",
                            _ => "created"
                        },
                        result.resource.id
                    )
                }]
            });
            let base_response = status.into_response();
            let response = format_resource_response(
                operation_outcome,
                &params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::Representation => {
            // Return full resource representation
            let base_response = status.into_response();
            let response = format_resource_response(
                result.resource.resource,
                &params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
    }
}

/// HEAD request for a resource (HEAD /[resourceType]/[id])
///
/// Spec-compliant behavior:
/// - Same as read_resource but returns empty body
/// - Returns headers (ETag, Last-Modified) without body
/// - Useful for checking if a resource exists and getting version info
pub async fn head_resource(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceRead,
        "read",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;

    let resource = service.read_resource(&resource_type, &id).await?;

    // Build response headers (same as read)
    let response_headers =
        FhirResponseHeaders::for_read(resource.version_id, &resource.last_updated)
            .with_cache_control_max_age(30);
    let etag = response_headers.etag.as_ref().unwrap();

    // Handle conditional requests (same as read)
    if let Some(if_none_match) = extract_if_none_match(&headers) {
        if if_none_match == *etag {
            let response = StatusCode::NOT_MODIFIED.into_response();
            return Ok(response_headers.apply_to_response(response));
        }
    }

    if let Some(if_modified_since) = extract_if_modified_since(&headers) {
        if let Some(ref last_modified) = response_headers.last_modified {
            if if_modified_since == *last_modified {
                let response = StatusCode::NOT_MODIFIED.into_response();
                return Ok(response_headers.apply_to_response(response));
            }
        }
    }

    // Return 200 OK with headers but no body
    let response = StatusCode::OK.into_response();
    Ok(response_headers.apply_to_response(response))
}

/// Read a resource (GET /[resourceType]/[id])
///
/// Spec-compliant behavior:
/// - 200 OK with resource
/// - 404 Not Found
/// - 410 Gone if deleted
/// - ETag and Last-Modified headers
/// - If-Modified-Since and If-None-Match (304 Not Modified)
pub async fn read_resource(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceRead,
        "read",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;
    let default_format = runtime_default_format(&state).await;

    let resource = service.read_resource(&resource_type, &id).await?;

    // Build response headers
    let response_headers =
        FhirResponseHeaders::for_read(resource.version_id, &resource.last_updated)
            .with_cache_control_max_age(30);
    let etag = response_headers.etag.as_ref().unwrap();

    // Handle conditional read (If-None-Match)
    if let Some(if_none_match) = extract_if_none_match(&headers) {
        if if_none_match == *etag {
            let response = StatusCode::NOT_MODIFIED.into_response();
            return Ok(response_headers.apply_to_response(response));
        }
    }

    // Handle conditional read (If-Modified-Since)
    if let Some(if_modified_since) = extract_if_modified_since(&headers) {
        if let Some(ref last_modified) = response_headers.last_modified {
            if if_modified_since == *last_modified {
                let response = StatusCode::NOT_MODIFIED.into_response();
                return Ok(response_headers.apply_to_response(response));
            }
        }
    }

    // Build response with content negotiation
    let base_response = StatusCode::OK.into_response();
    let response = format_resource_response(
        resource.resource,
        &params,
        &headers,
        &default_format,
        base_response,
    )?;

    Ok(response_headers.apply_to_response(response))
}

/// Update a resource (PUT /[resourceType]/[id])
///
/// Spec-compliant behavior:
/// - 200 OK if updated, 201 Created if created via update
/// - Location header
/// - ETag and Last-Modified headers
/// - If-Match for version-aware updates (409/412 if mismatch)
pub async fn update_resource(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    FhirBody(resource): FhirBody,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceUpdate,
        "update",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;
    let default_format = runtime_default_format(&state).await;
    let default_prefer_return = runtime_default_prefer_return(&state).await;

    // Handle If-Match conditional update
    let if_match = extract_if_match(&headers);

    let update_params = if if_match.is_some() {
        Some(UpdateParams { if_match })
    } else {
        None
    };

    let base_url = api_url::base_url_from_headers(&headers);
    let mut resource = resource;
    state
        .conditional_reference_resolver
        .resolve(&mut resource, Some(&base_url))
        .await?;

    let result = service
        .update_resource(&resource_type, &id, resource, update_params)
        .await?;

    // Build response headers
    let response_headers = FhirResponseHeaders::for_create_update(
        &resource_type,
        &result.resource.id,
        result.resource.version_id,
        &result.resource.last_updated,
    );

    // Determine status based on operation
    let status = match result.operation {
        ResourceOperation::Created => StatusCode::CREATED,
        ResourceOperation::Updated => StatusCode::OK,
        _ => StatusCode::OK,
    };

    // Honor Prefer header - determine what to return
    let prefer_return = get_effective_prefer_return(&headers, &default_prefer_return);

    match prefer_return {
        PreferReturn::Minimal => {
            // Minimal response - no body, just headers
            let response = status.into_response();
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::OperationOutcome => {
            // Return OperationOutcome with hints about the operation
            let operation_outcome = serde_json::json!({
                "resourceType": "OperationOutcome",
                "issue": [{
                    "severity": "information",
                    "code": "informational",
                    "diagnostics": format!(
                        "Resource {} successfully with ID {}",
                        match result.operation {
                            ResourceOperation::Created => "created via update",
                            ResourceOperation::Updated => "updated",
                            _ => "updated"
                        },
                        result.resource.id
                    )
                }]
            });
            let base_response = status.into_response();
            let response = format_resource_response(
                operation_outcome,
                &params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::Representation => {
            // Return full resource representation
            let base_response = status.into_response();
            let response = format_resource_response(
                result.resource.resource,
                &params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
    }
}

/// Delete a resource (DELETE /[resourceType]/[id])
///
/// Spec-compliant behavior:
/// - 204 No Content on success
/// - 404 Not Found if doesn't exist
/// - Optional ETag for version tracking
pub async fn delete_resource(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceDelete,
        "delete",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;

    // Per spec: request body SHALL be empty.
    if !body.is_empty() {
        return Err(crate::Error::InvalidResource(
            "DELETE request body must be empty".to_string(),
        ));
    }

    let version_id = service.delete_resource(&resource_type, &id).await?;

    let mut response_headers = FhirResponseHeaders::new();
    if let Some(version_id) = version_id {
        response_headers = response_headers.with_etag(version_id);
    }

    let response = StatusCode::NO_CONTENT.into_response();
    Ok(response_headers.apply_to_response(response))
}

/// HEAD request for a specific version (HEAD /[resourceType]/[id]/_history/[vid])
///
/// Spec-compliant behavior:
/// - Same as vread_resource but returns empty body
/// - Returns headers (ETag, Last-Modified) without body
/// - 410 Gone if version was a deletion
pub async fn head_vread_resource(
    State(state): State<AppState>,
    Path((resource_type, id, vid)): Path<(String, String, i32)>,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceVread,
        "vread",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;

    let resource = service.vread_resource(&resource_type, &id, vid).await?;

    let response_headers =
        FhirResponseHeaders::for_read(resource.version_id, &resource.last_updated)
            .with_cache_control_immutable(31_536_000);

    // Return 200 OK with headers but no body
    let response = StatusCode::OK.into_response();
    Ok(response_headers.apply_to_response(response))
}

/// Read a specific version (GET /[resourceType]/[id]/_history/[vid])
///
/// Spec-compliant behavior:
/// - 200 OK with specific version
/// - 404 Not Found if version doesn't exist
/// - 410 Gone if version was a deletion
/// - ETag and Last-Modified headers
pub async fn vread_resource(
    State(state): State<AppState>,
    Path((resource_type, id, vid)): Path<(String, String, i32)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceVread,
        "vread",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;
    let default_format = runtime_default_format(&state).await;

    let resource = service.vread_resource(&resource_type, &id, vid).await?;

    let response_headers =
        FhirResponseHeaders::for_read(resource.version_id, &resource.last_updated)
            .with_cache_control_immutable(31_536_000);

    // Build response with content negotiation
    let base_response = StatusCode::OK.into_response();
    let response = format_resource_response(
        resource.resource,
        &params,
        &headers,
        &default_format,
        base_response,
    )?;

    Ok(response_headers.apply_to_response(response))
}

/// Get resource history (GET /[resourceType]/[id]/_history)
///
/// Spec-compliant behavior:
/// - 200 OK with Bundle containing history
/// - Supports _count and _since parameters
pub async fn resource_history(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceHistory,
        "history-instance",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let service = &state.crud_service;
    let default_format = runtime_default_format(&state).await;
    let history_query = parse_history_query(request.uri().query())?;
    let sort_ascending = matches!(history_query.sort, HistorySort::LastUpdatedAsc);

    let count = history_query.count;
    let since = history_query.since;
    let at = history_query.at;

    let history = service
        .resource_history(&resource_type, &id, count, since, at, sort_ascending)
        .await?;

    // Build Bundle per FHIR spec.
    // NOTE: Spec requires "sorted with oldest versions last" (i.e., newest first).
    // Store queries already apply ordering, but `_sort=none` is treated as implementation-defined.
    let base_url = build_base_url(&headers, &request);
    let mut entries = Vec::with_capacity(history.entries.len());
    for entry in history.entries {
        let resource = if matches!(entry.method, HistoryMethod::Delete) {
            None
        } else {
            Some(entry.resource.resource)
        };
        entries.push(build_history_entry(
            &base_url,
            entry.method,
            &resource_type,
            &entry.resource.id,
            entry.resource.version_id,
            &entry.resource.last_updated,
            resource,
        ));
    }

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "history",
        "total": history.total,
        "link": [{
            "relation": "self",
            "url": match history_query.raw_query.as_deref() {
                Some(q) if !q.is_empty() => format!("{}/{}/{}/_history?{}", base_url, resource_type, id, q),
                _ => format!("{}/{}/{}/_history", base_url, resource_type, id),
            }
        }],
        "entry": entries
    });

    // Format response with content negotiation.
    let base_response = StatusCode::OK.into_response();
    let response = format_resource_response(
        bundle,
        &history_query.query_params,
        &headers,
        &default_format,
        base_response,
    )?;

    let response_headers = FhirResponseHeaders::new().with_cache_control_no_cache();
    Ok(response_headers.apply_to_response(response))
}

/// Conditional update a resource (PUT /[resourceType]?query)
///
/// Spec-compliant behavior:
/// - Updates resource matching search criteria
/// - Creates if no match found (if allowed)
/// - Returns 200 OK if updated, 201 Created if created
/// - Returns 412 Precondition Failed if multiple matches
/// - Location, ETag and Last-Modified headers
pub async fn conditional_update_resource(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsTypeConditionalUpdate,
        "conditional-update",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let headers = request.headers().clone();
    let default_format = runtime_default_format(&state).await;
    let default_prefer_return = runtime_default_prefer_return(&state).await;

    let query_items = request
        .uri()
        .query()
        .map(parse_form_urlencoded)
        .transpose()?
        .unwrap_or_default();

    if query_items.is_empty() {
        return Err(crate::Error::Validation(
            "Conditional update requires search parameters in the query string".to_string(),
        ));
    }

    let query_params = items_to_single_map_last(&query_items);

    let base_url = build_base_url(&headers, &request);

    // Parse resource body (JSON or XML).
    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| crate::Error::Validation(format!("Failed to read request body: {}", e)))?;
    let mut resource: JsonValue =
        crate::api::extractors::parse_fhir_body(&body_bytes, &headers)?;

    // Determine target ID based on match results + optional client-provided id.
    let id_in_body = resource
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let strict_handling =
        extract_prefer_handling(&headers) == crate::api::headers::PreferHandling::Strict;
    let conditional = state.conditional_service.clone();
    let mut store = state.crud_service.clone();

    let resolution = conditional
        .resolve_conditional_target(
            &mut store,
            &resource_type,
            &query_items,
            Some(&base_url),
            strict_handling,
            id_in_body.as_deref(),
        )
        .await?;

    let if_none_match = parse_if_none_match_for_conditional_update(
        headers.get("if-none-match").and_then(|v| v.to_str().ok()),
    )?;
    conditional
        .check_if_none_match(
            &mut store,
            &resource_type,
            resolution.target_id.as_deref(),
            resolution.target_existed,
            if_none_match,
        )
        .await?;

    // Handle If-Match (version-aware update) when we have a target id.
    let if_match = extract_if_match(&headers);
    let update_params = if if_match.is_some() {
        Some(UpdateParams { if_match })
    } else {
        None
    };

    state
        .conditional_reference_resolver
        .resolve(&mut resource, Some(&base_url))
        .await?;

    let result = if let Some(id) = &resolution.target_id {
        state
            .crud_service
            .update_resource(&resource_type, id, resource, update_params)
            .await?
    } else {
        state
            .crud_service
            .create_resource(&resource_type, resource, None)
            .await?
    };

    let response_headers = FhirResponseHeaders::for_create_update(
        &resource_type,
        &result.resource.id,
        result.resource.version_id,
        &result.resource.last_updated,
    );

    let status = match result.operation {
        ResourceOperation::Created => StatusCode::CREATED,
        ResourceOperation::Updated => StatusCode::OK,
        _ => StatusCode::OK,
    };

    let prefer_return = get_effective_prefer_return(&headers, &default_prefer_return);

    match prefer_return {
        PreferReturn::Minimal => {
            let response = status.into_response();
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::OperationOutcome => {
            let operation_outcome = serde_json::json!({
                "resourceType": "OperationOutcome",
                "issue": [{
                    "severity": "information",
                    "code": "informational",
                    "diagnostics": format!(
                        "Resource {} successfully with ID {}",
                        match result.operation {
                            ResourceOperation::Created => "created",
                            ResourceOperation::Updated => "updated",
                            _ => "updated"
                        },
                        result.resource.id
                    )
                }]
            });
            let base_response = status.into_response();
            let response = format_resource_response(
                operation_outcome,
                &query_params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::Representation => {
            let base_response = status.into_response();
            let response = format_resource_response(
                result.resource.resource,
                &query_params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
    }
}

/// Patch a resource (PATCH /[resourceType]/[id])
///
/// Spec-compliant behavior:
/// - Applies JSON Patch, XML Patch, or FHIRPath Patch
/// - Returns 200 OK if updated
/// - Location, ETag and Last-Modified headers
/// - Supports If-Match for version-aware patches
pub async fn patch_resource(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstancePatch,
        "patch",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let default_format = runtime_default_format(&state).await;
    let default_prefer_return = runtime_default_prefer_return(&state).await;

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();

    if content_type.is_empty() {
        return Err(crate::Error::UnsupportedMediaType(
            "Missing Content-Type for PATCH request".to_string(),
        ));
    }

    if content_type != "application/json-patch+json" {
        return Err(crate::Error::UnsupportedMediaType(format!(
            "Unsupported PATCH Content-Type '{}'. Supported: application/json-patch+json",
            content_type
        )));
    }

    let patch: json_patch::Patch = serde_json::from_slice(&body).map_err(|e| {
        crate::Error::InvalidResource(format!("Invalid JSON Patch document: {}", e))
    })?;

    // Handle If-Match conditional patch (resource contention)
    let if_match = extract_if_match(&headers);
    let update_params = if if_match.is_some() {
        Some(UpdateParams { if_match })
    } else {
        None
    };

    let service = &state.crud_service;
    let current = service.read_resource(&resource_type, &id).await?;
    if let Some(expected_version) = if_match {
        if current.version_id != expected_version {
            return Err(crate::Error::VersionConflict {
                expected: expected_version,
                actual: current.version_id,
            });
        }
    }

    let mut patched = current.resource.clone();
    json_patch::patch(&mut patched, &patch.0).map_err(|e| match e.kind {
        PatchErrorKind::TestFailed => crate::Error::UnprocessableEntity(e.to_string()),
        _ => crate::Error::InvalidResource(e.to_string()),
    })?;

    let obj = patched.as_object_mut().ok_or_else(|| {
        crate::Error::InvalidResource("Patched resource must be a JSON object".to_string())
    })?;
    obj.insert(
        "resourceType".to_string(),
        serde_json::json!(&resource_type),
    );
    obj.insert("id".to_string(), serde_json::json!(&id));
    obj.remove("text");

    let base_url = api_url::base_url_from_headers(&headers);
    state
        .conditional_reference_resolver
        .resolve(&mut patched, Some(&base_url))
        .await?;

    let result = service
        .update_resource(&resource_type, &id, patched, update_params)
        .await?;

    let response_headers = FhirResponseHeaders::for_create_update(
        &resource_type,
        &result.resource.id,
        result.resource.version_id,
        &result.resource.last_updated,
    );

    let prefer_return = get_effective_prefer_return(&headers, &default_prefer_return);

    match prefer_return {
        PreferReturn::Minimal => {
            let response = StatusCode::OK.into_response();
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::OperationOutcome => {
            let operation_outcome = serde_json::json!({
                "resourceType": "OperationOutcome",
                "issue": [{
                    "severity": "information",
                    "code": "informational",
                    "diagnostics": format!("Resource patched successfully with ID {}", result.resource.id)
                }]
            });
            let base_response = StatusCode::OK.into_response();
            let response = format_resource_response(
                operation_outcome,
                &params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::Representation => {
            let base_response = StatusCode::OK.into_response();
            let response = format_resource_response(
                result.resource.resource,
                &params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
    }
}

/// Conditional patch a resource (PATCH /[resourceType]?query)
///
/// Spec-compliant behavior:
/// - Applies patch to resource matching search criteria
/// - Returns 200 OK if updated
/// - Returns 412 Precondition Failed if multiple matches
/// - Location, ETag and Last-Modified headers
pub async fn conditional_patch_resource(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsTypeConditionalPatch,
        "conditional-patch",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let headers = request.headers().clone();
    let default_format = runtime_default_format(&state).await;
    let default_prefer_return = runtime_default_prefer_return(&state).await;

    let query_items = request
        .uri()
        .query()
        .map(parse_form_urlencoded)
        .transpose()?
        .unwrap_or_default();

    if query_items.is_empty() {
        return Err(crate::Error::Validation(
            "Conditional patch requires search parameters in the query string".to_string(),
        ));
    }

    let query_params = items_to_single_map_last(&query_items);
    let base_url = build_base_url(&headers, &request);

    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();

    if content_type.is_empty() {
        return Err(crate::Error::UnsupportedMediaType(
            "Missing Content-Type for PATCH request".to_string(),
        ));
    }
    if content_type != "application/json-patch+json" {
        return Err(crate::Error::UnsupportedMediaType(format!(
            "Unsupported PATCH Content-Type '{}'. Supported: application/json-patch+json",
            content_type
        )));
    }

    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| crate::Error::Validation(format!("Failed to read request body: {}", e)))?;

    let patch: json_patch::Patch = serde_json::from_slice(&body_bytes).map_err(|e| {
        crate::Error::InvalidResource(format!("Invalid JSON Patch document: {}", e))
    })?;

    let strict_handling =
        extract_prefer_handling(&headers) == crate::api::headers::PreferHandling::Strict;
    let conditional = state.conditional_service.clone();
    let mut store = state.crud_service.clone();
    let resolution = conditional
        .resolve_conditional_target(
            &mut store,
            &resource_type,
            &query_items,
            Some(&base_url),
            strict_handling,
            None,
        )
        .await?;

    let Some(id) = resolution.target_id else {
        return Err(crate::Error::NotFound(
            "No resources match conditional patch criteria".to_string(),
        ));
    };

    // Handle If-Match conditional patch (resource contention)
    let if_match = extract_if_match(&headers);
    let update_params = if if_match.is_some() {
        Some(UpdateParams { if_match })
    } else {
        None
    };

    let result = state.crud_service;

    let current = result.read_resource(&resource_type, &id).await?;
    if let Some(expected_version) = if_match {
        if current.version_id != expected_version {
            return Err(crate::Error::VersionConflict {
                expected: expected_version,
                actual: current.version_id,
            });
        }
    }

    let mut patched = current.resource.clone();
    json_patch::patch(&mut patched, &patch.0).map_err(|e| match e.kind {
        PatchErrorKind::TestFailed => crate::Error::UnprocessableEntity(e.to_string()),
        _ => crate::Error::InvalidResource(e.to_string()),
    })?;

    let obj = patched.as_object_mut().ok_or_else(|| {
        crate::Error::InvalidResource("Patched resource must be a JSON object".to_string())
    })?;
    obj.insert(
        "resourceType".to_string(),
        serde_json::json!(&resource_type),
    );
    obj.insert("id".to_string(), serde_json::json!(&id));
    obj.remove("text");

    state
        .conditional_reference_resolver
        .resolve(&mut patched, Some(&base_url))
        .await?;

    let result = result
        .update_resource(&resource_type, &id, patched, update_params)
        .await?;

    let response_headers = FhirResponseHeaders::for_create_update(
        &resource_type,
        &result.resource.id,
        result.resource.version_id,
        &result.resource.last_updated,
    );

    let prefer_return = get_effective_prefer_return(&headers, &default_prefer_return);

    match prefer_return {
        PreferReturn::Minimal => {
            let response = StatusCode::OK.into_response();
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::OperationOutcome => {
            let operation_outcome = serde_json::json!({
                "resourceType": "OperationOutcome",
                "issue": [{
                    "severity": "information",
                    "code": "informational",
                    "diagnostics": format!("Resource patched successfully with ID {}", result.resource.id)
                }]
            });
            let base_response = StatusCode::OK.into_response();
            let response = format_resource_response(
                operation_outcome,
                &query_params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
        PreferReturn::Representation => {
            let base_response = StatusCode::OK.into_response();
            let response = format_resource_response(
                result.resource.resource,
                &query_params,
                &headers,
                &default_format,
                base_response,
            )?;
            Ok(response_headers.apply_to_response(response))
        }
    }
}

/// Delete all historical versions of a resource (DELETE /[resourceType]/[id]/_history)
///
/// Spec-compliant behavior:
/// - 204 No Content on success
/// - 404 Not Found if resource doesn't exist
pub async fn delete_resource_history(
    State(state): State<AppState>,
    Path((resource_type, id)): Path<(String, String)>,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceDeleteHistory,
        "delete-history",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    state
        .crud_service
        .delete_resource_history(&resource_type, &id)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Delete a specific version of a resource (DELETE /[resourceType]/[id]/_history/[vid])
///
/// Spec-compliant behavior:
/// - 204 No Content on success
/// - 404 Not Found if version doesn't exist
pub async fn delete_resource_history_version(
    State(state): State<AppState>,
    Path((resource_type, id, vid)): Path<(String, String, i32)>,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsInstanceDeleteHistoryVersion,
        "delete-history-version",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    state
        .crud_service
        .delete_resource_history_version(&resource_type, &id, vid)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Conditionally delete resource(s) (DELETE /[resourceType]?query)
///
/// Spec-compliant behavior:
/// - 204 No Content if single resource deleted
/// - 200 OK with OperationOutcome if multiple resources deleted
/// - 412 Precondition Failed if multiple matches (when not allowed)
/// - Returns count of deleted resources
pub async fn conditional_delete_resource(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsTypeConditionalDelete,
        "conditional-delete",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let headers = request.headers().clone();

    let query_items = request
        .uri()
        .query()
        .map(parse_form_urlencoded)
        .transpose()?
        .unwrap_or_default();

    if query_items.is_empty() {
        return Err(crate::Error::Validation(
            "Conditional delete requires search parameters in the query string".to_string(),
        ));
    }

    let base_url = build_base_url(&headers, &request);

    // Per spec: request body SHALL be empty.
    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| crate::Error::Validation(format!("Failed to read request body: {}", e)))?;
    if !body_bytes.is_empty() {
        return Err(crate::Error::InvalidResource(
            "DELETE request body must be empty".to_string(),
        ));
    }

    let strict_handling =
        extract_prefer_handling(&headers) == crate::api::headers::PreferHandling::Strict;
    let conditional = state.conditional_service.clone();
    let mut store = state.crud_service.clone();
    let resolution = conditional
        .resolve_conditional_target(
            &mut store,
            &resource_type,
            &query_items,
            Some(&base_url),
            strict_handling,
            None,
        )
        .await?;

    let Some(id) = resolution.target_id else {
        return Err(crate::Error::NotFound(
            "No resources match conditional delete criteria".to_string(),
        ));
    };

    // Optional: Conditional delete with If-Match.
    if let Some(expected_version) = extract_if_match(&headers) {
        let current = state
            .crud_service
            .read_resource(&resource_type, &id)
            .await?;
        if current.version_id != expected_version {
            return Err(crate::Error::VersionConflict {
                expected: expected_version,
                actual: current.version_id,
            });
        }
    }

    let version_id = state
        .crud_service
        .delete_resource(&resource_type, &id)
        .await?;

    let mut response_headers = FhirResponseHeaders::new();
    if let Some(version_id) = version_id {
        response_headers = response_headers.with_etag(version_id);
    }

    let response = StatusCode::NO_CONTENT.into_response();
    Ok(response_headers.apply_to_response(response))
}

/// Get resource type history (GET /[resourceType]/_history)
///
/// Spec-compliant behavior:
/// - 200 OK with Bundle containing history for all resources of this type
/// - Supports _count and _since parameters
pub async fn type_history(
    State(state): State<AppState>,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsTypeHistory,
        "history-type",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    let default_format = runtime_default_format(&state).await;
    let history_query = parse_history_query(request.uri().query())?;
    let sort_ascending = matches!(history_query.sort, HistorySort::LastUpdatedAsc);
    let count = history_query.count;
    let since = history_query.since;
    let at = history_query.at;

    let history = state
        .crud_service
        .type_history(&resource_type, count, since, at, sort_ascending)
        .await?;

    let base_url = build_base_url(&headers, &request);
    let mut entries = Vec::with_capacity(history.entries.len());
    for entry in history.entries {
        let resource = if matches!(entry.method, HistoryMethod::Delete) {
            None
        } else {
            Some(entry.resource.resource)
        };
        entries.push(build_history_entry(
            &base_url,
            entry.method,
            &entry.resource.resource_type,
            &entry.resource.id,
            entry.resource.version_id,
            &entry.resource.last_updated,
            resource,
        ));
    }

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "history",
        "link": [{
            "relation": "self",
            "url": match history_query.raw_query.as_deref() {
                Some(q) if !q.is_empty() => format!("{}/{}/_history?{}", base_url, resource_type, q),
                _ => format!("{}/{}/_history", base_url, resource_type),
            }
        }],
        "entry": entries
    });

    let base_response = StatusCode::OK.into_response();
    let response = format_resource_response(
        bundle,
        &history_query.query_params,
        &headers,
        &default_format,
        base_response,
    )?;

    let response_headers = FhirResponseHeaders::new().with_cache_control_no_cache();
    Ok(response_headers.apply_to_response(response))
}

/// Conditionally delete resources across all types (DELETE /?query)
///
/// Spec-compliant behavior:
/// - 200 OK with OperationOutcome containing count of deleted resources
/// - Supports search parameters across all resource types
pub async fn system_delete(State(state): State<AppState>, request: Request) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsSystemDelete,
        "delete-system",
    )
    .await?;

    let headers = request.headers().clone();

    let query_items = request
        .uri()
        .query()
        .map(parse_form_urlencoded)
        .transpose()?
        .unwrap_or_default();

    let base_url = build_base_url(&headers, &request);

    // Per spec: request body SHALL be empty.
    let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|e| crate::Error::Validation(format!("Failed to read request body: {}", e)))?;
    if !body_bytes.is_empty() {
        return Err(crate::Error::InvalidResource(
            "DELETE request body must be empty".to_string(),
        ));
    }

    let strict_handling =
        extract_prefer_handling(&headers) == crate::api::headers::PreferHandling::Strict;
    let expected_version = extract_if_match(&headers);
    let version_id = state
        .system_service
        .system_delete(&query_items, &base_url, strict_handling, expected_version)
        .await?;

    let mut response_headers = FhirResponseHeaders::new();
    if let Some(version_id) = version_id {
        response_headers = response_headers.with_etag(version_id);
    }

    let response = StatusCode::NO_CONTENT.into_response();
    Ok(response_headers.apply_to_response(response))
}

/// Get system-wide history (GET /_history)
///
/// Spec-compliant behavior:
/// - 200 OK with Bundle containing history for all resources
/// - Supports _count and _since parameters
pub async fn system_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsSystemHistory,
        "history-system",
    )
    .await?;

    let default_format = runtime_default_format(&state).await;
    let history_query = parse_history_query(request.uri().query())?;
    let sort_ascending = matches!(history_query.sort, HistorySort::LastUpdatedAsc);
    let count = history_query.count;
    let since = history_query.since;
    let at = history_query.at;

    let history = state
        .crud_service
        .system_history(count, since, at, sort_ascending)
        .await?;

    let base_url = build_base_url(&headers, &request);
    let mut entries = Vec::with_capacity(history.entries.len());
    for entry in history.entries {
        let resource = if matches!(entry.method, HistoryMethod::Delete) {
            None
        } else {
            Some(entry.resource.resource)
        };
        entries.push(build_history_entry(
            &base_url,
            entry.method,
            &entry.resource.resource_type,
            &entry.resource.id,
            entry.resource.version_id,
            &entry.resource.last_updated,
            resource,
        ));
    }

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "history",
        "link": [{
            "relation": "self",
            "url": match history_query.raw_query.as_deref() {
                Some(q) if !q.is_empty() => format!("{}/_history?{}", base_url, q),
                _ => format!("{}/_history", base_url),
            }
        }],
        "entry": entries
    });

    let base_response = StatusCode::OK.into_response();
    let response = format_resource_response(
        bundle,
        &history_query.query_params,
        &headers,
        &default_format,
        base_response,
    )?;

    let response_headers = FhirResponseHeaders::new().with_cache_control_no_cache();
    Ok(response_headers.apply_to_response(response))
}
