//! Runtime configuration module
//!
//! Provides runtime-configurable settings that can be changed through the admin UI
//! and stored in the database. Changes take effect immediately without server restart.

pub mod cache;
pub mod keys;

pub use cache::RuntimeConfigCache;
pub use keys::{ConfigCategory, ConfigKey, ConfigValueType};

use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};

/// A runtime configuration entry stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub category: String,
    pub description: Option<String>,
    pub value_type: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    pub version: i32,
}

/// A runtime configuration entry with metadata for the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigEntryWithMetadata {
    pub key: String,
    pub value: serde_json::Value,
    pub default_value: serde_json::Value,
    pub category: String,
    pub description: String,
    pub value_type: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
    pub is_default: bool,
    /// Possible values for enum types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    /// Minimum value for numeric types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<i64>,
    /// Maximum value for numeric types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<i64>,
}

/// Audit log entry for configuration changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigAuditEntry {
    pub id: i64,
    pub key: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: serde_json::Value,
    pub changed_by: Option<String>,
    pub changed_at: DateTime<Utc>,
    pub change_type: String,
}

/// Request to update a configuration value
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfigRequest {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Response for configuration list
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfigListResponse {
    pub entries: Vec<RuntimeConfigEntryWithMetadata>,
    pub total: usize,
}

/// Response for audit log list
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeConfigAuditResponse {
    pub entries: Vec<RuntimeConfigAuditEntry>,
    pub total: i64,
}
