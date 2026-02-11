//! Computed search parameter hooks
//!
//! Handle parameters that require special runtime logic beyond standard FHIRPath.
//! Each computed parameter has two hooks:
//! - Index hook: Extract and index values when resources are created/updated
//! - Query hook: Transform query parameters at search time

use crate::db::search::query_builder::ResolvedParam;
use crate::models::Resource;
use crate::services::indexing::SearchParameter;
use crate::Result;
use zunder_fhirpath::Context;

mod age;

/// Hook for indexing a computed parameter
#[async_trait::async_trait]
pub(crate) trait IndexHook: Send + Sync {
    /// Resource type this hook applies to (e.g., "Patient")
    fn resource_type(&self) -> &'static str;

    /// Parameter code this hook handles (e.g., "age")
    fn parameter_code(&self) -> &'static str;

    /// Extract and index the parameter value
    async fn index(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param: &SearchParameter,
        ctx: &Context,
        fhirpath_engine: &zunder_fhirpath::Engine,
    ) -> Result<()>;
}

/// Hook for transforming query parameters
pub(crate) trait QueryHook: Send + Sync {
    /// Resource type this hook applies to (e.g., "Patient")
    fn resource_type(&self) -> &'static str;

    /// Parameter code this hook handles (e.g., "age")
    fn parameter_code(&self) -> &'static str;

    /// Transform search values into resolved parameters
    fn transform(
        &self,
        values: &[crate::db::search::query_builder::SearchValue],
    ) -> Option<Vec<ResolvedParam>>;
}

/// Global registry of computed parameter hooks
pub(crate) struct HookRegistry {
    index_hooks: Vec<Box<dyn IndexHook>>,
    query_hooks: Vec<Box<dyn QueryHook>>,
}

impl HookRegistry {
    /// Create registry with all known hooks
    pub fn new() -> Self {
        Self {
            index_hooks: vec![Box::new(age::AgeIndexHook)],
            query_hooks: vec![Box::new(age::AgeQueryHook)],
        }
    }

    /// Find index hook for a parameter
    pub fn find_index_hook(&self, resource_type: &str, param_code: &str) -> Option<&dyn IndexHook> {
        self.index_hooks
            .iter()
            .find(|h| h.resource_type() == resource_type && h.parameter_code() == param_code)
            .map(|b| b.as_ref())
    }

    /// Find query hook for a parameter
    pub fn find_query_hook(&self, resource_type: &str, param_code: &str) -> Option<&dyn QueryHook> {
        self.query_hooks
            .iter()
            .find(|h| h.resource_type() == resource_type && h.parameter_code() == param_code)
            .map(|b| b.as_ref())
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
