//! Package service - loading and storing FHIR packages with production-ready error tracking.

use crate::{
    db::packages::{PackageRecord, PackageRepository, PackageResourceRecord},
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as JsonValue};
use std::collections::HashMap;
use ferrum_registry_client::FhirPackage;

use super::{BatchService, CrudService};
use ferrum_models::{Bundle, BundleEntry, BundleEntryRequest, BundleType};

/// Categories of errors that can occur during package operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    MissingRequiredFields,
    ValidationError,
    StorageError,
    LinkageError,
    InvalidJson,
    ConflictError,
    Unknown,
}

impl ErrorCategory {
    pub fn description(&self) -> &'static str {
        match self {
            Self::MissingRequiredFields => "Missing required fields",
            Self::ValidationError => "FHIR validation failed",
            Self::StorageError => "Database storage failed",
            Self::LinkageError => "Package linkage failed",
            Self::InvalidJson => "Invalid JSON structure",
            Self::ConflictError => "Resource conflict",
            Self::Unknown => "Unknown error",
        }
    }

    pub fn from_error_message(error: &str) -> Self {
        let error_lower = error.to_lowercase();

        if error_lower.contains("missing")
            && (error_lower.contains("resourcetype") || error_lower.contains("id"))
        {
            Self::MissingRequiredFields
        } else if error_lower.contains("validation") || error_lower.contains("invalid") {
            Self::ValidationError
        } else if error_lower.contains("storage") || error_lower.contains("database") {
            Self::StorageError
        } else if error_lower.contains("link") || error_lower.contains("foreign key") {
            Self::LinkageError
        } else if error_lower.contains("json") || error_lower.contains("parse") {
            Self::InvalidJson
        } else if error_lower.contains("conflict") || error_lower.contains("duplicate") {
            Self::ConflictError
        } else {
            Self::Unknown
        }
    }
}

/// Detailed information about a single resource failure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFailure {
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub error_message: String,
    pub category: ErrorCategory,
}

impl ResourceFailure {
    pub fn new(
        resource_type: Option<String>,
        resource_id: Option<String>,
        error_message: String,
    ) -> Self {
        let category = ErrorCategory::from_error_message(&error_message);
        Self {
            resource_type,
            resource_id,
            error_message,
            category,
        }
    }

    pub fn missing_resource_type() -> Self {
        Self::new(
            None,
            None,
            "Resource missing resourceType field".to_string(),
        )
    }

    pub fn missing_id(resource_type: String) -> Self {
        Self::new(
            Some(resource_type.clone()),
            None,
            format!("Resource {} missing id field", resource_type),
        )
    }
}

/// Aggregated error summary for package installation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorSummary {
    pub total_failures: usize,
    pub by_category: std::collections::HashMap<ErrorCategory, usize>,
    pub by_resource_type: std::collections::HashMap<String, usize>,
    pub sample_failures: Vec<ResourceFailure>,
}

impl ErrorSummary {
    pub fn new() -> Self {
        Self {
            total_failures: 0,
            by_category: std::collections::HashMap::new(),
            by_resource_type: std::collections::HashMap::new(),
            sample_failures: Vec::new(),
        }
    }

    pub fn add_failure(&mut self, failure: ResourceFailure) {
        self.total_failures += 1;

        *self.by_category.entry(failure.category).or_insert(0) += 1;

        if let Some(resource_type) = &failure.resource_type {
            *self
                .by_resource_type
                .entry(resource_type.clone())
                .or_insert(0) += 1;
        }

        const MAX_SAMPLES: usize = 20;
        if self.sample_failures.len() < MAX_SAMPLES {
            self.sample_failures.push(failure);
        }
    }

    pub fn to_error_message(&self) -> String {
        if self.total_failures == 0 {
            return String::new();
        }

        let mut parts = Vec::new();

        if !self.by_category.is_empty() {
            let mut category_parts: Vec<String> = self
                .by_category
                .iter()
                .map(|(cat, count)| format!("{} {} error(s)", count, cat.description()))
                .collect();
            category_parts.sort();
            parts.push(format!("Failures: {}", category_parts.join(", ")));
        }

        if !self.by_resource_type.is_empty() {
            let mut type_parts: Vec<String> = self
                .by_resource_type
                .iter()
                .map(|(rt, count)| format!("{} {}", count, rt))
                .collect();
            type_parts.sort();
            parts.push(format!("Affected types: {}", type_parts.join(", ")));
        }

        parts.join(". ")
    }

    pub fn has_failures(&self) -> bool {
        self.total_failures > 0
    }
}

