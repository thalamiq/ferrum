//! Runtime configuration service
//!
//! Business logic for managing runtime configuration settings.

use crate::db::RuntimeConfigRepository;
use crate::runtime_config::{
    ConfigCategory, ConfigKey, ConfigValueType, RuntimeConfigAuditResponse, RuntimeConfigCache,
    RuntimeConfigEntryWithMetadata, RuntimeConfigListResponse, UpdateConfigRequest,
};
use crate::{Error, Result};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Service for managing runtime configuration
pub struct RuntimeConfigService {
    repo: RuntimeConfigRepository,
    cache: Arc<RuntimeConfigCache>,
}

impl RuntimeConfigService {
    /// Create a new runtime configuration service
    pub fn new(repo: RuntimeConfigRepository, cache: Arc<RuntimeConfigCache>) -> Self {
        Self { repo, cache }
    }

    /// Initialize the cache from database
    pub async fn initialize_cache(&self) -> Result<()> {
        let values = self.repo.get_all_as_map().await?;
        self.cache.load(values).await;
        tracing::info!(
            "Loaded {} runtime configuration overrides from database",
            self.cache.get_all().await.len()
        );
        Ok(())
    }

    /// Get all configuration entries with metadata
    pub async fn list_all(&self, category: Option<&str>) -> Result<RuntimeConfigListResponse> {
        let db_entries = self.repo.get_all().await?;
        let db_map: std::collections::HashMap<_, _> =
            db_entries.into_iter().map(|e| (e.key.clone(), e)).collect();

        let keys = if let Some(cat) = category {
            let cat = ConfigCategory::from_str(cat)
                .ok_or_else(|| Error::Validation(format!("Invalid category: {}", cat)))?;
            ConfigKey::all()
                .into_iter()
                .filter(|k| k.category() == cat)
                .collect()
        } else {
            ConfigKey::all()
        };

        let mut entries = Vec::with_capacity(keys.len());

        for key in keys {
            let key_str = key.as_str();
            let default_value = self.cache.get_static_default_json(key);

            let entry = if let Some(db_entry) = db_map.get(key_str) {
                RuntimeConfigEntryWithMetadata {
                    key: key_str.to_string(),
                    value: db_entry.value.clone(),
                    default_value,
                    category: key.category().to_string(),
                    description: key.description().to_string(),
                    value_type: key.value_type().to_string(),
                    updated_at: Some(db_entry.updated_at),
                    updated_by: db_entry.updated_by.clone(),
                    is_default: false,
                    enum_values: key
                        .enum_values()
                        .map(|v| v.iter().map(|s| s.to_string()).collect()),
                    min_value: key.integer_bounds().map(|(min, _)| min),
                    max_value: key.integer_bounds().map(|(_, max)| max),
                }
            } else {
                RuntimeConfigEntryWithMetadata {
                    key: key_str.to_string(),
                    value: default_value.clone(),
                    default_value,
                    category: key.category().to_string(),
                    description: key.description().to_string(),
                    value_type: key.value_type().to_string(),
                    updated_at: None,
                    updated_by: None,
                    is_default: true,
                    enum_values: key
                        .enum_values()
                        .map(|v| v.iter().map(|s| s.to_string()).collect()),
                    min_value: key.integer_bounds().map(|(min, _)| min),
                    max_value: key.integer_bounds().map(|(_, max)| max),
                }
            };

            entries.push(entry);
        }

        let total = entries.len();
        Ok(RuntimeConfigListResponse { entries, total })
    }

    /// Get a single configuration entry
    pub async fn get(&self, key_str: &str) -> Result<RuntimeConfigEntryWithMetadata> {
        let key = ConfigKey::from_str(key_str)
            .ok_or_else(|| Error::Validation(format!("Unknown configuration key: {}", key_str)))?;

        let db_entry = self.repo.get(key_str).await?;
        let default_value = self.cache.get_static_default_json(key);

        let entry = if let Some(db_entry) = db_entry {
            RuntimeConfigEntryWithMetadata {
                key: key_str.to_string(),
                value: db_entry.value,
                default_value,
                category: key.category().to_string(),
                description: key.description().to_string(),
                value_type: key.value_type().to_string(),
                updated_at: Some(db_entry.updated_at),
                updated_by: db_entry.updated_by,
                is_default: false,
                enum_values: key
                    .enum_values()
                    .map(|v| v.iter().map(|s| s.to_string()).collect()),
                min_value: key.integer_bounds().map(|(min, _)| min),
                max_value: key.integer_bounds().map(|(_, max)| max),
            }
        } else {
            RuntimeConfigEntryWithMetadata {
                key: key_str.to_string(),
                value: default_value.clone(),
                default_value,
                category: key.category().to_string(),
                description: key.description().to_string(),
                value_type: key.value_type().to_string(),
                updated_at: None,
                updated_by: None,
                is_default: true,
                enum_values: key
                    .enum_values()
                    .map(|v| v.iter().map(|s| s.to_string()).collect()),
                min_value: key.integer_bounds().map(|(min, _)| min),
                max_value: key.integer_bounds().map(|(_, max)| max),
            }
        };

        Ok(entry)
    }

