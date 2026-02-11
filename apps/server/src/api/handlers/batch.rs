//! Batch and transaction bundle handlers

use crate::{
    api::{
        content_negotiation::ContentNegotiation,
        extractors::FhirBody,
        headers::extract_prefer_return,
        resource_formatter::ResourceFormatter,
    },
    runtime_config::ConfigKey,
    services::batch::{BundleRequestOptions, PreferReturn as BatchPreferReturn},
    state::AppState,
    Result,
};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use url::Url;

fn build_base_url_from_headers(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    format!("{}://{}/fhir", scheme, host)
}

/// Handle batch, transaction, and history bundle POST requests (POST /fhir/)
///
/// Accepts FHIR Bundle with type 'batch', 'transaction', or 'history' and processes
/// all entries according to FHIR specification:
/// - Batch: Entries processed independently (parallel)
/// - Transaction: Entries processed atomically with specific ordering
/// - History: Entries processed sequentially for replication (idempotent, non-atomic)
///
/// After processing, queues background indexing jobs for affected resources.
pub async fn batch_transaction(
    State(state): State<AppState>,
    Query(query_params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    FhirBody(bundle): FhirBody,
) -> Result<Response> {
    tracing::info!("Received batch/transaction request");

    let default_format: String = state
        .runtime_config_cache
        .get(ConfigKey::FormatDefault)
        .await;

    // Convert from api::headers::PreferReturn to services::batch::PreferReturn
    let prefer_return = match extract_prefer_return(&headers) {
        crate::api::headers::PreferReturn::Minimal => BatchPreferReturn::Minimal,
        crate::api::headers::PreferReturn::Representation => BatchPreferReturn::Representation,
        crate::api::headers::PreferReturn::OperationOutcome => BatchPreferReturn::OperationOutcome,
    };

    let bundle_type = bundle
        .get("type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    validate_bundle_entry_access(&state, &bundle).await?;

    let options = BundleRequestOptions {
        prefer_return,
        base_url: Some(build_base_url_from_headers(&headers)),
    };

    let response_bundle = match bundle_type.as_str() {
        "batch" => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                &state,
                ConfigKey::InteractionsSystemBatch,
                "batch",
            )
            .await?;
            state
                .batch_service
                .process_bundle_with_options(bundle, options)
                .await?
        }
        "transaction" => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                &state,
                ConfigKey::InteractionsSystemTransaction,
                "transaction",
            )
            .await?;
            state
                .transaction_service
                .process_bundle_with_options(bundle, options)
                .await?
        }
        "history" => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                &state,
                ConfigKey::InteractionsSystemHistoryBundle,
                "history-bundle",
            )
            .await?;
            state
                .history_service
                .process_bundle_with_options(bundle, options)
                .await?
        }
        other => {
            return Err(crate::Error::InvalidResource(format!(
                "Unsupported Bundle.type '{}'. POST to [base] requires type 'batch', 'transaction', or 'history'",
                other
            )));
        }
    };

    // Format response with content negotiation (_format / Accept).
    let negotiation = ContentNegotiation::from_request(&query_params, &headers, &default_format);
    if !negotiation.format.is_supported() {
        return Err(crate::Error::Validation(format!(
            "Unsupported format: {}. Supported formats: application/fhir+json, application/fhir+xml",
            negotiation.format.mime_type()
        )));
    }

    let formatter = ResourceFormatter::new(negotiation);
    let formatted_body = formatter
        .format_resource(response_bundle)
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

async fn validate_bundle_entry_access(state: &AppState, bundle: &JsonValue) -> Result<()> {
    let Some(entries) = bundle.get("entry").and_then(|v| v.as_array()) else {
        return Ok(());
    };

    for (idx, entry) in entries.iter().enumerate() {
        let req = entry
            .get("request")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                crate::Error::InvalidResource(format!("Bundle.entry[{idx}].request is required"))
            })?;

        let method = req.get("method").and_then(|v| v.as_str()).ok_or_else(|| {
            crate::Error::InvalidResource(format!("Bundle.entry[{idx}].request.method is required"))
        })?;
        let url = req.get("url").and_then(|v| v.as_str()).ok_or_else(|| {
            crate::Error::InvalidResource(format!("Bundle.entry[{idx}].request.url is required"))
        })?;

        validate_entry_request(state, method, url)
            .await
            .map_err(|e| match e {
                crate::Error::MethodNotAllowed(msg) => {
                    crate::Error::MethodNotAllowed(format!("Bundle.entry[{idx}]: {msg}"))
                }
                crate::Error::Validation(msg) => {
                    crate::Error::Validation(format!("Bundle.entry[{idx}]: {msg}"))
                }
                crate::Error::InvalidResource(msg) => {
                    crate::Error::InvalidResource(format!("Bundle.entry[{idx}]: {msg}"))
                }
                other => other,
            })?;
    }

    Ok(())
}

