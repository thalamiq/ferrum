//! FHIRPath ResourceResolver implementation for resolving FHIR references
//!
//! This module provides a production-ready ResourceResolver that can resolve FHIR
//! references in FHIRPath expressions. It supports:
//! - Relative references (Patient/123)
//! - Absolute references (http://server/fhir/Patient/123)
//! - Canonical references (http://hl7.org/fhir/StructureDefinition/Patient|4.0.1)
//! - Fragment references (#contained-id) - handled by FHIRPath VM
//!
//! ## Architecture
//!
//! The resolver uses a two-phase approach to handle the sync/async mismatch:
//! 1. **Pre-warm phase** (async): Extract all references, resolve them, populate cache
//! 2. **Evaluation phase** (sync): resolve() trait method returns cached results
//!
//! ## Connection Context
//!
//! The resolver supports two modes to prevent connection pool exhaustion:
//! - `ResolutionContext::Pool`: Acquires connections from pool (standalone operations)
//! - `ResolutionContext::Connection`: Reuses existing connection (within transactions)

use crate::db::search::engine::SearchEngine;
use crate::db::search::params::SearchParameters;
use crate::{Error, Result};
use lru::LruCache;
use serde_json::Value as JsonValue;
use sqlx::{PgConnection, PgPool};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ferrum_fhirpath::resolver::ResourceResolver;
use ferrum_fhirpath::Value;

