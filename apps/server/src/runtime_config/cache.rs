//! In-memory cache for runtime configuration
//!
//! Provides fast access to configuration values with automatic cache invalidation
//! via PostgreSQL LISTEN/NOTIFY.

use crate::config::Config;
use crate::runtime_config::keys::ConfigKey;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory cache for runtime configuration values
///
/// This cache is populated from the database at startup and updated
/// via PostgreSQL LISTEN/NOTIFY when values change. If a value is not
/// found in the cache, it falls back to the static configuration defaults.
#[derive(Debug)]
pub struct RuntimeConfigCache {
    /// Cached values from the database (key -> JSON value)
    cache: RwLock<HashMap<String, JsonValue>>,
    /// Reference to static configuration for defaults
    static_config: Arc<Config>,
}

impl RuntimeConfigCache {
    /// Create a new runtime configuration cache
    pub fn new(static_config: Arc<Config>) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            static_config,
        }
    }

    /// Get a configuration value, falling back to static config if not in cache
    pub async fn get<T: DeserializeOwned>(&self, key: ConfigKey) -> T {
        let cache = self.cache.read().await;
        if let Some(value) = cache.get(key.as_str()) {
            if let Ok(parsed) = serde_json::from_value(value.clone()) {
                return parsed;
            }
        }
        drop(cache);

        // Fall back to static config default
        self.get_static_default(key)
    }

    /// Get a configuration value as JSON
    pub async fn get_json(&self, key: ConfigKey) -> JsonValue {
        let cache = self.cache.read().await;
        if let Some(value) = cache.get(key.as_str()) {
            return value.clone();
        }
        drop(cache);

        // Fall back to static config default as JSON
        self.get_static_default_json(key)
    }

    /// Check if a key has a value in the cache (overriding default)
    pub async fn has_override(&self, key: ConfigKey) -> bool {
        let cache = self.cache.read().await;
        cache.contains_key(key.as_str())
    }

    /// Set a value in the cache
    pub async fn set(&self, key: &str, value: JsonValue) {
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), value);
    }

    /// Remove a value from the cache (reset to default)
    pub async fn remove(&self, key: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key);
    }

    /// Clear the entire cache
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Load initial values from a map (used at startup)
    pub async fn load(&self, values: HashMap<String, JsonValue>) {
        let mut cache = self.cache.write().await;
        *cache = values;
    }

    /// Get all cached values
    pub async fn get_all(&self) -> HashMap<String, JsonValue> {
        let cache = self.cache.read().await;
        cache.clone()
    }

    /// Get the static default value for a key
    fn get_static_default<T: DeserializeOwned>(&self, key: ConfigKey) -> T {
        let json = self.get_static_default_json(key);
        serde_json::from_value(json).expect("Static default should be valid")
    }

    /// Get the static default value as JSON
    pub fn get_static_default_json(&self, key: ConfigKey) -> JsonValue {
        match key {
            // Logging
            ConfigKey::LoggingLevel => JsonValue::String(self.static_config.logging.level.clone()),

            // Search
            ConfigKey::SearchDefaultCount => {
                JsonValue::Number(self.static_config.fhir.search.default_count.into())
            }
            ConfigKey::SearchMaxCount => {
                JsonValue::Number(self.static_config.fhir.search.max_count.into())
            }
            ConfigKey::SearchMaxTotalResults => {
                JsonValue::Number(self.static_config.fhir.search.max_total_results.into())
            }
            ConfigKey::SearchMaxIncludeDepth => {
                JsonValue::Number(self.static_config.fhir.search.max_include_depth.into())
            }
            ConfigKey::SearchMaxIncludes => {
                JsonValue::Number(self.static_config.fhir.search.max_includes.into())
            }

            // Interactions - Instance
            ConfigKey::InteractionsInstanceRead => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.read)
            }
            ConfigKey::InteractionsInstanceVread => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.vread)
            }
            ConfigKey::InteractionsInstanceUpdate => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.update)
            }
            ConfigKey::InteractionsInstancePatch => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.patch)
            }
            ConfigKey::InteractionsInstanceDelete => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.delete)
            }
            ConfigKey::InteractionsInstanceHistory => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.history)
            }
            ConfigKey::InteractionsInstanceDeleteHistory => {
                JsonValue::Bool(self.static_config.fhir.interactions.instance.delete_history)
            }
            ConfigKey::InteractionsInstanceDeleteHistoryVersion => JsonValue::Bool(
                self.static_config
                    .fhir
                    .interactions
                    .instance
                    .delete_history_version,
            ),

            // Interactions - Type
            ConfigKey::InteractionsTypeCreate => {
                JsonValue::Bool(self.static_config.fhir.interactions.type_level.create)
            }
            ConfigKey::InteractionsTypeConditionalCreate => JsonValue::Bool(
                self.static_config
                    .fhir
                    .interactions
                    .type_level
                    .conditional_create,
            ),
            ConfigKey::InteractionsTypeSearch => {
                JsonValue::Bool(self.static_config.fhir.interactions.type_level.search)
            }
            ConfigKey::InteractionsTypeHistory => {
                JsonValue::Bool(self.static_config.fhir.interactions.type_level.history)
            }
            ConfigKey::InteractionsTypeConditionalUpdate => JsonValue::Bool(
                self.static_config
                    .fhir
                    .interactions
                    .type_level
                    .conditional_update,
            ),
            ConfigKey::InteractionsTypeConditionalPatch => JsonValue::Bool(
                self.static_config
                    .fhir
                    .interactions
                    .type_level
                    .conditional_patch,
            ),
            ConfigKey::InteractionsTypeConditionalDelete => JsonValue::Bool(
                self.static_config
                    .fhir
                    .interactions
                    .type_level
                    .conditional_delete,
            ),

            // Interactions - System
            ConfigKey::InteractionsSystemCapabilities => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.capabilities)
            }
            ConfigKey::InteractionsSystemSearch => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.search)
            }
            ConfigKey::InteractionsSystemHistory => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.history)
            }
            ConfigKey::InteractionsSystemDelete => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.delete)
            }
            ConfigKey::InteractionsSystemBatch => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.batch)
            }
            ConfigKey::InteractionsSystemTransaction => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.transaction)
            }
            ConfigKey::InteractionsSystemHistoryBundle => {
                JsonValue::Bool(self.static_config.fhir.interactions.system.history_bundle)
            }

            // Interactions - Compartment
            ConfigKey::InteractionsCompartmentSearch => {
                JsonValue::Bool(self.static_config.fhir.interactions.compartment.search)
            }

            // Interactions - Operations
            ConfigKey::InteractionsOperationsSystem => {
                JsonValue::Bool(self.static_config.fhir.interactions.operations.system)
            }
            ConfigKey::InteractionsOperationsTypeLevel => {
                JsonValue::Bool(self.static_config.fhir.interactions.operations.type_level)
            }
            ConfigKey::InteractionsOperationsInstance => {
                JsonValue::Bool(self.static_config.fhir.interactions.operations.instance)
            }

            // Format
            ConfigKey::FormatDefault => {
                JsonValue::String(self.static_config.fhir.default_format.clone())
            }
            ConfigKey::FormatDefaultPreferReturn => {
                JsonValue::String(self.static_config.fhir.default_prefer_return.clone())
            }

            // Behavior
            ConfigKey::BehaviorAllowUpdateCreate => {
                JsonValue::Bool(self.static_config.fhir.allow_update_create)
            }
            ConfigKey::BehaviorHardDelete => JsonValue::Bool(self.static_config.fhir.hard_delete),

            // Audit
            ConfigKey::AuditEnabled => JsonValue::Bool(self.static_config.logging.audit.enabled),
            ConfigKey::AuditIncludeSuccess => {
                JsonValue::Bool(self.static_config.logging.audit.include_success)
            }
            ConfigKey::AuditIncludeAuthzFailure => {
                JsonValue::Bool(self.static_config.logging.audit.include_authz_failure)
            }
            ConfigKey::AuditIncludeProcessingFailure => {
                JsonValue::Bool(self.static_config.logging.audit.include_processing_failure)
            }
            ConfigKey::AuditCaptureSearchQuery => {
                JsonValue::Bool(self.static_config.logging.audit.capture_search_query)
            }
            ConfigKey::AuditCaptureOperationOutcome => {
                JsonValue::Bool(self.static_config.logging.audit.capture_operation_outcome)
            }
            ConfigKey::AuditPerPatientEventsForSearch => JsonValue::Bool(
                self.static_config
                    .logging
                    .audit
                    .per_patient_events_for_search,
            ),
            ConfigKey::AuditInteractionsRead => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.read)
            }
            ConfigKey::AuditInteractionsVread => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.vread)
            }
            ConfigKey::AuditInteractionsHistory => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.history)
            }
            ConfigKey::AuditInteractionsSearch => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.search)
            }
            ConfigKey::AuditInteractionsCreate => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.create)
            }
            ConfigKey::AuditInteractionsUpdate => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.update)
            }
            ConfigKey::AuditInteractionsPatch => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.patch)
            }
            ConfigKey::AuditInteractionsDelete => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.delete)
            }
            ConfigKey::AuditInteractionsCapabilities => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.capabilities)
            }
            ConfigKey::AuditInteractionsOperation => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.operation)
            }
            ConfigKey::AuditInteractionsBatch => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.batch)
            }
            ConfigKey::AuditInteractionsTransaction => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.transaction)
            }
            ConfigKey::AuditInteractionsExport => {
                JsonValue::Bool(self.static_config.logging.audit.interactions.export)
            }
        }
    }

    /// Get the static configuration reference
    pub fn static_config(&self) -> &Config {
        &self.static_config
    }
}
