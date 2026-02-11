//! Request handlers for API endpoints
//!
//! Handlers coordinate between routes and services, handling:
//! - Request extraction and validation
//! - Service invocation
//! - Response formatting
//! - Error handling

pub mod admin;
pub mod batch;
pub mod crud;
pub mod jobs;
pub mod metadata;
pub mod metrics;
pub mod operations;
pub mod packages;
pub mod runtime_config;
pub mod search;
pub mod smart;

pub use admin::*;
pub use batch::*;
pub use crud::*;
pub use jobs::*;
pub use metadata::*;
pub use metrics::*;
pub use operations::*;
pub use packages::*;
pub use runtime_config::*;
pub use search::*;
pub use smart::*;