async fn validate_entry_request(state: &AppState, method: &str, request_url: &str) -> Result<()> {
    let method = method.to_ascii_uppercase();
    let (path, _query) = normalize_fhir_request_url(request_url)?;
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return Ok(());
    }

    // System-level endpoints
    match segments[0] {
        "metadata" => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                state,
                ConfigKey::InteractionsSystemCapabilities,
                "capabilities",
            )
            .await?;
            return Ok(());
        }
        "_search" => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                state,
                ConfigKey::InteractionsSystemSearch,
                "search-system",
            )
            .await?;
            return Ok(());
        }
        "_history" => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                state,
                ConfigKey::InteractionsSystemHistory,
                "history-system",
            )
            .await?;
            return Ok(());
        }
        seg if seg.starts_with('$') => {
            crate::api::fhir_access::ensure_interaction_enabled_runtime(
                state,
                ConfigKey::InteractionsOperationsSystem,
                "operation-system",
            )
            .await?;
            return Ok(());
        }
        _ => {}
    }

    // Type/instance endpoints (first segment is the resource type)
    let resource_type = segments[0];
    crate::api::fhir_access::ensure_resource_type_supported(state, resource_type)?;

    // Type-level /{type}/_search, /{type}/_history, /{type}/$op
    if segments.len() == 2 {
        match segments[1] {
            "_search" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeSearch,
                    "search-type",
                )
                .await?;
                return Ok(());
            }
            "_history" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeHistory,
                    "history-type",
                )
                .await?;
                return Ok(());
            }
            seg if seg.starts_with('$') => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsOperationsTypeLevel,
                    "operation-type",
                )
                .await?;
                return Ok(());
            }
            _ => {}
        }
    }

    // Type-level /{type}
    if segments.len() == 1 {
        match method.as_str() {
            "GET" | "HEAD" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeSearch,
                    "search-type",
                )
                .await?
            }
            "POST" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeCreate,
                    "create",
                )
                .await?
            }
            "PUT" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeConditionalUpdate,
                    "conditional-update",
                )
                .await?
            }
            "PATCH" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeConditionalPatch,
                    "conditional-patch",
                )
                .await?
            }
            "DELETE" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsTypeConditionalDelete,
                    "conditional-delete",
                )
                .await?
            }
            _ => {}
        }
        return Ok(());
    }

    // Instance-level /{type}/{id} and /{type}/{id}/$op
    if segments.len() == 2 {
        match method.as_str() {
            "GET" | "HEAD" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsInstanceRead,
                    "read",
                )
                .await?
            }
            "PUT" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsInstanceUpdate,
                    "update",
                )
                .await?
            }
            "PATCH" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsInstancePatch,
                    "patch",
                )
                .await?
            }
            "DELETE" => {
                crate::api::fhir_access::ensure_interaction_enabled_runtime(
                    state,
                    ConfigKey::InteractionsInstanceDelete,
                    "delete",
                )
                .await?
            }
            _ => {}
        }
        return Ok(());
    }

    // History endpoints:
    // - /{type}/{id}/_history
    // - /{type}/{id}/_history/{vid}
    if segments.len() >= 3 && segments[2] == "_history" {
        if segments.len() == 3 {
            match method.as_str() {
                "GET" | "HEAD" => {
                    crate::api::fhir_access::ensure_interaction_enabled_runtime(
                        state,
                        ConfigKey::InteractionsInstanceHistory,
                        "history-instance",
                    )
                    .await?
                }
                "DELETE" => {
                    crate::api::fhir_access::ensure_interaction_enabled_runtime(
                        state,
                        ConfigKey::InteractionsInstanceDeleteHistory,
                        "delete-history",
                    )
                    .await?
                }
                _ => {}
            }
            return Ok(());
        }

        if segments.len() == 4 {
            match method.as_str() {
                "GET" | "HEAD" => {
                    crate::api::fhir_access::ensure_interaction_enabled_runtime(
                        state,
                        ConfigKey::InteractionsInstanceVread,
                        "vread",
                    )
                    .await?
                }
                "DELETE" => {
                    crate::api::fhir_access::ensure_interaction_enabled_runtime(
                        state,
                        ConfigKey::InteractionsInstanceDeleteHistoryVersion,
                        "delete-history-version",
                    )
                    .await?
                }
                _ => {}
            }
            return Ok(());
        }
    }

    // Instance-level operations: /{type}/{id}/$op
    if segments.len() == 3 && segments[2].starts_with('$') {
        crate::api::fhir_access::ensure_interaction_enabled_runtime(
            state,
            ConfigKey::InteractionsOperationsInstance,
            "operation-instance",
        )
        .await?;
        return Ok(());
    }

    Ok(())
}

fn normalize_fhir_request_url(request_url: &str) -> Result<(String, Option<String>)> {
    let (path, query) = if request_url.starts_with("http://") || request_url.starts_with("https://")
    {
        let parsed = Url::parse(request_url)
            .map_err(|e| crate::Error::Validation(format!("Invalid request.url: {e}")))?;
        (
            parsed.path().to_string(),
            parsed.query().map(|q| q.to_string()),
        )
    } else {
        let (p, q) = request_url.split_once('?').unwrap_or((request_url, ""));
        (
            p.to_string(),
            if q.is_empty() {
                None
            } else {
                Some(q.to_string())
            },
        )
    };

    // Strip optional leading "/fhir" prefix if present in an absolute URL.
    let path = path
        .strip_prefix("/fhir/")
        .or_else(|| path.strip_prefix("/fhir"))
        .unwrap_or(path.as_str())
        .trim_start_matches('/')
        .to_string();

    Ok((path, query))
}
