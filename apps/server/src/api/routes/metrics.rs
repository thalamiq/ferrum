//! Metrics API Routes
//!
//! Exposes Prometheus-compatible metrics endpoint for monitoring

use crate::api::handlers::metrics;
use crate::state::AppState;
use axum::{routing::get, Router};

pub fn metrics_routes() -> Router<AppState> {
    Router::new().route("/metrics", get(metrics::metrics_handler))
}
