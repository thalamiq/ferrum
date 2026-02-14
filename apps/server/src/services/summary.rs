//! FHIR Search Result Parameter filtering
//!
//! Handles filtering resources according to FHIR spec 3.2.1.7:
//! - _summary: Return only portions of resources based on pre-defined levels
//! - _elements: Request that only a specific set of elements be returned
//!
//! These are Search Result Parameters that modify search results,
//! not content negotiation parameters.

use crate::db::search::params::SummaryMode;
use lru::LruCache;
use serde_json::{Map, Value as JsonValue};
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use ferrum_context::FhirContext;

/// Cached summary element information for a resource type
#[derive(Clone, Debug)]
struct SummaryElements {
    /// Element paths where isSummary=true
    summary_paths: HashSet<String>,
    /// Mandatory top-level element paths (min > 0)
    mandatory_paths: HashSet<String>,
    /// Modifier element paths (isModifier=true)
    modifier_paths: HashSet<String>,
}

/// Service that filters resources according to _summary and _elements parameters
pub struct SummaryFilter {
    fhir_context: Arc<dyn FhirContext>,
    cache: Mutex<LruCache<String, Arc<SummaryElements>>>,
}

impl SummaryFilter {
    pub fn new(fhir_context: Arc<dyn FhirContext>) -> Self {
        let capacity = NonZeroUsize::new(256).unwrap();
        Self {
            fhir_context,
            cache: Mutex::new(LruCache::new(capacity)),
        }
    }

    /// Apply summary filtering to a resource
    pub fn filter_resource(
        &self,
        mut resource: JsonValue,
        mode: SummaryMode,
    ) -> crate::Result<JsonValue> {
        match mode {
            SummaryMode::False => Ok(resource), // No filtering
            SummaryMode::Count => Ok(resource), // Handled elsewhere
            SummaryMode::Data => {
                // Remove text element
                if let Some(obj) = resource.as_object_mut() {
                    obj.remove("text");
                }
                Self::add_subsetted_tag(&mut resource);
                Ok(resource)
            }
            SummaryMode::Text => {
                // Keep: id, meta, text, top-level mandatory elements
                let filtered = self.filter_text_mode(&resource)?;
                Ok(filtered)
            }
            SummaryMode::True => {
                // Keep only isSummary elements
                let filtered = self.filter_summary_mode(&resource)?;
                Ok(filtered)
            }
        }
    }

    /// Apply elements filtering to a resource
    /// Per FHIR spec 3.2.1.7.6:
    /// - Include requested elements
    /// - Always include mandatory elements (min > 0)
    /// - Always include modifier elements that have values
    pub fn filter_elements(
        &self,
        resource: JsonValue,
        elements: &[String],
    ) -> crate::Result<JsonValue> {
        let Some(obj) = resource.as_object() else {
            return Ok(resource);
        };

        // Validate element names (spec: SHALL be base element name, no [x] or type suffixes)
        for element in elements {
            let element_name = if let Some((_, name)) = element.split_once('.') {
                name
            } else {
                element.as_str()
            };

            if element_name.contains("[x]") || element_name.contains('[') {
                return Err(crate::Error::Validation(format!(
                    "Invalid _elements value '{}': SHALL be base element name without [x] notation",
                    element
                )));
            }
        }

        let mut filtered = Map::new();

        // Always include resourceType, id, and meta (FHIR base mandatory elements)
        const ALWAYS_INCLUDE: &[&str] = &["resourceType", "id", "meta"];
        for field in ALWAYS_INCLUDE {
            if let Some(value) = obj.get(*field) {
                filtered.insert(field.to_string(), value.clone());
            }
        }

        // Get resource type and load element metadata
        let resource_type = obj.get("resourceType").and_then(|v| v.as_str());
        let element_info = if let Some(rt) = resource_type {
            self.get_or_load_summary_elements(rt).ok()
        } else {
            None
        };

        // Include all mandatory elements (per spec: SHOULD return whether requested or not)
        if let Some(ref info) = element_info {
            for mandatory_element in &info.mandatory_paths {
                if let Some(value) = obj.get(mandatory_element) {
                    filtered.insert(mandatory_element.clone(), value.clone());
                }
            }
        }

        // Include all modifier elements that have values (per spec)
        if let Some(ref info) = element_info {
            for modifier_element in &info.modifier_paths {
                if let Some(value) = obj.get(modifier_element) {
                    filtered.insert(modifier_element.clone(), value.clone());
                }
            }
        }

        // Include requested elements
        for element in elements {
            // Handle ResourceType.element notation (e.g., "Patient.name")
            let key = if let Some((rt, name)) = element.split_once('.') {
                if resource_type == Some(rt) {
                    name
                } else {
                    continue; // Skip elements for other resource types
                }
            } else {
                element.as_str()
            };

            if let Some(value) = obj.get(key) {
                filtered.insert(key.to_string(), value.clone());
            }
        }

        let mut result = JsonValue::Object(filtered);
        Self::add_subsetted_tag(&mut result);
        Ok(result)
    }