impl Default for ErrorSummary {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of a package installation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInstallOutcome {
    pub package_id: i32,
    pub name: String,
    pub version: String,
    pub already_loaded: bool,
    pub attempted_resources: usize,
    pub stored_resources: usize,
    pub linked_resources: usize,
    pub failed_resources: usize,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<ErrorSummary>,
}

impl PackageInstallOutcome {
    pub fn is_success(&self) -> bool {
        self.status == "loaded" && self.failed_resources == 0
    }

    pub fn is_failure(&self) -> bool {
        self.status == "failed" || (self.stored_resources == 0 && self.failed_resources > 0)
    }

    pub fn is_partial(&self) -> bool {
        self.status == "partial"
    }
}

pub struct PackageService {
    repo: PackageRepository,
    #[allow(dead_code)]
    crud: Option<CrudService>,
    batch: Option<BatchService>,
}

impl PackageService {
    pub fn new(repo: PackageRepository, crud: CrudService, batch: BatchService) -> Self {
        Self {
            repo,
            crud: Some(crud),
            batch: Some(batch),
        }
    }

    pub fn new_admin(repo: PackageRepository) -> Self {
        Self {
            repo,
            crud: None,
            batch: None,
        }
    }

    pub async fn list_packages(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<PackageRecord>, i64)> {
        self.repo.list_packages(status, limit, offset).await
    }

    pub async fn get_package(&self, package_id: i32) -> Result<Option<PackageRecord>> {
        self.repo.get_package(package_id).await
    }

