//! Conformance resource access for `fhir-context`.

use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use std::{collections::HashMap, sync::OnceLock};
use zunder_context::{
    ConformanceResourceProvider, DefaultFhirContext, Error as ContextError,
    FallbackConformanceProvider, FhirContext, FlexibleFhirContext, Result as ContextResult,
};
use zunder_package::FhirPackage;
use zunder_registry_client::{FileSystemCache, PackageCache, RegistryClient};

use crate::db::PostgresResourceStore;
use crate::Result;

pub struct DbConformanceProvider {
    store: PostgresResourceStore,
}

impl DbConformanceProvider {
    pub fn new(pool: PgPool) -> Self {
        Self {
            store: PostgresResourceStore::new(pool),
        }
    }
}

pub fn db_backed_fhir_context(pool: PgPool) -> Result<Arc<dyn FhirContext>> {
    let provider: Arc<dyn ConformanceResourceProvider> = Arc::new(DbConformanceProvider::new(pool));
    let ctx =
        FlexibleFhirContext::new(provider).map_err(|e| crate::Error::FhirContext(e.to_string()))?;
    Ok(Arc::new(ctx))
}

pub fn db_backed_fhir_context_with_fallback(
    pool: PgPool,
    fallback: Arc<dyn ConformanceResourceProvider>,
) -> Result<Arc<dyn FhirContext>> {
    let db: Arc<dyn ConformanceResourceProvider> = Arc::new(DbConformanceProvider::new(pool));
    let provider: Arc<dyn ConformanceResourceProvider> =
        Arc::new(FallbackConformanceProvider::new(db, fallback));
    let ctx =
        FlexibleFhirContext::new(provider).map_err(|e| crate::Error::FhirContext(e.to_string()))?;
    Ok(Arc::new(ctx))
}

/// Create an empty FhirContext that doesn't query the database.
///
/// Used for FHIRPath evaluation during indexing to avoid sync/async deadlocks.
/// The TypePass will not be able to resolve types, but FHIRPath evaluation will
/// still work correctly (it will just use dynamic typing at runtime).
pub fn empty_fhir_context() -> Result<Arc<dyn FhirContext>> {
    let provider: Arc<dyn ConformanceResourceProvider> = Arc::new(EmptyConformanceProvider);
    let ctx =
        FlexibleFhirContext::new(provider).map_err(|e| crate::Error::FhirContext(e.to_string()))?;
    Ok(Arc::new(ctx))
}

/// Ensure the core FHIR package is cached, downloading if necessary.
///
/// This should be called during application startup to ensure the package is available
/// before any services attempt to load it. Downloads from Simplifier registry if not
/// already cached locally.
///
/// This function is optimized to avoid loading the package into RAM - it only checks
/// if the package directory exists and downloads if needed.
///
/// # Arguments
///
/// * `fhir_version` - FHIR version string (e.g., "R4", "R4B", "R5")
///
/// # Errors
///
/// Returns an error if:
/// - The FHIR version is unsupported
/// - Download from registry fails
/// - Package storage fails
pub async fn ensure_core_package_cached(fhir_version: &str) -> Result<()> {
    let (core_name, core_version) = match fhir_version {
        "R4" => ("hl7.fhir.r4.core", "4.0.1"),
        "R4B" => ("hl7.fhir.r4b.core", "4.3.0"),
        "R5" => ("hl7.fhir.r5.core", "5.0.0"),
        other => {
            return Err(crate::Error::Internal(format!(
                "Unsupported FHIR version: {}",
                other
            )));
        }
    };

    // Use spawn_blocking to avoid blocking async runtime for file system operations
    let core_name_clone = core_name.to_string();
    let core_version_clone = core_version.to_string();

    let is_cached = tokio::task::spawn_blocking(move || {
        let cache = FileSystemCache::new(None);
        cache.has_package(&core_name_clone, &core_version_clone)
    })
    .await
    .map_err(|e| crate::Error::Internal(format!("Cache check task failed: {}", e)))?;

    if is_cached {
        tracing::debug!(
            package = core_name,
            version = core_version,
            "Core FHIR package already cached"
        );
        return Ok(());
    }

    tracing::info!(
        package = core_name,
        version = core_version,
        "Core FHIR package not found in cache, downloading..."
    );

    // Download and store the package (RegistryClient handles storage)
    let client = RegistryClient::new(None);
    client
        .load_or_download_package(core_name, core_version)
        .await
        .map_err(|e| {
            crate::Error::FhirContext(format!(
                "Failed to download core package {}#{}: {}",
                core_name, core_version, e
            ))
        })?;

    tracing::info!(
        package = core_name,
        version = core_version,
        "Core FHIR package successfully cached"
    );

    Ok(())
}

