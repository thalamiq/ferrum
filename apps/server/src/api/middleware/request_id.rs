//! Request ID middleware with OpenTelemetry trace context injection

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use opentelemetry::trace::TraceContextExt;
use std::time::Instant;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

use crate::request_context::RequestContext;

/// Request ID middleware with OpenTelemetry trace context injection
///
/// Creates root span for each HTTP request and:
/// - Extracts W3C Trace Context from headers (traceparent, tracestate)
/// - Generates server request ID
/// - Adds trace_id and span_id to response headers
/// - Attaches structured context fields for FHIR operations
///
/// Per FHIR spec (3.2.0.18 Custom Headers):
/// - Server assigns X-Request-Id in response (uses client's ID or generates new one)
/// - If server ID differs from client ID, echo client ID in X-Correlation-Id
#[tracing::instrument(
    name = "http_request",
    skip_all,
    fields(
        http.method = %req.method(),
        http.route = %req.uri().path(),
        http.scheme = %req.uri().scheme_str().unwrap_or("http"),
        otel.kind = "server",
        http.response.status_code = tracing::field::Empty,
        fhir.resource_type = tracing::field::Empty,
        fhir.operation = tracing::field::Empty,
        request_id = tracing::field::Empty,
    )
)]
pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let current_span = Span::current();
    let start = Instant::now();

    let client_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let server_id = Uuid::new_v4().to_string();
    current_span.record("request_id", &server_id);

    // Make request ID available to inner middleware/handlers.
    let mut req = req;
    req.extensions_mut().insert(RequestContext {
        request_id: server_id.clone(),
    });

    // Extract FHIR context from path (clone before moving req)
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    if let Some(resource_type) = crate::metrics::extract_resource_type(&path) {
        current_span.record("fhir.resource_type", &resource_type);
    }
    if let Some(operation) = crate::metrics::extract_operation(req.method().as_str(), &path) {
        current_span.record("fhir.operation", &operation);
    }

    // Log incoming request
    tracing::debug!(
        method = %method,
        path = %path,
        request_id = %server_id,
        "Incoming request"
    );

    let mut response = next.run(req).await;

    let status = response.status();
    let duration = start.elapsed();
    current_span.record("http.response.status_code", status.as_u16());

    // Log request completion
    tracing::info!(
        method = %method,
        path = %path,
        status = %status.as_u16(),
        duration_ms = duration.as_millis(),
        request_id = %server_id,
        "Request completed"
    );

    // Add trace context to response headers
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&server_id) {
        headers.insert("x-request-id", value);
    }

    // Add trace ID for debugging
    let trace_id = current_span
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();
    if let Ok(value) = HeaderValue::from_str(&trace_id) {
        headers.insert("x-trace-id", value);
    }

    // Echo client correlation ID if different
    if let Some(client_id) = client_id {
        if client_id != server_id {
            if let Ok(value) = HeaderValue::from_str(&client_id) {
                headers.insert("x-correlation-id", value);
            }
        }
    }

    response
}