    pub async fn list_package_resources(
        &self,
        package_id: i32,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<(Vec<PackageResourceRecord>, i64)> {
        if self.repo.get_package(package_id).await?.is_none() {
            return Err(crate::Error::ResourceNotFound {
                resource_type: "Package".to_string(),
                id: package_id.to_string(),
            });
        }

        self.repo
            .list_package_resources(package_id, limit, offset)
            .await
    }

    /// Install a package and all its resources into the database using batch processing.
    ///
    /// Uses PUT semantics (create-via-update) to preserve package resource IDs.
    pub async fn install_package(
        &self,
        package: &FhirPackage,
        install_examples: bool,
        filter: &crate::config::ResourceTypeFilter,
    ) -> Result<PackageInstallOutcome> {
        let batch = self.batch.as_ref().ok_or_else(|| {
            crate::Error::Internal("PackageService not configured for installation".to_string())
        })?;

        let name = package.manifest.name.clone();
        let version = package.manifest.version.clone();

        tracing::info!("Starting package installation: {}#{}", name, version);

        if let Some(existing_id) = self
            .repo
            .get_existing_loaded_package_id(&name, &version)
            .await?
        {
            return Ok(PackageInstallOutcome {
                package_id: existing_id,
                name,
                version,
                already_loaded: true,
                attempted_resources: 0,
                stored_resources: 0,
                linked_resources: 0,
                failed_resources: 0,
                status: "loaded".to_string(),
                error_message: None,
                error_summary: None,
            });
        }

        let package_id = self.repo.mark_package_loading(&name, &version).await?;

        let source = format!("package:{}#{}", name, version);

        let mut stored_resources = 0usize;
        let mut linked_resources = 0usize;
        let mut error_summary = ErrorSummary::new();

        // Step 1: Collect conformance resources, optionally adding examples
        let mut all_resources: Vec<&JsonValue> = package.conformance_resources().iter().collect();
        if install_examples {
            all_resources.extend(package.example_resources().iter());
        }

        // Step 2: Apply resource type filtering
        let resources: Vec<&JsonValue> = if filter.is_active() {
            all_resources
                .into_iter()
                .filter(|resource| {
                    resource
                        .get("resourceType")
                        .and_then(|v| v.as_str())
                        .map(|rt| {
                            let should_include = filter.should_include(rt);
                            if !should_include {
                                tracing::debug!(
                                    "Filtering out {} resource from package {}#{} (resourceType filter)",
                                    rt,
                                    name,
                                    version
                                );
                            }
                            should_include
                        })
                        .unwrap_or(false)
                })
                .collect()
        } else {
            all_resources
        };

        let total_resources_before_filter = package.conformance_resources().len()
            + if install_examples {
                package.example_resources().len()
            } else {
                0
            };
        let filtered_count = total_resources_before_filter - resources.len();

        if filtered_count > 0 {
            tracing::info!(
                "Filtered {} resources by resourceType from {}#{} (keeping {} of {})",
                filtered_count,
                name,
                version,
                resources.len(),
                total_resources_before_filter
            );
        }

        let attempted_resources = resources.len();

        let mut entries = Vec::with_capacity(resources.len());
        let mut entry_identities: Vec<(String, String)> = Vec::with_capacity(resources.len());
        let mut skip_count = 0usize;

        for resource in &resources {
            let resource_type = match resource.get("resourceType").and_then(|v| v.as_str()) {
                Some(rt) => rt,
                None => {
                    error_summary.add_failure(ResourceFailure::missing_resource_type());
                    skip_count += 1;
                    continue;
                }
            };

            let id = match resource.get("id").and_then(|v| v.as_str()) {
                Some(id) => id,
                None => {
                    error_summary
                        .add_failure(ResourceFailure::missing_id(resource_type.to_string()));
                    skip_count += 1;
                    continue;
                }
            };

            entry_identities.push((resource_type.to_string(), id.to_string()));

            let annotated = annotate_meta_source((*resource).clone(), &source);

            entries.push(BundleEntry {
                full_url: Some(format!("urn:uuid:{}", uuid::Uuid::new_v4())),
                resource: Some(annotated),
                request: Some(BundleEntryRequest {
                    method: "PUT".to_string(),
                    url: format!("{}/{}", resource_type, id),
                    if_match: None,
                    if_none_match: None,
                    if_modified_since: None,
                    if_none_exist: None,
                    extensions: HashMap::new(),
                }),
                response: None,
                search: None,
                extensions: HashMap::new(),
            });
        }

        let bundle = Bundle {
            resource_type: "Bundle".to_string(),
            id: Some(uuid::Uuid::new_v4().to_string()),
            bundle_type: BundleType::Batch,
            timestamp: None,
            total: None,
            link: None,
            entry: Some(entries),
            signature: None,
            extensions: HashMap::new(),
        };

        let bundle_json = serde_json::to_value(bundle)
            .map_err(|e| crate::Error::Internal(format!("Failed to serialize bundle: {}", e)))?;

        let response_json = batch.process_bundle(bundle_json).await?;
        let response_bundle: Bundle = serde_json::from_value(response_json).map_err(|e| {
            crate::Error::Internal(format!("Failed to parse response bundle: {}", e))
        })?;

        if let Some(response_entries) = response_bundle.entry {
            for (idx, entry) in response_entries.into_iter().enumerate() {
                let Some(response) = entry.response else {
                    continue;
                };
                let status = response.status.as_str();

                let (rt, id) = entry_identities
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| ("(unknown)".to_string(), "(unknown)".to_string()));

                if status.starts_with('2') {
                    stored_resources += 1;

                    let version_id = response
                        .etag
                        .as_deref()
                        .and_then(parse_etag)
                        .or_else(|| response.location.as_deref().and_then(parse_history_version))
                        .unwrap_or(1);

                    match self
                        .repo
                        .link_resource_to_package(package_id, &rt, &id, version_id)
                        .await
                    {
                        Ok(()) => linked_resources += 1,
                        Err(e) => {
                            error_summary.add_failure(ResourceFailure::new(
                                Some(rt),
                                Some(id),
                                format!("Failed to link to package: {}", e),
                            ));
                        }
                    }
                } else {
                    error_summary.add_failure(ResourceFailure::new(
                        Some(rt),
                        Some(id),
                        format!("HTTP {}", status),
                    ));
                }
            }
        }

        let failed_resources = error_summary.total_failures + skip_count;

        let status = if stored_resources == 0 && failed_resources > 0 {
            "failed"
        } else if failed_resources > 0 {
            "partial"
        } else {
            "loaded"
        };

        let error_message = match status {
            "loaded" => None,
            "failed" => Some(format!(
                "Package installation failed: all {} resources failed. {}",
                attempted_resources,
                error_summary.to_error_message()
            )),
            "partial" => Some(format!(
                "Package installed with {} failures out of {} resources. {}",
                failed_resources,
                attempted_resources,
                error_summary.to_error_message()
            )),
            _ => None,
        };

        let metadata = build_package_metadata(
            package,
            &error_summary,
            install_examples,
            filter,
            filtered_count,
        );

        let finalized_id = self
            .repo
            .finalize_package_load(
                &name,
                &version,
                status,
                Some(&metadata),
                error_message.as_deref(),
            )
            .await?;

        Ok(PackageInstallOutcome {
            package_id: finalized_id,
            name,
            version,
            already_loaded: false,
            attempted_resources,
            stored_resources,
            linked_resources,
            failed_resources,
            status: status.to_string(),
            error_message,
            error_summary: if error_summary.has_failures() {
                Some(error_summary)
            } else {
                None
            },
        })
    }

