//! Layer factories for middleware

use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, Any, CorsLayer},
};

/// Tracing/logging middleware
///
/// With OpenTelemetry integration, we handle tracing via #[instrument] attributes
/// on the request_id_middleware instead of tower_http TraceLayer.
pub fn trace() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}

/// CORS middleware
pub fn cors(origins: &[String]) -> CorsLayer {
    if origins.is_empty() {
        // Secure default: do not emit permissive CORS headers unless explicitly configured.
        return CorsLayer::new();
    }

    let mut header_values = Vec::with_capacity(origins.len());
    for origin in origins {
        if let Ok(value) = axum::http::HeaderValue::from_str(origin) {
            header_values.push(value);
        }
    }

    // If all configured origins were invalid, fall back to no CORS.
    if header_values.is_empty() {
        return CorsLayer::new();
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(header_values))
        .allow_methods(Any)
        .allow_headers(Any)
}

/// Compression middleware
pub fn compression() -> CompressionLayer {
    CompressionLayer::new()
}