/// Connection context for reference resolution
///
/// This enum allows the resolver to operate both standalone (acquiring from pool)
/// and within transactions (reusing existing connection) to prevent pool exhaustion.
pub enum ResolutionContext<'a> {
    /// Standalone mode: acquire connections from the pool
    Pool(&'a PgPool),
    /// Transaction mode: reuse the provided connection
    Connection(&'a mut PgConnection),
}

/// FHIR ResourceResolver implementation
///
/// Resolves FHIR references during FHIRPath evaluation with:
/// - Per-request LRU cache to prevent redundant DB queries
/// - Transaction-safe connection handling
/// - Optional external HTTP resolution
/// - Configurable behavior via FhirPathConfig
pub struct FhirResourceResolver {
    pool: PgPool,
    search_engine: Arc<SearchEngine>,
    base_url: Option<String>,
    cache: Mutex<LruCache<String, Option<JsonValue>>>,
    enable_external_http: bool,
    http_timeout_seconds: u64,
    http_client: tokio::sync::OnceCell<reqwest::Client>,
}

impl FhirResourceResolver {
    /// Create a new resolver instance
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `search_engine` - Shared search engine for canonical reference resolution
    /// * `base_url` - Base URL of this server (e.g., "http://localhost:8080")
    /// * `cache_size` - LRU cache size (per-request)
    /// * `enable_external_http` - Allow HTTP resolution of external URLs
    /// * `http_timeout_seconds` - HTTP request timeout
    pub fn new(
        pool: PgPool,
        search_engine: Arc<SearchEngine>,
        base_url: Option<String>,
        cache_size: usize,
        enable_external_http: bool,
        http_timeout_seconds: u64,
    ) -> Self {
        Self {
            pool,
            search_engine,
            base_url,
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(100).unwrap()),
            )),
            enable_external_http,
            http_timeout_seconds,
            http_client: tokio::sync::OnceCell::new(),
        }
    }

    /// Asynchronously resolve a reference
    ///
    /// This is the main resolution workhorse. It checks the cache first, then
    /// routes to the appropriate handler based on reference type.
    ///
    /// # Arguments
    ///
    /// * `reference` - Reference string (e.g., "Patient/123", "http://server/Patient/123")
    /// * `ctx` - Connection context (pool or transaction)
    ///
    /// # Returns
    ///
    /// * `Ok(Some(json))` - Successfully resolved reference
    /// * `Ok(None)` - Reference not found or fragment reference
    /// * `Err(_)` - Resolution error
    pub async fn resolve_async<'a>(
        &self,
        reference: &str,
        ctx: &'a mut ResolutionContext<'a>,
    ) -> Result<Option<JsonValue>> {
        // Check cache first
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(reference) {
                return Ok(cached.clone());
            }
        }

        // Parse the reference
        let parsed = self.parse_reference(reference)?;

        // Resolve based on type
        let result = match parsed {
            ParsedReference::Fragment(_id) => {
                // Fragment references (#contained-id) are handled by the FHIRPath VM
                // which has access to the containing resource's .contained array
                Ok(None)
            }
            ParsedReference::Relative {
                resource_type,
                id,
                version,
            } => {
                self.resolve_local(Some(resource_type), id, version, ctx)
                    .await
            }
            ParsedReference::Absolute {
                url,
                resource_type,
                id,
                version,
            } => {
                // Check if this is a local absolute reference
                if self.is_local_absolute(&url) {
                    self.resolve_local(resource_type, id, version, ctx).await
                } else if self.enable_external_http {
                    self.resolve_http(&url).await
                } else {
                    Ok(None)
                }
            }
            ParsedReference::Canonical { url, version } => {
                self.resolve_canonical(&url, version.as_deref(), ctx).await
            }
        };

        // Cache the result
        if let Ok(ref value) = result {
            let mut cache = self.cache.lock().unwrap();
            cache.put(reference.to_string(), value.clone());
        }

        result
    }

    /// Pre-warm the cache for all references in a resource
    ///
    /// Extracts all FHIR references from the resource JSON and resolves them
    /// asynchronously to populate the cache. This allows the sync resolve()
    /// trait method to return cached results.
    ///
    /// # Arguments
    ///
    /// * `resource` - FHIR resource as JSON
    /// * `ctx` - Connection context
    pub async fn prewarm_cache_for_resource(&self, resource: &JsonValue) -> Result<()> {
        let references = extract_all_references(resource);

        // Use pool for pre-warming to avoid mutable borrow issues
        // The cache will still be populated for later use
        for reference in references {
            let mut ctx = ResolutionContext::Pool(&self.pool);
            let _ = self.resolve_async(&reference, &mut ctx).await;
        }

        Ok(())
    }

    /// Parse a reference string into its components
    fn parse_reference(&self, reference: &str) -> Result<ParsedReference> {
        use crate::db::search::query_builder::{parse_reference_query_value, ParsedReferenceQuery};

        // Use existing parser from search engine
        let parsed = parse_reference_query_value(reference, self.base_url.as_deref())
            .ok_or_else(|| Error::InvalidReference(format!("Invalid reference: {}", reference)))?;

        match parsed {
            ParsedReferenceQuery::Fragment { id } => Ok(ParsedReference::Fragment(id)),
            ParsedReferenceQuery::Relative { typ, id, version } => Ok(ParsedReference::Relative {
                resource_type: typ.unwrap_or_default(),
                id,
                version,
            }),
            ParsedReferenceQuery::Absolute {
                url,
                typ,
                id,
                version,
                ..
            } => Ok(ParsedReference::Absolute {
                url,
                resource_type: typ,
                id: id.unwrap_or_default(),
                version,
            }),
            ParsedReferenceQuery::Canonical { url, version } => Ok(ParsedReference::Canonical {
                url,
                version: if version.is_empty() {
                    None
                } else {
                    Some(version)
                },
            }),
        }
    }

    /// Check if an absolute URL refers to this server
    fn is_local_absolute(&self, url: &str) -> bool {
        if let Some(ref base) = self.base_url {
            url.starts_with(base)
        } else {
            false
        }
    }

    /// Resolve a local reference (relative or local absolute)
    async fn resolve_local<'a>(
        &self,
        resource_type: Option<String>,
        id: String,
        version: Option<String>,
        ctx: &'a mut ResolutionContext<'a>,
    ) -> Result<Option<JsonValue>> {
        let resource_type = resource_type.ok_or_else(|| {
            Error::InvalidReference("Missing resource type in reference".to_string())
        })?;

        let query = if version.is_some() {
            "SELECT resource FROM resource_history
             WHERE resource_type = $1 AND id = $2 AND version_id = $3"
        } else {
            "SELECT resource FROM resources
             WHERE resource_type = $1 AND id = $2 AND deleted = false"
        };

        let result = match ctx {
            ResolutionContext::Pool(pool) => {
                if let Some(ref version_str) = version {
                    let version_id: i32 = version_str.parse().map_err(|_| {
                        Error::InvalidReference(format!("Invalid version: {}", version_str))
                    })?;
                    sqlx::query_scalar(query)
                        .bind(&resource_type)
                        .bind(&id)
                        .bind(version_id)
                        .fetch_optional(pool as &PgPool)
                        .await
                        .map_err(Error::Database)?
                } else {
                    sqlx::query_scalar(query)
                        .bind(&resource_type)
                        .bind(&id)
                        .fetch_optional(pool as &PgPool)
                        .await
                        .map_err(Error::Database)?
                }
            }
            ResolutionContext::Connection(conn) => {
                if let Some(ref version_str) = version {
                    let version_id: i32 = version_str.parse().map_err(|_| {
                        Error::InvalidReference(format!("Invalid version: {}", version_str))
                    })?;
                    sqlx::query_scalar(query)
                        .bind(&resource_type)
                        .bind(&id)
                        .bind(version_id)
                        .fetch_optional(&mut **conn)
                        .await
                        .map_err(Error::Database)?
                } else {
                    sqlx::query_scalar(query)
                        .bind(&resource_type)
                        .bind(&id)
                        .fetch_optional(&mut **conn)
                        .await
                        .map_err(Error::Database)?
                }
            }
        };

        Ok(result)
    }

    /// Resolve a canonical reference by searching for URL (and optionally version)
    async fn resolve_canonical<'a>(
        &self,
        url: &str,
        version: Option<&str>,
        _ctx: &'a mut ResolutionContext<'a>,
    ) -> Result<Option<JsonValue>> {
        // Build search parameters
        use std::collections::HashMap;
        let mut params_map = HashMap::new();
        params_map.insert("url".to_string(), vec![url.to_string()]);
        if let Some(v) = version {
            params_map.insert("version".to_string(), vec![v.to_string()]);
        }

        let search_params = SearchParameters::from_params(&params_map)?;

        // Execute search (system-wide, no specific resource type)
        // NOTE: This currently uses pool-based search even in transaction context
        // because SearchEngine doesn't support connection context for system-wide searches.
        // This is a known limitation documented in the plan.
        let results = self
            .search_engine
            .search(None, &search_params, self.base_url.as_deref())
            .await?;

        // Return first match
        Ok(results.resources.first().cloned())
    }

    /// Resolve an external HTTP reference
    async fn resolve_http(&self, url: &str) -> Result<Option<JsonValue>> {
        let client = self
            .http_client
            .get_or_init(|| async {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(self.http_timeout_seconds))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new())
            })
            .await;

        let response = client
            .get(url)
            .header("Accept", "application/fhir+json")
            .send()
            .await
            .map_err(|e| Error::ExternalReference(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let json = response
            .json::<JsonValue>()
            .await
            .map_err(|e| Error::ExternalReference(format!("Invalid JSON response: {}", e)))?;

        Ok(Some(json))
    }
}

