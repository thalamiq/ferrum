//! History bundle processing service
//!
//! Implements FHIR history bundle replication rules:
//! - Entries are processed independently and sequentially (not atomic)
//! - Resource IDs are preserved from the bundle
//! - Create or update based on existence
//! - Version ordering is not guaranteed; duplicate/old versions are ignored
//! - References are not rewritten

use crate::{
    db::PostgresResourceStore,
    hooks::ResourceHook,
    models::Resource,
    queue::{JobPriority, JobQueue},
    runtime_config::RuntimeConfigCache,
    services::{
        batch::{BundleRequestOptions, PreferReturn},
        CrudService,
    },
    Result,
};
use axum::http::StatusCode;
use serde_json::{json, Value as JsonValue};
use std::{collections::HashMap, sync::Arc};
use ferrum_models::{Bundle, BundleEntry, BundleEntryResponse, BundleType};
use uuid::Uuid;

pub struct HistoryService {
    store: PostgresResourceStore,
    #[allow(dead_code)]
    hooks: Vec<Arc<dyn ResourceHook>>,
    job_queue: Arc<dyn JobQueue>,
    allow_update_create: bool,
    hard_delete: bool,
    runtime_config_cache: Option<Arc<RuntimeConfigCache>>,
}

impl HistoryService {
    pub fn new(
        store: PostgresResourceStore,
        hooks: Vec<Arc<dyn ResourceHook>>,
        job_queue: Arc<dyn JobQueue>,
        allow_update_create: bool,
        hard_delete: bool,
    ) -> Self {
        Self {
            store,
            hooks,
            job_queue,
            allow_update_create,
            hard_delete,
            runtime_config_cache: None,
        }
    }

    pub fn new_with_runtime_config(
        store: PostgresResourceStore,
        hooks: Vec<Arc<dyn ResourceHook>>,
        job_queue: Arc<dyn JobQueue>,
        allow_update_create: bool,
        hard_delete: bool,
        runtime_config_cache: Arc<RuntimeConfigCache>,
    ) -> Self {
        let mut service = Self::new(store, hooks, job_queue, allow_update_create, hard_delete);
        service.runtime_config_cache = Some(runtime_config_cache);
        service
    }

    /// Process a FHIR history bundle (Bundle.type = history).
    pub async fn process_bundle(&self, bundle_json: JsonValue) -> Result<JsonValue> {
        self.process_bundle_with_options(bundle_json, BundleRequestOptions::default())
            .await
    }

    pub async fn process_bundle_with_options(
        &self,
        bundle_json: JsonValue,
        options: BundleRequestOptions,
    ) -> Result<JsonValue> {
        let bundle: Bundle = serde_json::from_value(bundle_json.clone())
            .map_err(|e| crate::Error::InvalidResource(format!("Invalid Bundle: {}", e)))?;

        if bundle.bundle_type != BundleType::History {
            return Err(crate::Error::InvalidResource(format!(
                "Unsupported Bundle type: {:?}. HistoryService requires type 'history'",
                bundle.bundle_type
            )));
        }

        let response_bundle = self.process_history(bundle, &options).await?;

        if let Err(e) = self.trigger_conformance_hooks(&response_bundle).await {
            tracing::warn!("Failed to trigger conformance hooks: {}", e);
        }

        let affected_resources = self.collect_affected_resources(&response_bundle);
        if !affected_resources.is_empty() {
            if let Err(e) = self.queue_indexing_jobs(affected_resources).await {
                tracing::warn!("Failed to queue indexing jobs: {}", e);
            }
        }

        serde_json::to_value(response_bundle).map_err(|e| {
            crate::Error::Internal(format!(
                "Failed to serialize history response bundle: {}",
                e
            ))
        })
    }

