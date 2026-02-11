//! Search implementation - query building and execution
//!
//! The SearchEngine is responsible for:
//! - Building SQL queries from FHIR search parameters
//! - Executing searches against the database
//! - Handling _include and _revinclude
//! - Managing pagination and result limits

use crate::db::search::{params, query_builder};
use crate::runtime_config::RuntimeConfigCache;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::sync::Arc;

pub use params::SearchParameters;
pub use query_builder::QueryBuilder;

mod api;
mod compartments;
mod execute;
mod filter;
mod includes;
mod normalize;
mod resolve;
mod sort;
mod util;

/// Search engine executes FHIR searches against the database
pub struct SearchEngine {
    db_pool: PgPool,
    param_cache: Arc<crate::db::search::parameter_lookup::SearchParamCache>,
    computed_hooks: crate::hooks::computed::HookRegistry,
    enable_text_search: bool,
    enable_content_search: bool,
    runtime_config_cache: Option<Arc<RuntimeConfigCache>>,
    search_config: crate::config::FhirSearchConfig,
}