/// Implement the sync ResourceResolver trait
///
/// This implementation only returns cached results. For it to work, you must
/// call prewarm_cache_for_resource() before FHIRPath evaluation.
impl ResourceResolver for FhirResourceResolver {
    fn resolve(&self, reference: &str) -> ferrum_fhirpath::Result<Option<Value>> {
        // Check cache only (no async DB access in sync context)
        let mut cache = self.cache.lock().unwrap();
        if let Some(result) = cache.get(reference) {
            return Ok(result.as_ref().map(|json| Value::from_json(json.clone())));
        }

        // Not in cache - return None
        // This means prewarm wasn't called or the reference wasn't in the resource
        Ok(None)
    }

    // Use the trait's default implementation for extract_type
    // It handles: "Patient/123" -> "Patient", "http://server/fhir/Patient/123" -> "Patient"
}

/// Parsed reference components
#[derive(Debug)]
enum ParsedReference {
    Fragment(String),
    Relative {
        resource_type: String,
        id: String,
        version: Option<String>,
    },
    Absolute {
        url: String,
        resource_type: Option<String>,
        id: String,
        version: Option<String>,
    },
    Canonical {
        url: String,
        version: Option<String>,
    },
}

/// Extract all FHIR references from a resource JSON
///
/// Recursively traverses the JSON to find all {"reference": "..."} objects,
/// excluding fragment references (#id) which are handled by the FHIRPath VM.
fn extract_all_references(value: &JsonValue) -> Vec<String> {
    let mut references = Vec::new();
    extract_references_recursive(value, &mut references);
    references
}

