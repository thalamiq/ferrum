//! Metrics endpoint handler
//!
//! Exposes Prometheus-compatible metrics for monitoring

use axum::{extract::State, http::StatusCode, response::IntoResponse};
use prometheus::{Encoder, TextEncoder};

use crate::state::AppState;

/// Handler for /metrics endpoint
/// Returns Prometheus text format metrics
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();

    let mut buffer = vec![];
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => {
            // Collect custom application metrics via service
            let custom_metrics = state
                .metrics_service
                .collect_custom_metrics(env!("CARGO_PKG_VERSION"), &state.config.fhir.version)
                .await;
            buffer.extend_from_slice(custom_metrics.as_bytes());

            (
                StatusCode::OK,
                [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
                buffer,
            )
        }
        Err(e) => {
            tracing::error!("Failed to encode metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [("Content-Type", "text/plain")],
                b"Failed to encode metrics".to_vec(),
            )
        }
    }
}
