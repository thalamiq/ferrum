//! API layer - routes, handlers, and middleware

pub mod content_negotiation;
pub mod extractors;
pub(crate) mod fhir_access;
pub mod handlers;
pub mod headers;
pub mod middleware;
pub mod resource_formatter;
pub mod routes;
pub mod url;

use crate::state::AppState;
use axum::{
    extract::DefaultBodyLimit,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::json;

/// Create the main application router
pub fn create_router(state: AppState) -> Router {
    // Get request body size limit from config
    let max_body_size = state.config.server.max_request_body_size;
    let cors_origins = state.config.server.cors_origins.clone();
    let fhir_auth_state = state.clone();
    let fhir_audit_state = state.clone();
    let admin_auth_state = state.clone();

    let fhir_router = routes::fhir::fhir_routes()
        .layer(axum::middleware::from_fn_with_state(
            fhir_audit_state,
            middleware::audit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            fhir_auth_state,
            crate::auth::auth_middleware,
        ));
    let admin_router = routes::admin::admin_routes().layer(axum::middleware::from_fn_with_state(
        admin_auth_state,
        crate::admin_auth::admin_middleware,
    ));

    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Root endpoint
        .route("/", get(root))
        // Favicon handler (returns 204 to prevent 404 logs)
        .route("/favicon.ico", get(favicon))
        // Metrics endpoint
        .merge(routes::metrics::metrics_routes())
        // FHIR API routes (to be implemented)
        .nest("/fhir", fhir_router)
        // Internal admin routes (to be implemented)
        .nest("/admin", admin_router)
        // Add state
        .with_state(state)
        // Add middleware (applied in reverse order)
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::request_id_middleware))
        .layer(axum::middleware::from_fn(middleware::metrics_middleware))
        .layer(middleware::compression())
        .layer(middleware::cors(&cors_origins))
        .layer(middleware::trace())
        // Limit request body size to prevent DoS via large payloads
        .layer(DefaultBodyLimit::max(max_body_size))
}

async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "fhir-server"
    }))
}

async fn root(State(state): State<AppState>) -> impl IntoResponse {
    // Note: This is an informational endpoint (not a FHIR interaction).
    (
        StatusCode::OK,
        Json(json!({
            "server": "FHIR Server (Rust)",
            "version": env!("CARGO_PKG_VERSION"),
            "fhirVersion": state.config.fhir.version,
            "status": "running"
        })),
    )
}

async fn favicon() -> impl IntoResponse {
    // Return 204 No Content to indicate no favicon is available
    // This prevents 404 errors from cluttering logs
    StatusCode::NO_CONTENT
}