/// Recursive helper for reference extraction
fn extract_references_recursive(value: &JsonValue, references: &mut Vec<String>) {
    match value {
        JsonValue::Object(map) => {
            // Check if this is a Reference object
            if let Some(JsonValue::String(ref_str)) = map.get("reference") {
                // Skip fragment references (handled by VM)
                if !ref_str.starts_with('#') {
                    references.push(ref_str.clone());
                }
            }

            // Recurse into all nested objects/arrays
            for value in map.values() {
                extract_references_recursive(value, references);
            }
        }
        JsonValue::Array(arr) => {
            for item in arr {
                extract_references_recursive(item, references);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_references() {
        let json = serde_json::json!({
            "resourceType": "Observation",
            "id": "obs1",
            "subject": {
                "reference": "Patient/123"
            },
            "performer": [
                {"reference": "Practitioner/456"},
                {"reference": "#contained"}  // Should be skipped
            ],
            "basedOn": [
                {"reference": "ServiceRequest/789"}
            ]
        });

        let refs = extract_all_references(&json);
        assert_eq!(refs.len(), 3);
        assert!(refs.contains(&"Patient/123".to_string()));
        assert!(refs.contains(&"Practitioner/456".to_string()));
        assert!(refs.contains(&"ServiceRequest/789".to_string()));
        assert!(!refs.contains(&"#contained".to_string()));
    }

    #[test]
    fn test_extract_references_nested() {
        let json = serde_json::json!({
            "resourceType": "Bundle",
            "entry": [
                {
                    "resource": {
                        "resourceType": "Patient",
                        "managingOrganization": {
                            "reference": "Organization/org1"
                        }
                    }
                }
            ]
        });

        let refs = extract_all_references(&json);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "Organization/org1");
    }

    #[test]
    fn test_extract_references_empty() {
        let json = serde_json::json!({
            "resourceType": "Patient",
            "id": "123",
            "name": [{"given": ["John"]}]
        });

        let refs = extract_all_references(&json);
        assert_eq!(refs.len(), 0);
    }

    #[test]
    fn test_extract_references_only_fragments() {
        let json = serde_json::json!({
            "resourceType": "Composition",
            "section": [
                {"entry": [{"reference": "#contained1"}]},
                {"entry": [{"reference": "#contained2"}]}
            ]
        });

        let refs = extract_all_references(&json);
        assert_eq!(refs.len(), 0); // All fragments should be skipped
    }

    #[test]
    fn test_extract_references_mixed() {
        let json = serde_json::json!({
            "resourceType": "DiagnosticReport",
            "subject": {"reference": "Patient/123"},
            "result": [
                {"reference": "Observation/obs1"},
                {"reference": "#obs2"},  // Fragment - skip
                {"reference": "Observation/obs3"}
            ],
            "performer": [
                {"reference": "Practitioner/doc1"}
            ]
        });

        let refs = extract_all_references(&json);
        assert_eq!(refs.len(), 4);
        assert!(refs.contains(&"Patient/123".to_string()));
        assert!(refs.contains(&"Observation/obs1".to_string()));
        assert!(refs.contains(&"Observation/obs3".to_string()));
        assert!(refs.contains(&"Practitioner/doc1".to_string()));
        assert!(!refs.contains(&"#obs2".to_string()));
    }

    #[tokio::test]
    async fn test_parse_reference_relative() {
        let resolver = create_test_resolver(None);

        let parsed = resolver.parse_reference("Patient/123").unwrap();
        match parsed {
            ParsedReference::Relative {
                resource_type,
                id,
                version,
            } => {
                assert_eq!(resource_type, "Patient");
                assert_eq!(id, "123");
                assert_eq!(version, None);
            }
            _ => panic!("Expected Relative reference"),
        }
    }

    #[tokio::test]
    async fn test_parse_reference_relative_with_version() {
        let resolver = create_test_resolver(None);

        let parsed = resolver.parse_reference("Patient/123/_history/5").unwrap();
        match parsed {
            ParsedReference::Relative {
                resource_type,
                id,
                version,
            } => {
                assert_eq!(resource_type, "Patient");
                assert_eq!(id, "123");
                assert_eq!(version, Some("5".to_string()));
            }
            _ => panic!("Expected Relative reference"),
        }
    }

    #[tokio::test]
    async fn test_parse_reference_absolute_local() {
        let base_url = "http://localhost:8080/fhir".to_string();
        let resolver = create_test_resolver(Some(base_url.clone()));

        let parsed = resolver
            .parse_reference("http://localhost:8080/fhir/Patient/123")
            .unwrap();
        match parsed {
            ParsedReference::Absolute {
                url,
                resource_type,
                id,
                version,
            } => {
                assert!(url.contains("Patient/123"));
                assert_eq!(resource_type, Some("Patient".to_string()));
                assert_eq!(id, "123");
                assert_eq!(version, None);
            }
            _ => panic!("Expected Absolute reference"),
        }
    }

    #[tokio::test]
    async fn test_parse_reference_absolute_external() {
        let resolver = create_test_resolver(Some("http://localhost:8080".to_string()));

        let parsed = resolver
            .parse_reference("http://example.org/fhir/Patient/456")
            .unwrap();
        match parsed {
            ParsedReference::Absolute {
                url,
                resource_type,
                id,
                version,
            } => {
                assert!(url.contains("example.org"));
                assert_eq!(resource_type, Some("Patient".to_string()));
                assert_eq!(id, "456");
                assert_eq!(version, None);
            }
            _ => panic!("Expected Absolute reference"),
        }
    }

    #[tokio::test]
    async fn test_parse_reference_canonical() {
        let resolver = create_test_resolver(None);

        // Without version separator, URL-like references are parsed as Absolute, not Canonical
        let parsed = resolver
            .parse_reference("http://hl7.org/fhir/StructureDefinition/Patient")
            .unwrap();
        match parsed {
            ParsedReference::Absolute { .. } => {
                // This is expected - canonical needs version separator (|)
            }
            _ => panic!("Expected Absolute reference (canonical needs version separator)"),
        }
    }

    #[tokio::test]
    async fn test_parse_reference_canonical_with_version() {
        let resolver = create_test_resolver(None);

        let parsed = resolver
            .parse_reference("http://hl7.org/fhir/StructureDefinition/Patient|4.0.1")
            .unwrap();
        match parsed {
            ParsedReference::Canonical { url, version } => {
                assert_eq!(url, "http://hl7.org/fhir/StructureDefinition/Patient");
                assert_eq!(version, Some("4.0.1".to_string()));
            }
            _ => panic!("Expected Canonical reference"),
        }
    }

    #[tokio::test]
    async fn test_parse_reference_fragment() {
        let resolver = create_test_resolver(None);

        let parsed = resolver.parse_reference("#contained-id").unwrap();
        match parsed {
            ParsedReference::Fragment(id) => {
                assert_eq!(id, "contained-id");
            }
            _ => panic!("Expected Fragment reference"),
        }
    }

    #[tokio::test]
    async fn test_is_local_absolute() {
        let base_url = "http://localhost:8080/fhir".to_string();
        let resolver = create_test_resolver(Some(base_url));

        assert!(resolver.is_local_absolute("http://localhost:8080/fhir/Patient/123"));
        assert!(!resolver.is_local_absolute("http://example.org/fhir/Patient/123"));
        assert!(!resolver.is_local_absolute("Patient/123"));
    }

    #[tokio::test]
    async fn test_is_local_absolute_no_base() {
        let resolver = create_test_resolver(None);

        assert!(!resolver.is_local_absolute("http://localhost:8080/fhir/Patient/123"));
        assert!(!resolver.is_local_absolute("Patient/123"));
    }

    // Helper function to create a test resolver
    fn create_test_resolver(base_url: Option<String>) -> FhirResourceResolver {
        use sqlx::postgres::PgPoolOptions;
        use std::sync::Arc;

        // Create a minimal pool (won't be used in these tests)
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgresql://test:test@localhost/test")
            .unwrap();

        // Create a default search config for testing
        let search_config = crate::config::FhirSearchConfig::default();

        // Create a mock search engine (won't be used in parse tests)
        let search_engine = Arc::new(crate::db::search::engine::SearchEngine::new(
            pool.clone(),
            search_config,
        ));

        FhirResourceResolver::new(pool, search_engine, base_url, 100, false, 5)
    }

    #[tokio::test]
    async fn test_cache_behavior() {
        let resolver = create_test_resolver(None);

        // First access should populate cache
        let reference = "Patient/123";
        {
            let mut cache = resolver.cache.lock().unwrap();
            assert!(cache.get(reference).is_none());
        }

        // Manually populate cache
        {
            let mut cache = resolver.cache.lock().unwrap();
            cache.put(
                reference.to_string(),
                Some(serde_json::json!({"resourceType": "Patient", "id": "123"})),
            );
        }

        // Second access should hit cache
        {
            let cache = resolver.cache.lock().unwrap();
            assert!(cache.peek(reference).is_some());
        }
    }

    #[tokio::test]
    async fn test_extract_type_default_implementation() {
        let resolver = create_test_resolver(None);

        // Test relative references
        assert_eq!(resolver.extract_type("Patient/123"), Some("Patient"));
        assert_eq!(
            resolver.extract_type("Observation/obs1"),
            Some("Observation")
        );

        // Test absolute URLs
        assert_eq!(
            resolver.extract_type("http://example.org/fhir/Patient/123"),
            Some("Patient")
        );
        assert_eq!(
            resolver.extract_type("https://server.com/fhir/Observation/obs1"),
            Some("Observation")
        );

        // Test fragment (should return None)
        assert_eq!(resolver.extract_type("#contained"), None);

        // Test invalid/single segment (should return None - no slash)
        assert_eq!(resolver.extract_type(""), None);
        assert_eq!(resolver.extract_type("invalid"), None);
    }
}