    /// Filter for _summary=text mode
    fn filter_text_mode(&self, resource: &JsonValue) -> crate::Result<JsonValue> {
        let Some(obj) = resource.as_object() else {
            return Ok(resource.clone());
        };

        let mut filtered = Map::new();

        // Always keep these elements
        const ALWAYS_KEEP: &[&str] = &["resourceType", "id", "meta", "text"];
        for key in ALWAYS_KEEP {
            if let Some(value) = obj.get(*key) {
                filtered.insert(key.to_string(), value.clone());
            }
        }

        // Get resource type
        let Some(resource_type) = obj.get("resourceType").and_then(|v| v.as_str()) else {
            return Ok(JsonValue::Object(filtered));
        };

        // Get mandatory top-level elements for this resource type
        let elements = self.get_or_load_summary_elements(resource_type)?;
        for path in &elements.mandatory_paths {
            if let Some(value) = obj.get(path) {
                filtered.insert(path.clone(), value.clone());
            }
        }

        let mut result = JsonValue::Object(filtered);
        Self::add_subsetted_tag(&mut result);
        Ok(result)
    }

    /// Filter for _summary=true mode
    fn filter_summary_mode(&self, resource: &JsonValue) -> crate::Result<JsonValue> {
        let Some(obj) = resource.as_object() else {
            return Ok(resource.clone());
        };

        // Get resource type
        let Some(resource_type) = obj.get("resourceType").and_then(|v| v.as_str()) else {
            return Ok(resource.clone());
        };

        // Get summary elements for this resource type
        let elements = self.get_or_load_summary_elements(resource_type)?;

        let mut filtered = Map::new();

        // Always keep resourceType even if not in summary
        if let Some(value) = obj.get("resourceType") {
            filtered.insert("resourceType".to_string(), value.clone());
        }

        // Keep summary elements
        for path in &elements.summary_paths {
            if let Some(value) = obj.get(path) {
                filtered.insert(path.clone(), value.clone());
            }
        }

        let mut result = JsonValue::Object(filtered);
        Self::add_subsetted_tag(&mut result);
        Ok(result)
    }

    /// Get or load summary elements for a resource type
    fn get_or_load_summary_elements(
        &self,
        resource_type: &str,
    ) -> crate::Result<Arc<SummaryElements>> {
        // Check cache first
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(elements) = cache.get(resource_type) {
                return Ok(Arc::clone(elements));
            }
        }

        // Load from StructureDefinition
        let elements = self.load_summary_elements(resource_type)?;
        let elements = Arc::new(elements);

        // Cache it
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(resource_type.to_string(), elements.clone());
        }

        Ok(elements)
    }

    /// Load summary elements from StructureDefinition
    fn load_summary_elements(&self, resource_type: &str) -> crate::Result<SummaryElements> {
        let sd = self
            .fhir_context
            .get_core_structure_definition_by_type(resource_type)
            .map_err(|e| crate::Error::FhirContext(e.to_string()))?;

        let Some(sd) = sd else {
            // No StructureDefinition found, return minimal set
            return Ok(SummaryElements {
                summary_paths: HashSet::from_iter(vec!["id".to_string(), "meta".to_string()]),
                mandatory_paths: HashSet::new(),
                modifier_paths: HashSet::new(),
            });
        };

        let mut summary_paths = HashSet::new();
        let mut mandatory_paths = HashSet::new();
        let mut modifier_paths = HashSet::new();

        // Extract elements from StructureDefinition
        if let Some(elements) = &sd.snapshot {
            for element in &elements.element {
                let path = &element.path;

                // Only include top-level elements (no dots except for resourceType.field)
                let parts: Vec<&str> = path.split('.').collect();

                if parts.len() != 2 {
                    continue; // Skip nested elements
                }

                let element_name = parts[1];

                // Check if isSummary
                if element.is_summary.unwrap_or(false) {
                    summary_paths.insert(element_name.to_string());
                }

                // Check if mandatory (min > 0)
                if element.min.unwrap_or(0) > 0 {
                    mandatory_paths.insert(element_name.to_string());
                }

                // Check if modifier element (isModifier=true)
                if element.is_modifier.unwrap_or(false) {
                    modifier_paths.insert(element_name.to_string());
                }
            }
        }

        // Always include these core elements in summary
        summary_paths.insert("id".to_string());
        summary_paths.insert("meta".to_string());

        Ok(SummaryElements {
            summary_paths,
            mandatory_paths,
            modifier_paths,
        })
    }

    /// Add SUBSETTED tag to resource meta
    fn add_subsetted_tag(resource: &mut JsonValue) {
        let obj = match resource.as_object_mut() {
            Some(obj) => obj,
            None => return,
        };

        // Get or create meta
        let meta = obj
            .entry("meta")
            .or_insert_with(|| JsonValue::Object(Map::new()));

        let meta_obj = match meta.as_object_mut() {
            Some(obj) => obj,
            None => return,
        };

        // Get or create tag array
        let tags = meta_obj
            .entry("tag")
            .or_insert_with(|| JsonValue::Array(vec![]));

        let tags_array = match tags.as_array_mut() {
            Some(arr) => arr,
            None => return,
        };

        // Check if SUBSETTED tag already exists
        let has_subsetted = tags_array.iter().any(|tag| {
            tag.get("system").and_then(|v| v.as_str())
                == Some("http://terminology.hl7.org/CodeSystem/v3-ObservationValue")
                && tag.get("code").and_then(|v| v.as_str()) == Some("SUBSETTED")
        });

        if !has_subsetted {
            tags_array.push(serde_json::json!({
                "system": "http://terminology.hl7.org/CodeSystem/v3-ObservationValue",
                "code": "SUBSETTED"
            }));
        }
    }
}