    async fn process_history(
        &self,
        bundle: Bundle,
        options: &BundleRequestOptions,
    ) -> Result<Bundle> {
        let entries = bundle.entry.unwrap_or_default();
        let mut response_entries = Vec::with_capacity(entries.len());

        if entries.is_empty() {
            return Ok(Bundle {
                resource_type: "Bundle".to_string(),
                id: Some(Uuid::new_v4().to_string()),
                bundle_type: BundleType::BatchResponse,
                timestamp: None,
                total: None,
                link: None,
                entry: Some(vec![]),
                signature: None,
                extensions: HashMap::new(),
            });
        }

        let crud = if let Some(cache) = &self.runtime_config_cache {
            CrudService::new_with_policy_and_runtime_config(
                self.store.clone(),
                self.allow_update_create,
                self.hard_delete,
                cache.clone(),
            )
        } else {
            CrudService::new_with_policy(
                self.store.clone(),
                self.allow_update_create,
                self.hard_delete,
            )
        };

        // Process entries sequentially
        for (index, entry) in entries.iter().enumerate() {
            let response_entry = match self
                .process_entry(&crud, entry, index, options.prefer_return)
                .await
            {
                Ok(e) => e,
                Err(err) => create_error_entry(entry.full_url.as_deref(), &err),
            };

            response_entries.push(response_entry);
        }

        Ok(Bundle {
            resource_type: "Bundle".to_string(),
            id: Some(Uuid::new_v4().to_string()),
            bundle_type: BundleType::BatchResponse,
            timestamp: None,
            total: None,
            link: None,
            entry: Some(response_entries),
            signature: None,
            extensions: HashMap::new(),
        })
    }

