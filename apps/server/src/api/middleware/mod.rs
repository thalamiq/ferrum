//! Middleware stack for the API

pub mod audit;
pub mod layers;
pub mod metrics;
pub mod request_id;
pub mod security;

// Re-export public API
pub use audit::audit_middleware;
pub use layers::{compression, cors, trace};
pub use metrics::metrics_middleware;
pub use request_id::request_id_middleware;
pub use security::security_headers_middleware;