/// Create an in-memory FhirContext for the core FHIR package (no DB access).
///
/// This is used by services that must avoid database lookups during execution (e.g. indexing),
/// but still require core StructureDefinitions for FHIRPath type operations like `ofType(uri)`.
///
/// **Important**: Call `ensure_core_package_cached()` during startup before using this function
/// to ensure the package is available in the cache.
pub fn cached_core_fhir_context(fhir_version: &str) -> Result<Arc<dyn FhirContext>> {
    static CORE_CTX: OnceLock<std::sync::Mutex<HashMap<String, Arc<dyn FhirContext>>>> =
        OnceLock::new();

    let cache_map = CORE_CTX.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Some(ctx) = cache_map.lock().unwrap().get(fhir_version).cloned() {
        return Ok(ctx);
    }

    let (core_name, core_version) = match fhir_version {
        "R4" => ("hl7.fhir.r4.core", "4.0.1"),
        "R4B" => ("hl7.fhir.r4b.core", "4.3.0"),
        "R5" => ("hl7.fhir.r5.core", "5.0.0"),
        other => {
            return Err(crate::Error::Internal(format!(
                "Unsupported FHIR version: {}",
                other
            )));
        }
    };

    let cache = FileSystemCache::new(None);
    let package_path = cache
        .cache_root()
        .join(format!("{}#{}", core_name, core_version))
        .join("package");
    let package = FhirPackage::from_directory(&package_path).map_err(|e| {
        crate::Error::FhirContext(format!(
            "Failed to load core package {}#{} from cache {}: {}",
            core_name,
            core_version,
            package_path.display(),
            e
        ))
    })?;

    let ctx: Arc<dyn FhirContext> = Arc::new(DefaultFhirContext::new(package));
    cache_map
        .lock()
        .unwrap()
        .insert(fhir_version.to_string(), ctx.clone());

    Ok(ctx)
}

/// Empty conformance provider that returns no resources.
/// Used to create a FhirContext that doesn't make database calls.
struct EmptyConformanceProvider;

#[async_trait]
impl ConformanceResourceProvider for EmptyConformanceProvider {
    async fn list_by_canonical(&self, _canonical_url: &str) -> ContextResult<Vec<Arc<Value>>> {
        Ok(Vec::new())
    }

    async fn get_by_canonical_and_version(
        &self,
        _canonical_url: &str,
        _version: &str,
    ) -> ContextResult<Option<Arc<Value>>> {
        Ok(None)
    }
}

#[async_trait]
impl ConformanceResourceProvider for DbConformanceProvider {
    async fn list_by_canonical(&self, canonical_url: &str) -> ContextResult<Vec<Arc<Value>>> {
        let resources = self
            .store
            .list_current_by_canonical_url(canonical_url)
            .await
            .map_err(|e| ContextError::ConformanceStore(e.to_string()))?;

        Ok(resources.into_iter().map(Arc::new).collect())
    }

    async fn get_by_canonical_and_version(
        &self,
        canonical_url: &str,
        version: &str,
    ) -> ContextResult<Option<Arc<Value>>> {
        let resource = self
            .store
            .get_by_canonical_url_and_version(canonical_url, version)
            .await
            .map_err(|e| ContextError::ConformanceStore(e.to_string()))?;

        Ok(resource.map(Arc::new))
    }
}

/// Check if a resource type is a conformance resource that should trigger hooks
/// when created or updated in batch/transaction operations.
///
/// These resource types require special processing (e.g., updating search indexes,
/// rebuilding compartment memberships, etc.) when installed from packages.
///
/// # Examples
///
/// ```
/// use zunder::conformance::is_conformance_resource_type;
///
/// assert!(is_conformance_resource_type("SearchParameter"));
/// assert!(is_conformance_resource_type("CompartmentDefinition"));
/// assert!(!is_conformance_resource_type("Patient"));
/// ```
pub fn is_conformance_resource_type(resource_type: &str) -> bool {
    matches!(
        resource_type,
        "SearchParameter"
            | "StructureDefinition"
            | "CodeSystem"
            | "ValueSet"
            | "CompartmentDefinition"
    )
}