    /// Update a configuration value
    pub async fn update(
        &self,
        key_str: &str,
        request: UpdateConfigRequest,
    ) -> Result<RuntimeConfigEntryWithMetadata> {
        let key = ConfigKey::from_str(key_str)
            .ok_or_else(|| Error::Validation(format!("Unknown configuration key: {}", key_str)))?;

        // Validate the value
        self.validate_value(key, &request.value)?;

        // Save to database
        let entry = self
            .repo
            .upsert(
                key_str,
                &request.value,
                &key.category().to_string(),
                key.description(),
                &key.value_type().to_string(),
                request.updated_by.as_deref(),
            )
            .await?;

        // Update cache
        self.cache.set(key_str, request.value.clone()).await;

        let default_value = self.cache.get_static_default_json(key);

        Ok(RuntimeConfigEntryWithMetadata {
            key: key_str.to_string(),
            value: entry.value,
            default_value,
            category: key.category().to_string(),
            description: key.description().to_string(),
            value_type: key.value_type().to_string(),
            updated_at: Some(entry.updated_at),
            updated_by: entry.updated_by,
            is_default: false,
            enum_values: key
                .enum_values()
                .map(|v| v.iter().map(|s| s.to_string()).collect()),
            min_value: key.integer_bounds().map(|(min, _)| min),
            max_value: key.integer_bounds().map(|(_, max)| max),
        })
    }

    /// Reset a configuration value to default
    pub async fn reset(&self, key_str: &str) -> Result<RuntimeConfigEntryWithMetadata> {
        let key = ConfigKey::from_str(key_str)
            .ok_or_else(|| Error::Validation(format!("Unknown configuration key: {}", key_str)))?;

        // Delete from database
        self.repo.delete(key_str).await?;

        // Remove from cache
        self.cache.remove(key_str).await;

        let default_value = self.cache.get_static_default_json(key);

        Ok(RuntimeConfigEntryWithMetadata {
            key: key_str.to_string(),
            value: default_value.clone(),
            default_value,
            category: key.category().to_string(),
            description: key.description().to_string(),
            value_type: key.value_type().to_string(),
            updated_at: None,
            updated_by: None,
            is_default: true,
            enum_values: key
                .enum_values()
                .map(|v| v.iter().map(|s| s.to_string()).collect()),
            min_value: key.integer_bounds().map(|(min, _)| min),
            max_value: key.integer_bounds().map(|(_, max)| max),
        })
    }

    /// Get audit log
    pub async fn get_audit_log(
        &self,
        key: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<RuntimeConfigAuditResponse> {
        // Validate key if provided
        if let Some(k) = key {
            if ConfigKey::from_str(k).is_none() {
                return Err(Error::Validation(format!(
                    "Unknown configuration key: {}",
                    k
                )));
            }
        }

        let entries = self.repo.get_audit_log(key, limit, offset).await?;
        let total = self.repo.count_audit_log(key).await?;

        Ok(RuntimeConfigAuditResponse { entries, total })
    }

    /// Handle cache invalidation notification from another server instance
    pub async fn invalidate_cache_entry(&self, key: &str) -> Result<()> {
        // Reload the entry from the database
        if let Some(entry) = self.repo.get(key).await? {
            self.cache.set(key, entry.value).await;
            tracing::debug!("Cache invalidated for key: {}", key);
        } else {
            // Entry was deleted, remove from cache
            self.cache.remove(key).await;
            tracing::debug!("Cache entry removed for key: {}", key);
        }
        Ok(())
    }

    /// Validate a configuration value
    fn validate_value(&self, key: ConfigKey, value: &JsonValue) -> Result<()> {
        match key.value_type() {
            ConfigValueType::Boolean => {
                if !value.is_boolean() {
                    return Err(Error::Validation(format!(
                        "Expected boolean value for key: {}",
                        key.as_str()
                    )));
                }
            }
            ConfigValueType::Integer => {
                let num = value.as_i64().ok_or_else(|| {
                    Error::Validation(format!("Expected integer value for key: {}", key.as_str()))
                })?;

                if let Some((min, max)) = key.integer_bounds() {
                    if num < min || num > max {
                        return Err(Error::Validation(format!(
                            "Value {} out of range [{}, {}] for key: {}",
                            num,
                            min,
                            max,
                            key.as_str()
                        )));
                    }
                }
            }
            ConfigValueType::String => {
                if !value.is_string() {
                    return Err(Error::Validation(format!(
                        "Expected string value for key: {}",
                        key.as_str()
                    )));
                }
            }
            ConfigValueType::StringEnum => {
                let s = value.as_str().ok_or_else(|| {
                    Error::Validation(format!("Expected string value for key: {}", key.as_str()))
                })?;

                if let Some(valid_values) = key.enum_values() {
                    if !valid_values.contains(&s) {
                        return Err(Error::Validation(format!(
                            "Invalid value '{}' for key: {}. Valid values: {:?}",
                            s,
                            key.as_str(),
                            valid_values
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Get the cache reference for direct access
    pub fn cache(&self) -> &Arc<RuntimeConfigCache> {
        &self.cache
    }
}
