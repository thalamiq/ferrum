//! FHIR Server - Rust implementation
//!
//! A production-ready FHIR R4/R5 server with:
//! - Full CRUD operations with versioning
//! - Advanced search with indexed parameters
//! - Background job processing for indexing
//! - Batch/Transaction support
//! - Package registry integration

// Allow clippy lints that are acceptable for this domain-specific codebase
#![allow(
    clippy::too_many_arguments,      // Functions with many args are acceptable for domain operations
    clippy::type_complexity,         // Complex types are acceptable when they represent domain concepts
    clippy::large_enum_variant,      // Large enum variants acceptable; boxing may impact performance
    clippy::question_mark,           // let-else vs ? operator is a style preference
    clippy::vec_init_then_push,      // Vec initialization patterns are acceptable
)]

pub mod admin_auth;
pub mod api;
pub mod auth;
pub mod background;
pub mod config;
pub mod conformance;
pub mod db;
pub mod error;
pub mod hooks;
pub mod logging;
pub mod metrics;
pub mod models;
pub mod queue;
pub mod request_context;
pub mod runtime_config;
pub mod services;
pub mod startup;
pub mod state;
pub mod workers;

pub use config::Config;
pub use error::{Error, Result};
pub use state::AppState;