    #[allow(dead_code)]
    pub async fn install_packages(
        &self,
        packages: &[FhirPackage],
        install_examples: bool,
        filter: &crate::config::ResourceTypeFilter,
    ) -> Result<Vec<PackageInstallOutcome>> {
        let mut outcomes = Vec::with_capacity(packages.len());
        for package in packages {
            outcomes.push(
                self.install_package(package, install_examples, filter)
                    .await?,
            );
        }
        Ok(outcomes)
    }
}

fn parse_etag(etag: &str) -> Option<i32> {
    etag.trim_start_matches("W/\"")
        .trim_end_matches('"')
        .parse()
        .ok()
}

fn parse_history_version(location: &str) -> Option<i32> {
    let parts: Vec<&str> = location.split('/').filter(|s| !s.is_empty()).collect();
    let history_idx = parts.iter().position(|p| *p == "_history")?;
    parts.get(history_idx + 1)?.parse().ok()
}

fn annotate_meta_source(mut resource: JsonValue, source: &str) -> JsonValue {
    let Some(obj) = resource.as_object_mut() else {
        return resource;
    };

    let meta_value = obj
        .entry("meta")
        .or_insert_with(|| JsonValue::Object(Map::new()));

    if !meta_value.is_object() {
        *meta_value = JsonValue::Object(Map::new());
    }

    if let Some(meta_obj) = meta_value.as_object_mut() {
        meta_obj.insert("source".to_string(), JsonValue::String(source.to_string()));
    }

    resource
}

fn build_package_metadata(
    package: &FhirPackage,
    error_summary: &ErrorSummary,
    install_examples: bool,
    filter: &crate::config::ResourceTypeFilter,
    filtered_count: usize,
) -> JsonValue {
    let mut conformance_types = std::collections::BTreeSet::<String>::new();
    for r in package.conformance_resources() {
        if let Some(rt) = r.get("resourceType").and_then(|v| v.as_str()) {
            conformance_types.insert(rt.to_string());
        }
    }

    let mut example_types = std::collections::BTreeSet::<String>::new();
    for r in package.example_resources() {
        if let Some(rt) = r.get("resourceType").and_then(|v| v.as_str()) {
            example_types.insert(rt.to_string());
        }
    }

    let mut metadata = json!({
        "manifest": &package.manifest,
        "conformance_resource_types": conformance_types.into_iter().collect::<Vec<_>>(),
        "conformance_resources": package.conformance_resources().len(),
        "example_types": example_types.into_iter().collect::<Vec<_>>(),
        "examples": package.example_resources().len(),
        "total_resources": package.conformance_resources().len() + package.example_resources().len(),
        "install_examples": install_examples,
        "filtered_resources": filtered_count,
        "installed_resources": package.conformance_resources().len() + if install_examples { package.example_resources().len() } else { 0 } - filtered_count,
    });

    // Add filter information if active
    if filter.is_active() {
        let filter_info = if let Some(include) = &filter.include_resource_types {
            json!({
                "mode": "include",
                "resource_types": include,
            })
        } else if let Some(exclude) = &filter.exclude_resource_types {
            json!({
                "mode": "exclude",
                "resource_types": exclude,
            })
        } else {
            json!(null)
        };

        if let Some(obj) = metadata.as_object_mut() {
            obj.insert("resource_type_filter".to_string(), filter_info);
        }
    }

    if error_summary.has_failures() {
        let load_summary = json!({
            "total": error_summary.total_failures,
            "by_category": serialize_category_map(&error_summary.by_category),
            "by_resource_type": &error_summary.by_resource_type,
            "sample_failures": &error_summary.sample_failures,
        });

        if let Some(obj) = metadata.as_object_mut() {
            obj.insert("load_summary".to_string(), load_summary);
        }
    }

    metadata
}

fn serialize_category_map(map: &HashMap<ErrorCategory, usize>) -> HashMap<String, usize> {
    map.iter().map(|(k, v)| (format!("{:?}", k), *v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotate_meta_source() {
        let resource = json!({
            "resourceType": "Patient",
            "id": "example"
        });

        let annotated = annotate_meta_source(resource, "package:test#1.0.0");

        assert_eq!(
            annotated["meta"]["source"],
            JsonValue::String("package:test#1.0.0".to_string())
        );
    }

    #[test]
    fn test_outcome_status_checks() {
        let success = PackageInstallOutcome {
            package_id: 1,
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            already_loaded: false,
            attempted_resources: 10,
            stored_resources: 10,
            linked_resources: 10,
            failed_resources: 0,
            status: "loaded".to_string(),
            error_message: None,
            error_summary: None,
        };

        assert!(success.is_success());
        assert!(!success.is_failure());
        assert!(!success.is_partial());
    }
}