    async fn process_entry(
        &self,
        crud: &CrudService,
        entry: &BundleEntry,
        index: usize,
        prefer_return: PreferReturn,
    ) -> Result<BundleEntry> {
        // History entries have resource directly (no request element)
        let resource = entry.resource.clone().ok_or_else(|| {
            crate::Error::InvalidResource(format!("History entry {} missing resource", index))
        })?;

        // Extract resource type and ID from resource
        let resource_type = resource
            .get("resourceType")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                crate::Error::InvalidResource(format!(
                    "History entry {} missing resourceType",
                    index
                ))
            })?
            .to_string();

        let resource_id = resource
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                crate::Error::InvalidResource(format!(
                    "History entry {} missing resource.id",
                    index
                ))
            })?
            .to_string();

        // Check if this is a delete entry (response.status = "410 Gone")
        let is_delete = entry
            .response
            .as_ref()
            .map(|r| r.status.starts_with("410"))
            .unwrap_or(false);

        if is_delete {
            // Handle delete as logical delete
            match crud.delete_resource(&resource_type, &resource_id).await {
                Ok(version_id) => Ok(BundleEntry {
                    full_url: entry.full_url.clone(),
                    request: None,
                    response: Some(BundleEntryResponse {
                        status: status_line(StatusCode::NO_CONTENT),
                        location: None,
                        etag: version_id.map(|v| format!("W/\"{}\"", v)),
                        last_modified: None,
                        outcome: None,
                        extensions: HashMap::new(),
                    }),
                    resource: None,
                    search: None,
                    extensions: HashMap::new(),
                }),
                Err(crate::Error::ResourceNotFound { .. }) => {
                    // Resource already deleted or doesn't exist - ignore
                    Ok(BundleEntry {
                        full_url: entry.full_url.clone(),
                        request: None,
                        response: Some(BundleEntryResponse {
                            status: status_line(StatusCode::NO_CONTENT),
                            location: None,
                            etag: None,
                            last_modified: None,
                            outcome: None,
                            extensions: HashMap::new(),
                        }),
                        resource: None,
                        search: None,
                        extensions: HashMap::new(),
                    })
                }
                Err(e) => Err(e),
            }
        } else {
            // Check if resource exists
            let existing = crud.read_resource(&resource_type, &resource_id).await;

            match existing {
                Ok(existing_resource) => {
                    // Resource exists - check version to avoid applying old updates
                    let new_version = resource
                        .get("meta")
                        .and_then(|m| m.get("versionId"))
                        .and_then(|v| v.as_str())
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or(1);

                    let existing_version = existing_resource.version_id;

                    if new_version <= existing_version {
                        // Ignore older or duplicate version
                        return Ok(BundleEntry {
                            full_url: entry.full_url.clone(),
                            request: None,
                            response: Some(BundleEntryResponse {
                                status: status_line(StatusCode::OK),
                                location: Some(format!(
                                    "{}/{}/_history/{}",
                                    resource_type,
                                    existing_resource.id,
                                    existing_resource.version_id
                                )),
                                etag: Some(format!("W/\"{}\"", existing_resource.version_id)),
                                last_modified: Some(existing_resource.last_updated.to_rfc3339()),
                                outcome: match prefer_return {
                                    PreferReturn::OperationOutcome => Some(serde_json::json!({
                                        "resourceType": "OperationOutcome",
                                        "issue": [{
                                            "severity": "information",
                                            "code": "informational",
                                            "diagnostics": "Ignored older or duplicate history entry"
                                        }]
                                    })),
                                    _ => None,
                                },
                                extensions: HashMap::new(),
                            }),
                            resource: match prefer_return {
                                PreferReturn::Representation => Some(existing_resource.resource),
                                _ => None,
                            },
                            search: None,
                            extensions: HashMap::new(),
                        });
                    }

                    // Update with newer version
                    let result = crud
                        .update_resource(&resource_type, &resource_id, resource, None)
                        .await?;

                    Ok(BundleEntry {
                        full_url: entry.full_url.clone(),
                        request: None,
                        response: Some(BundleEntryResponse {
                            status: status_line(StatusCode::OK),
                            location: Some(format!(
                                "{}/{}/_history/{}",
                                resource_type, result.resource.id, result.resource.version_id
                            )),
                            etag: Some(format!("W/\"{}\"", result.resource.version_id)),
                            last_modified: Some(result.resource.last_updated.to_rfc3339()),
                            outcome: match prefer_return {
                                PreferReturn::OperationOutcome => Some(serde_json::json!({
                                    "resourceType": "OperationOutcome",
                                    "issue": [{
                                        "severity": "information",
                                        "code": "informational",
                                        "diagnostics": format!(
                                            "Resource updated successfully: {}/{}",
                                            resource_type, resource_id
                                        )
                                    }]
                                })),
                                _ => None,
                            },
                            extensions: HashMap::new(),
                        }),
                        resource: match prefer_return {
                            PreferReturn::Representation => Some(result.resource.resource),
                            _ => None,
                        },
                        search: None,
                        extensions: HashMap::new(),
                    })
                }
                Err(crate::Error::ResourceNotFound { .. })
                | Err(crate::Error::ResourceDeleted { .. }) => {
                    // Resource doesn't exist - create it
                    let result = crud.create_resource(&resource_type, resource, None).await?;

                    Ok(BundleEntry {
                        full_url: entry.full_url.clone(),
                        request: None,
                        response: Some(BundleEntryResponse {
                            status: status_line(StatusCode::CREATED),
                            location: Some(format!(
                                "{}/{}/_history/{}",
                                resource_type, result.resource.id, result.resource.version_id
                            )),
                            etag: Some(format!("W/\"{}\"", result.resource.version_id)),
                            last_modified: Some(result.resource.last_updated.to_rfc3339()),
                            outcome: match prefer_return {
                                PreferReturn::OperationOutcome => Some(serde_json::json!({
                                    "resourceType": "OperationOutcome",
                                    "issue": [{
                                        "severity": "information",
                                        "code": "informational",
                                        "diagnostics": format!(
                                            "Resource created successfully: {}/{}",
                                            resource_type, resource_id
                                        )
                                    }]
                                })),
                                _ => None,
                            },
                            extensions: HashMap::new(),
                        }),
                        resource: match prefer_return {
                            PreferReturn::Representation => Some(result.resource.resource),
                            _ => None,
                        },
                        search: None,
                        extensions: HashMap::new(),
                    })
                }
                Err(e) => Err(e),
            }
        }
    }

    fn collect_affected_resources(&self, bundle: &Bundle) -> HashMap<String, Vec<String>> {
        let mut resources: HashMap<String, Vec<String>> = HashMap::new();

        if let Some(entries) = &bundle.entry {
            for entry in entries {
                if let Some(response) = &entry.response {
                    let status = response.status.as_str();
                    if status.starts_with("200") || status.starts_with("201") {
                        let parsed_from_location = response.location.as_ref().and_then(|loc| {
                            let parts: Vec<&str> =
                                loc.split('/').filter(|s| !s.is_empty()).collect();
                            match (parts.first(), parts.get(1)) {
                                (Some(rt), Some(id)) => Some((rt.to_string(), id.to_string())),
                                _ => None,
                            }
                        });

                        let parsed_from_resource = entry.resource.as_ref().and_then(|r| {
                            let rt = r.get("resourceType")?.as_str()?.to_string();
                            let id = r.get("id")?.as_str()?.to_string();
                            Some((rt, id))
                        });

                        if let Some((rt, id)) = parsed_from_location.or(parsed_from_resource) {
                            resources.entry(rt).or_default().push(id);
                        }
                    }
                }
            }
        }

        resources
    }

    async fn queue_indexing_jobs(&self, resources: HashMap<String, Vec<String>>) -> Result<()> {
        for (resource_type, resource_ids) in resources {
            if resource_ids.is_empty() {
                continue;
            }

            let parameters = json!({
                "resource_type": resource_type,
                "resource_ids": resource_ids,
            });

            if let Err(e) = self
                .job_queue
                .enqueue(
                    "index_search".to_string(),
                    parameters,
                    JobPriority::Normal,
                    None,
                )
                .await
            {
                tracing::warn!("Failed to queue indexing job: {}", e);
            }
        }

        Ok(())
    }

    async fn trigger_conformance_hooks(&self, bundle: &Bundle) -> Result<()> {
        let mut conformance_ids: HashMap<String, Vec<String>> = HashMap::new();

        if let Some(entries) = &bundle.entry {
            for entry in entries {
                if let Some(response) = &entry.response {
                    let status = response.status.as_str();
                    if !(status.starts_with("200") || status.starts_with("201")) {
                        continue;
                    }

                    let parsed = response.location.as_ref().and_then(|loc| {
                        let parts: Vec<&str> = loc.split('/').filter(|s| !s.is_empty()).collect();
                        match (parts.first(), parts.get(1)) {
                            (Some(rt), Some(id)) => Some((rt.to_string(), id.to_string())),
                            _ => None,
                        }
                    });

                    let Some((rt, id)) = parsed else {
                        continue;
                    };

                    if crate::conformance::is_conformance_resource_type(&rt) {
                        conformance_ids.entry(rt).or_default().push(id);
                    }
                }
            }
        }

        if conformance_ids.is_empty() {
            return Ok(());
        }

        let mut all_conformance: Vec<Resource> = Vec::new();
        for (resource_type, ids) in conformance_ids {
            let resources = self
                .store
                .load_resources_batch(&resource_type, &ids)
                .await?;
            all_conformance.extend(resources);
        }

        for resource in &all_conformance {
            for hook in &self.hooks {
                if let Err(e) = hook.on_updated(resource).await {
                    tracing::warn!(
                        "Hook failed for conformance resource {}/{}: {}",
                        resource.resource_type,
                        resource.id,
                        e
                    );
                }
            }
        }

        Ok(())
    }
}

