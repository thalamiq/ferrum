//! Operation handlers

use crate::{
    api::{content_negotiation::ContentNegotiation, resource_formatter::ResourceFormatter},
    models::{OperationContext, OperationRequest, OperationResult, Parameters},
    runtime_config::ConfigKey,
    state::AppState,
    Result,
};
use axum::{
    body::Body,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;

/// System-level operation: POST [base]/$operation
pub async fn operation_system(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Query(query): Query<Vec<(String, String)>>,
    Path(operation): Path<String>,
    body: Bytes,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsOperationsSystem,
        "operation-system",
    )
    .await?;

    execute_operation(
        state,
        headers,
        method,
        operation,
        OperationContext::System,
        query,
        body,
    )
    .await
}

/// Type-level operation: POST [base]/{type}/$operation
pub async fn operation_type(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Query(query): Query<Vec<(String, String)>>,
    Path((resource_type, operation)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsOperationsTypeLevel,
        "operation-type",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    execute_operation(
        state,
        headers,
        method,
        operation,
        OperationContext::Type(resource_type),
        query,
        body,
    )
    .await
}

/// Instance-level operation: POST [base]/{type}/{id}/$operation
pub async fn operation_instance(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Query(query): Query<Vec<(String, String)>>,
    Path((resource_type, id, operation)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Response> {
    crate::api::fhir_access::ensure_interaction_enabled_runtime(
        &state,
        ConfigKey::InteractionsOperationsInstance,
        "operation-instance",
    )
    .await?;
    crate::api::fhir_access::ensure_resource_type_supported(&state, &resource_type)?;

    execute_operation(
        state,
        headers,
        method,
        operation,
        OperationContext::Instance(resource_type, id),
        query,
        body,
    )
    .await
}

async fn execute_operation(
    state: AppState,
    headers: HeaderMap,
    method: Method,
    operation: String,
    context: OperationContext,
    query: Vec<(String, String)>,
    body: Bytes,
) -> Result<Response> {
    let default_format: String = state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await;

    // Validate operation exists and context is appropriate
    let op_meta = state
        .operation_registry
        .find_operation(&operation, &context)
        .await?
        .ok_or_else(|| crate::Error::Validation(format!("Operation ${} not found", operation)))?;

    // Per spec, GET is only allowed for idempotent operations.
    if method == Method::GET && op_meta.affects_state {
        return Err(crate::Error::MethodNotAllowed(format!(
            "Operation ${} does not allow GET",
            operation
        )));
    }

    // Parse parameters: POST prefers body (FHIR Parameters), GET uses query string.
    // If both are present, merge query parameters into the Parameters resource.
    let mut parameters: Parameters = if body.is_empty() {
        Parameters::new()
    } else {
        let value: serde_json::Value =
            crate::api::extractors::parse_fhir_body(&body, &headers)?;
        serde_json::from_value(value).map_err(|e| {
            crate::Error::Validation(format!("Invalid Parameters resource: {}", e))
        })?
    };

    if !query.is_empty() {
        let mut query_grouped: HashMap<String, Vec<String>> = HashMap::new();
        for (k, v) in &query {
            query_grouped.entry(k.clone()).or_default().push(v.clone());
        }

        for (k, values) in query_grouped {
            for v in values {
                match k.as_str() {
                    // Control parameters (formatting) - do not include in operation parameters.
                    "_format" | "_pretty" => {}
                    "coding" => {
                        // Common shorthand: coding=system|code
                        let (system, code) = v
                            .split_once('|')
                            .map(|(a, b)| (a.trim(), b.trim()))
                            .unwrap_or(("", v.trim()));
                        let mut coding = serde_json::Map::new();
                        if !system.is_empty() {
                            coding.insert(
                                "system".to_string(),
                                serde_json::Value::String(system.to_string()),
                            );
                        }
                        if !code.is_empty() {
                            coding.insert(
                                "code".to_string(),
                                serde_json::Value::String(code.to_string()),
                            );
                        }
                        parameters.add_value_coding(k.clone(), serde_json::Value::Object(coding));
                    }
                    _ => {
                        if v.eq_ignore_ascii_case("true") {
                            parameters.add_value_boolean(k.clone(), true);
                        } else if v.eq_ignore_ascii_case("false") {
                            parameters.add_value_boolean(k.clone(), false);
                        } else if let Ok(i) = v.parse::<i64>() {
                            parameters.add_value_integer(k.clone(), i);
                        } else {
                            parameters.add_value_string(k.clone(), v);
                        }
                    }
                }
            }
        }
    }

    // Validate parameters
    state
        .operation_registry
        .validate_parameters(&op_meta, &parameters)
        .await?;

    // Execute operation
    let request = OperationRequest {
        operation_name: operation.clone(),
        context,
        parameters,
    };

    let result = state.operation_executor.execute(request).await?;

    // Build query map for content negotiation.
    let mut query_params = HashMap::new();
    for (k, v) in query {
        query_params.insert(k, v);
    }

    // Format response with content negotiation (_format / Accept).
    match result {
        OperationResult::NoContent => Ok(StatusCode::NO_CONTENT.into_response()),
        OperationResult::Resource(resource) => {
            let negotiation =
                ContentNegotiation::from_request(&query_params, &headers, &default_format);
            if !negotiation.format.is_supported() {
                return Err(crate::Error::Validation(format!(
                    "Unsupported format: {}. Supported formats: application/fhir+json, application/fhir+xml",
                    negotiation.format.mime_type()
                )));
            }

            let formatter = ResourceFormatter::new(negotiation);
            let formatted_body = formatter
                .format_resource(resource)
                .map_err(|e| crate::Error::Internal(e.to_string()))?;

            let base_response = StatusCode::OK.into_response();
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
        OperationResult::Parameters(params) => {
            let negotiation =
                ContentNegotiation::from_request(&query_params, &headers, &default_format);
            if !negotiation.format.is_supported() {
                return Err(crate::Error::Validation(format!(
                    "Unsupported format: {}. Supported formats: application/fhir+json, application/fhir+xml",
                    negotiation.format.mime_type()
                )));
            }

            let payload = serde_json::to_value(params).map_err(|e| {
                crate::Error::Internal(format!("Failed to serialize Parameters: {}", e))
            })?;
            let formatter = ResourceFormatter::new(negotiation);
            let formatted_body = formatter
                .format_resource(payload)
                .map_err(|e| crate::Error::Internal(e.to_string()))?;

            let base_response = StatusCode::OK.into_response();
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
        OperationResult::OperationOutcome(outcome) => {
            let negotiation =
                ContentNegotiation::from_request(&query_params, &headers, &default_format);
            if !negotiation.format.is_supported() {
                return Err(crate::Error::Validation(format!(
                    "Unsupported format: {}. Supported formats: application/fhir+json, application/fhir+xml",
                    negotiation.format.mime_type()
                )));
            }

            let formatter = ResourceFormatter::new(negotiation);
            let formatted_body = formatter
                .format_resource(outcome)
                .map_err(|e| crate::Error::Internal(e.to_string()))?;

            let base_response = StatusCode::OK.into_response();
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
    }
}
