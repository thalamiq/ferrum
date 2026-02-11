//! Domain models for FHIR REST operations (inlined from fhir-rest)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A FHIR resource with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Resource ID
    pub id: String,

    /// Resource type (e.g., "Patient", "Observation")
    pub resource_type: String,

    /// Version ID (starts at 1)
    pub version_id: i32,

    /// Full resource JSON
    pub resource: JsonValue,

    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,

    /// Is this resource deleted?
    pub deleted: bool,
}

/// Result of a resource operation
#[derive(Debug, Clone)]
pub struct ResourceResult {
    /// The resource
    pub resource: Resource,

    /// Operation that was performed
    pub operation: ResourceOperation,
}

/// Type of operation performed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceOperation {
    /// Resource was created (HTTP 201)
    Created,

    /// Resource was updated (HTTP 200)
    Updated,

    /// No operation performed - resource unchanged (HTTP 200)
    NoOp,

    /// Resource was deleted (HTTP 204 or 200 with OperationOutcome)
    Deleted,
}

impl ResourceOperation {
    /// Get HTTP status code for this operation
    pub fn status_code(&self) -> u16 {
        match self {
            ResourceOperation::Created => 201,
            ResourceOperation::Updated => 200,
            ResourceOperation::NoOp => 200,
            ResourceOperation::Deleted => 204,
        }
    }
}

/// Conditional operation parameters
#[derive(Debug, Clone)]
pub struct ConditionalParams {
    /// Search parameters for conditional operation
    pub search_params: Vec<(String, String)>,
}

impl ConditionalParams {
    pub fn from_query_string(query: &str) -> Self {
        let search_params = query
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => Some((key.to_string(), value.to_string())),
                    _ => None,
                }
            })
            .collect();

        Self { search_params }
    }

    pub fn is_empty(&self) -> bool {
        self.search_params.is_empty()
    }
}

/// History entry for a resource
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub resource: Resource,
    pub method: HistoryMethod,
}

#[derive(Debug, Clone, Copy)]
pub enum HistoryMethod {
    Post,   // Created
    Put,    // Updated
    Delete, // Deleted
}

/// History bundle result
#[derive(Debug, Clone)]
pub struct HistoryResult {
    pub entries: Vec<HistoryEntry>,
    pub total: Option<i64>,
}

/// Version-aware update parameters
#[derive(Debug, Clone)]
pub struct UpdateParams {
    /// Expected version (for If-Match)
    pub if_match: Option<i32>,
}

/// Create parameters
#[derive(Debug, Clone)]
pub struct CreateParams {
    /// Conditional create search criteria (for If-None-Exist)
    pub if_none_exist: Option<ConditionalParams>,
}