// =============================================================================
// Response helpers
// =============================================================================

fn status_line(status: StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("{} {}", status.as_u16(), reason),
        None => status.as_u16().to_string(),
    }
}

fn status_to_fhir_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid",
        StatusCode::NOT_FOUND => "not-found",
        StatusCode::GONE => "deleted",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PRECONDITION_FAILED => "conflict",
        StatusCode::UNPROCESSABLE_ENTITY => "processing",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "not-supported",
        _ => "exception",
    }
}

fn error_status(err: &crate::Error) -> StatusCode {
    match err {
        crate::Error::ResourceNotFound { .. } => StatusCode::NOT_FOUND,
        crate::Error::VersionNotFound { .. } => StatusCode::NOT_FOUND,
        crate::Error::NotFound(_) => StatusCode::NOT_FOUND,
        crate::Error::ResourceDeleted { .. } => StatusCode::GONE,
        crate::Error::InvalidResource(_)
        | crate::Error::Validation(_)
        | crate::Error::InvalidReference(_) => StatusCode::BAD_REQUEST,
        crate::Error::BusinessRule(_) => StatusCode::CONFLICT,
        crate::Error::VersionConflict { .. } | crate::Error::PreconditionFailed(_) => {
            StatusCode::PRECONDITION_FAILED
        }
        crate::Error::MethodNotAllowed(_) => StatusCode::METHOD_NOT_ALLOWED,
        crate::Error::Search(_) => StatusCode::BAD_REQUEST,
        crate::Error::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        crate::Error::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
        crate::Error::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        crate::Error::TooCostly(_) => StatusCode::FORBIDDEN,
        crate::Error::Database(_)
        | crate::Error::JobQueue(_)
        | crate::Error::FhirContext(_)
        | crate::Error::FhirPath(_)
        | crate::Error::ExternalReference(_)
        | crate::Error::Internal(_)
        | crate::Error::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn create_error_entry(full_url: Option<&str>, err: &crate::Error) -> BundleEntry {
    let status = error_status(err);
    let outcome = json!({
        "resourceType": "OperationOutcome",
        "issue": [{
            "severity": "error",
            "code": status_to_fhir_code(status),
            "diagnostics": err.to_string()
        }]
    });

    BundleEntry {
        full_url: full_url.map(|s| s.to_string()),
        request: None,
        response: Some(BundleEntryResponse {
            status: status_line(status),
            location: None,
            etag: None,
            last_modified: None,
            outcome: Some(outcome),
            extensions: HashMap::new(),
        }),
        resource: None,
        search: None,
        extensions: HashMap::new(),
    }
}
