//! Core traits for FHIR REST storage backends

use crate::{
    models::fhir::{HistoryResult, Resource},
    Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

/// Core storage trait for FHIR resources
///
/// This trait defines the minimal storage operations needed to implement
/// a FHIR REST API. Any storage backend (PostgreSQL, MongoDB, HTTP proxy,
/// in-memory, etc.) can implement this trait.
///
/// All methods are version-aware and handle soft deletes.
#[async_trait]
pub trait ResourceStore: Send + Sync + Clone {
    /// Create a new resource with a server-assigned ID
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type (e.g., "Patient")
    /// * `resource` - The resource JSON (should NOT have an id)
    ///
    /// # Returns
    /// The created resource with server-assigned ID and version 1
    async fn create(&self, resource_type: &str, resource: JsonValue) -> Result<Resource>;

    /// Create or update a resource with a client-specified ID
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `id` - The resource ID (client-specified)
    /// * `resource` - The resource JSON
    ///
    /// # Returns
    /// The resource with operation indicating if it was created or updated
    async fn upsert(&self, resource_type: &str, id: &str, resource: JsonValue) -> Result<Resource>;

    /// Read the current version of a resource
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `id` - The resource ID
    ///
    /// # Returns
    /// * `Ok(Some(resource))` - Resource found and not deleted
    /// * `Ok(None)` - Resource not found or deleted
    async fn read(&self, resource_type: &str, id: &str) -> Result<Option<Resource>>;

    /// Update an existing resource (creates new version)
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `id` - The resource ID
    /// * `resource` - The new resource JSON
    /// * `expected_version` - Optional version check (for If-Match)
    ///
    /// # Returns
    /// The updated resource with incremented version
    ///
    /// # Errors
    /// * `VersionConflict` - If expected_version doesn't match current
    /// * `ResourceNotFound` - If resource doesn't exist
    async fn update(
        &self,
        resource_type: &str,
        id: &str,
        resource: JsonValue,
        expected_version: Option<i32>,
    ) -> Result<Resource>;

    /// Soft delete a resource (creates new version with deleted=true)
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `id` - The resource ID
    ///
    /// # Returns
    /// The new version ID of the deleted resource
    ///
    /// # Errors
    /// * `ResourceNotFound` - If resource doesn't exist
    async fn delete(&self, resource_type: &str, id: &str) -> Result<i32>;

    /// Read a specific version of a resource
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `id` - The resource ID
    /// * `version_id` - The version ID
    ///
    /// # Returns
    /// The specified version of the resource (even if deleted)
    ///
    /// # Errors
    /// * `VersionNotFound` - If version doesn't exist
    async fn vread(&self, resource_type: &str, id: &str, version_id: i32) -> Result<Resource>;

    /// Get version history for a resource
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `id` - The resource ID
    /// * `count` - Maximum number of versions to return
    /// * `since` - Only return versions created at or after this instant
    /// * `at` - Only return the version(s) that were current at this instant
    /// * `sort_ascending` - Sort by `_lastUpdated` ascending when true, descending when false
    ///
    /// # Returns
    /// History bundle with all versions (newest first)
    async fn history(
        &self,
        resource_type: &str,
        id: &str,
        count: Option<i32>,
        since: Option<DateTime<Utc>>,
        at: Option<DateTime<Utc>>,
        sort_ascending: bool,
    ) -> Result<HistoryResult>;

    /// Search for resources matching criteria
    ///
    /// Used for conditional operations (If-None-Exist)
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `search_params` - Search parameters (key-value pairs)
    ///
    /// # Returns
    /// List of matching resources
    async fn search(
        &self,
        resource_type: &str,
        search_params: &[(String, String)],
    ) -> Result<Vec<Resource>>;

    /// Load multiple resources by IDs in a single batch operation
    ///
    /// # Arguments
    /// * `resource_type` - The FHIR resource type
    /// * `ids` - List of resource IDs to load
    ///
    /// # Returns
    /// List of resources that were found (may be fewer than requested if some don't exist)
    async fn load_resources_batch(
        &self,
        resource_type: &str,
        ids: &[String],
    ) -> Result<Vec<Resource>>;
}

/// Transaction support for atomic operations
///
/// Optional trait for backends that support transactions.
/// Used for FHIR batch/transaction bundles.
#[async_trait]
pub trait ResourceTransaction: ResourceStore + Sized {
    type Context: TransactionContext;

    /// Begin a transaction
    async fn begin_transaction(&self) -> Result<Self::Context>;
}

/// Transaction context for atomic operations
///
/// This trait provides methods to execute CRUD operations within a transaction scope.
/// All operations are atomic - either all succeed or all fail.
#[async_trait]
pub trait TransactionContext: Send {
    /// Commit the transaction
    async fn commit(self) -> Result<()>;

    /// Rollback the transaction (optional, called on drop)
    async fn rollback(self) -> Result<()>;

    /// Create a resource within the transaction.
    ///
    /// The service layer must populate `id` and `meta` per FHIR rules before calling this.
    async fn create(&mut self, resource_type: &str, resource: JsonValue) -> Result<Resource>;

    /// Create or update (upsert) a resource within the transaction.
    ///
    /// The service layer must populate `id` and `meta` per FHIR rules before calling this.
    async fn upsert(
        &mut self,
        resource_type: &str,
        id: &str,
        resource: JsonValue,
    ) -> Result<Resource>;

    /// Read the current version of a resource within the transaction.
    async fn read(&mut self, resource_type: &str, id: &str) -> Result<Option<Resource>>;

    /// Soft delete a resource within the transaction.
    ///
    /// Returns the new version ID.
    async fn delete(&mut self, resource_type: &str, id: &str) -> Result<i32>;
}
