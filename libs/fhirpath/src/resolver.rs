//! Resource resolver trait for custom resolve() implementations
//!
//! This module defines the ResourceResolver trait that allows consumers of the
//! FHIRPath engine to provide custom implementations for resolving references.
//!
//! The primary use case is to enable database-backed resolution of FHIR resource
//! references while maintaining high performance through optimizations like
//! short-circuit evaluation for type-only checks.

use crate::error::Result;
use crate::value::Value;

/// Trait for custom resource resolution
///
/// Implement this trait to provide custom resolution logic for the FHIRPath
/// `resolve()` function. This is particularly useful for resolving external
/// references that require database lookups or API calls.
///
/// # Short-circuit optimization for type checks
///
/// For performance, implementations should handle type-only checks efficiently.
/// When the resolved resource will only be used for type checking (e.g.,
/// `resolve().is(Patient)`), implementations can avoid full resolution by
/// returning a lightweight type indicator or using the reference string itself.
///
/// # Example
///
/// ```rust,ignore
/// use fhirpath_engine::resolver::ResourceResolver;
/// use fhirpath_engine::value::Value;
/// use fhirpath_engine::error::Result;
///
/// struct DatabaseResolver {
///     // database connection pool, etc.
/// }
///
/// impl ResourceResolver for DatabaseResolver {
///     fn resolve(&self, reference: &str) -> Result<Option<Value>> {
///         // Parse reference: "Patient/123" -> (Patient, 123)
///         let (resource_type, resource_id) = parse_reference(reference)?;
///
///         // Query database
///         let resource_json = query_database(resource_type, resource_id)?;
///
///         if let Some(json) = resource_json {
///             Ok(Some(Value::from_json(json)))
///         } else {
///             Ok(None)
///         }
///     }
/// }
/// ```
pub trait ResourceResolver: Send + Sync {
    /// Resolve a reference string to a resource value
    ///
    /// # Arguments
    ///
    /// * `reference` - Reference string to resolve (e.g., "Patient/123", "#contained-id")
    ///
    /// # Returns
    ///
    /// * `Ok(Some(value))` - Successfully resolved reference
    /// * `Ok(None)` - Reference not found (valid but non-existent)
    /// * `Err(_)` - Error during resolution (invalid reference, database error, etc.)
    ///
    /// # Performance considerations
    ///
    /// This method may be called frequently during FHIRPath evaluation. Consider:
    /// - Implementing caching to avoid repeated database queries
    /// - Short-circuit evaluation for type-only checks
    /// - Batch resolution when possible
    fn resolve(&self, reference: &str) -> Result<Option<Value>>;

    /// Extract resource type from a reference without full resolution (optional optimization)
    ///
    /// This method is called for type-checking operations like `resolve().is(Patient)`.
    /// The default implementation extracts the type from the reference string itself
    /// without calling the full resolve() method, which is much more efficient for
    /// type-only checks.
    ///
    /// Override this if your reference format is non-standard or if you need custom logic.
    ///
    /// # Arguments
    ///
    /// * `reference` - Reference string (e.g., "Patient/123", "#contained-id")
    ///
    /// # Returns
    ///
    /// * `Some(type_name)` - Resource type extracted from reference
    /// * `None` - Could not determine type from reference
    fn extract_type<'a>(&self, reference: &'a str) -> Option<&'a str> {
        // Default implementation: parse standard FHIR reference formats
        // Supports:
        // - "Patient/123" -> "Patient"
        // - "https://server.com/fhir/Patient/123" -> "Patient"
        // - Does not support contained references ("#id") - these return None

        if reference.starts_with('#') {
            // Contained references don't have type information in the reference string
            return None;
        }

        if reference.starts_with("http://") || reference.starts_with("https://") {
            // Absolute URL: extract type from path
            // "https://server.com/fhir/Patient/123" -> "Patient"
            let parts: Vec<&str> = reference.rsplitn(3, '/').collect();
            if parts.len() >= 2 {
                return Some(parts[1]);
            }
        } else {
            // Relative reference: "Patient/123" -> "Patient"
            if let Some(slash_idx) = reference.find('/') {
                return Some(&reference[..slash_idx]);
            }
        }

        None
    }
}
