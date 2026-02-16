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
    response::{IntoResponse, Json, Redirect},
    routing::get,
    Router,
};
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

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

    let mut router = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Root endpoint — redirect to UI if serving static files, otherwise JSON info
        .route("/", get(root_redirect))
        // Favicon handler (returns 204 to prevent 404 logs)
        .route("/favicon.ico", get(favicon))
        // Metrics endpoint
        .merge(routes::metrics::metrics_routes())
        // FHIR API routes
        .nest("/fhir", fhir_router)
        // Internal admin routes
        .nest("/admin", admin_router);

    // Serve admin UI static files at /ui/ if a static_dir is configured
    if let Some(ref static_dir) = state.config.ui.static_dir {
        let index_path = format!("{}/index.html", static_dir);
        let ui_service = ServeDir::new(static_dir)
            .not_found_service(ServeFile::new(&index_path));
        router = router.nest_service("/ui", ui_service);
        tracing::info!(path = %static_dir, "Serving admin UI at /ui/");
    }

    router
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

async fn root_redirect(State(state): State<AppState>) -> impl IntoResponse {
    // If UI static files are configured, redirect to the UI
    if state.config.ui.static_dir.is_some() {
        Redirect::temporary("/ui/").into_response()
    } else {
        Json(json!({
            "server": "FHIR Server (Rust)",
            "version": env!("CARGO_PKG_VERSION"),
            "fhirVersion": state.config.fhir.version,
            "status": "running"
        }))
        .into_response()
    }
}

async fn favicon() -> impl IntoResponse {
    // Return 204 No Content to indicate no favicon is available
    // This prevents 404 errors from cluttering logs
    StatusCode::NO_CONTENT
}
