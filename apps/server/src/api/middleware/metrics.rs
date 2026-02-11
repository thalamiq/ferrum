//! Metrics middleware - tracks HTTP request metrics

use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

/// Metrics middleware - tracks HTTP request metrics
pub async fn metrics_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Sanitize path for metrics (remove IDs to reduce cardinality)
    let sanitized_path = crate::metrics::sanitize_path(&path);

    // Track in-flight requests
    crate::metrics::HTTP_REQUESTS_IN_FLIGHT
        .with_label_values(&[&method, &sanitized_path])
        .inc();

    // Extract content-length if present
    if let Some(content_length) = req
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<f64>().ok())
    {
        crate::metrics::HTTP_REQUEST_SIZE_BYTES
            .with_label_values(&[&method, &sanitized_path])
            .observe(content_length);
    }

    // Process request
    let response = next.run(req).await;

    // Record metrics after request completion
    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    // Update counters and histograms
    crate::metrics::HTTP_REQUESTS_TOTAL
        .with_label_values(&[&method, &sanitized_path, &status])
        .inc();

    crate::metrics::HTTP_REQUEST_DURATION_SECONDS
        .with_label_values(&[&method, &sanitized_path])
        .observe(duration);

    // Track response size if present
    if let Some(content_length) = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<f64>().ok())
    {
        crate::metrics::HTTP_RESPONSE_SIZE_BYTES
            .with_label_values(&[&method, &sanitized_path, &status])
            .observe(content_length);
    }

    // Track FHIR-specific operations
    if let Some(resource_type) = crate::metrics::extract_resource_type(&path) {
        if let Some(operation) = crate::metrics::extract_operation(&method, &path) {
            let fhir_status = if response.status().is_success() {
                "success"
            } else if response.status().is_client_error() {
                "client_error"
            } else {
                "server_error"
            };

            crate::metrics::FHIR_OPERATIONS_TOTAL
                .with_label_values(&[&resource_type, &operation, fhir_status])
                .inc();

            crate::metrics::FHIR_OPERATION_DURATION_SECONDS
                .with_label_values(&[&resource_type, &operation])
                .observe(duration);

            // Track search operations separately
            if operation == "search" {
                crate::metrics::FHIR_SEARCH_TOTAL
                    .with_label_values(&[&resource_type, fhir_status])
                    .inc();
            }
        }
    }

    // Decrement in-flight requests
    crate::metrics::HTTP_REQUESTS_IN_FLIGHT
        .with_label_values(&[&method, &sanitized_path])
        .dec();

    response
}
