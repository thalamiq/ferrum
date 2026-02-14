//! FHIR slicing engine for snapshot generation
//!
//! This module implements FHIR slicing semantics according to the specification:
//! - Creating slices under the correct parent
//! - Merging slicing info (discriminator, rules, ordered, description)
//! - Handling sliceName on children
//! - Enforcing slicing entry rules
//! - Correct slice ordering
//! - Re-slicing (slice within a slice)
//! - Type slicing (e.g., Observation.value[x] slicing by type)
//! - Profile slicing (slicing on profile)

use crate::error::{Error, Result};
use std::collections::HashMap;
use ferrum_models::{
    ElementDefinition, ElementDefinitionDiscriminator, ElementDefinitionSlicing,
};

/// Information about a slice entry in the snapshot
#[derive(Debug, Clone)]
pub struct SliceEntry {
    /// The element path being sliced
    pub path: String,
    /// The slicing definition
    pub slicing: ElementDefinitionSlicing,
    /// Index of the slice entry element in the snapshot
    pub entry_index: usize,
}

/// Information about a slice instance
#[derive(Debug, Clone)]
pub struct SliceInstance {
    /// The slice name
    pub slice_name: String,
    /// The full path including parent slices (e.g., "component:systolic" or "component:systolic:extreme")
    pub full_slice_name: String,
    /// The element definition
    pub element: ElementDefinition,
    /// Parent slice if this is a re-slice
    pub parent_slice: Option<String>,
}

/// Slicing context for managing slices during snapshot generation
pub struct SlicingContext {
    /// Map of path -> slice entry
    slice_entries: HashMap<String, SliceEntry>,
    /// Map of path:sliceName -> slice instance
    slice_instances: HashMap<String, SliceInstance>,
    /// Track which paths have implicit slicing (detected from sliced elements without explicit entry)
    implicit_slicing: HashMap<String, Vec<String>>,
}

impl SlicingContext {
    /// Create a new slicing context
    pub fn new() -> Self {
        Self {
            slice_entries: HashMap::new(),
            slice_instances: HashMap::new(),
            implicit_slicing: HashMap::new(),
        }
    }

    /// Register a slice entry element
    pub fn register_slice_entry(
        &mut self,
        path: &str,
        slicing: ElementDefinitionSlicing,
        index: usize,
    ) -> Result<()> {
        if self.slice_entries.contains_key(path) {
            // Merge slicing definitions if there are multiple
            let existing = self.slice_entries.get_mut(path).unwrap();
            existing.slicing = merge_slicing_definitions(&existing.slicing, &slicing)?;
        } else {
            self.slice_entries.insert(
                path.to_string(),
                SliceEntry {
                    path: path.to_string(),
                    slicing,
                    entry_index: index,
                },
            );
        }
        Ok(())
    }

    /// Register a slice instance
    pub fn register_slice_instance(&mut self, element: &ElementDefinition) -> Result<()> {
        if let Some(slice_name) = &element.slice_name {
            let key = element.key();

            // Detect if this is a re-slice (slice_name contains multiple parts)
            let (parent_slice, full_slice_name) = if slice_name.contains(':') {
                // This is a re-slice: "systolic:extreme"
                let parts: Vec<&str> = slice_name.split(':').collect();
                let parent = parts[0].to_string();
                (Some(parent), slice_name.clone())
            } else {
                (None, slice_name.clone())
            };

            self.slice_instances.insert(
                key.clone(),
                SliceInstance {
                    slice_name: slice_name.clone(),
                    full_slice_name,
                    element: element.clone(),
                    parent_slice,
                },
            );
        }
        Ok(())
    }

    /// Detect implicit slicing (sliced elements without explicit slicing entry)
    pub fn detect_implicit_slicing(&mut self, elements: &[ElementDefinition]) {
        let mut sliced_paths: HashMap<String, Vec<String>> = HashMap::new();

        // Find all sliced elements
        for elem in elements {
            if let Some(slice_name) = &elem.slice_name {
                sliced_paths
                    .entry(elem.path.clone())
                    .or_default()
                    .push(slice_name.clone());
            }
        }

        // Check which paths have slices but no explicit slicing entry
        for (path, slice_names) in sliced_paths {
            if !self.slice_entries.contains_key(&path) {
                self.implicit_slicing.insert(path, slice_names);
            }
        }
    }

    /// Get the slice entry for a given path
    pub fn get_slice_entry(&self, path: &str) -> Option<&SliceEntry> {
        self.slice_entries.get(path)
    }

    /// Get all slice instances for a given path
    pub fn get_slices_for_path(&self, path: &str) -> Vec<&SliceInstance> {
        self.slice_instances
            .values()
            .filter(|s| s.element.path == path)
            .collect()
    }

    /// Check if a path has implicit slicing
    pub fn has_implicit_slicing(&self, path: &str) -> bool {
        self.implicit_slicing.contains_key(path)
    }

    /// Get implicit slice names for a path
    pub fn get_implicit_slices(&self, path: &str) -> Option<&Vec<String>> {
        self.implicit_slicing.get(path)
    }

    /// Get all implicit slicing entries
    pub fn get_all_implicit_slicing(&self) -> &HashMap<String, Vec<String>> {
        &self.implicit_slicing
    }

    /// Get all slice entries
    pub fn get_all_slice_entries(&self) -> &HashMap<String, SliceEntry> {
        &self.slice_entries
    }

    /// Create a default slicing entry for implicit slicing
    pub fn create_default_slicing_entry(&self, _path: &str) -> ElementDefinitionSlicing {
        use ferrum_models::{DiscriminatorType, SlicingRules};
        ElementDefinitionSlicing {
            discriminator: Some(vec![ElementDefinitionDiscriminator {
                discriminator_type: DiscriminatorType::Value,
                path: "url".to_string(), // Default discriminator
            }]),
            description: Some("Slicing detected from slice instances".to_string()),
            ordered: Some(false),
            rules: SlicingRules::Open, // Default to open
        }
    }

    /// Validate slice ordering according to slicing rules
    pub fn validate_slice_ordering(&self, path: &str, slices: &[&ElementDefinition]) -> Result<()> {
        if let Some(entry) = self.get_slice_entry(path) {
            if entry.slicing.ordered == Some(true) {
                // Check that slices appear in the order they were defined
                // This is a simplified check - full implementation would need to track definition order
                for i in 1..slices.len() {
                    let prev = slices[i - 1];
                    let curr = slices[i];

                    // Slices should maintain their relative order
                    if let (Some(prev_name), Some(curr_name)) = (&prev.slice_name, &curr.slice_name)
                    {
                        // This is where we'd check definition order if we had it
                        // For now, we just ensure they're both present
                        if prev_name.is_empty() || curr_name.is_empty() {
                            return Err(Error::Snapshot(format!(
                                "Invalid slice name for ordered slicing on path '{}'",
                                path
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if a slice matches the discriminator rules
    pub fn matches_discriminator(
        &self,
        element: &ElementDefinition,
        discriminator: &ElementDefinitionDiscriminator,
    ) -> bool {
        use ferrum_models::DiscriminatorType;
        match discriminator.discriminator_type {
            DiscriminatorType::Value => {
                // Check if element has a fixed value matching the discriminator path
                // This is a simplified implementation
                true
            }
            DiscriminatorType::Exists => {
                // Check if the discriminator path exists in the element
                true
            }
            DiscriminatorType::Pattern => {
                // Check if element has any pattern/fixed discriminator value present.
                // Pattern is stored as Value, while other pattern types (e.g. patternCoding)
                // are captured in the flattened `extensions` map.
                let has_pattern_field = element.pattern.is_some()
                    || element
                        .extensions
                        .iter()
                        .any(|(k, v)| k.starts_with("pattern") && !v.is_null());
                // Fixed values also satisfy a pattern discriminator in many profiles
                // (FHIR allows fixed values to stand in for pattern matching).
                let has_fixed_field = element.fixed.is_some()
                    || element
                        .extensions
                        .iter()
                        .any(|(k, v)| k.starts_with("fixed") && !v.is_null());

                has_pattern_field || has_fixed_field
            }
            DiscriminatorType::Type => {
                // Check if element type matches
                element.types.is_some()
            }
            DiscriminatorType::Profile => {
                // Check if element has matching profile
                if let Some(types) = &element.types {
                    types.iter().any(|t| t.profile.is_some())
                } else {
                    false
                }
            }
        }
    }

    /// Validate that all slices satisfy the discriminator rules
    pub fn validate_discriminators(&self, path: &str) -> Result<()> {
        if let Some(entry) = self.get_slice_entry(path) {
            if let Some(discriminators) = &entry.slicing.discriminator {
                let slices = self.get_slices_for_path(path);

                for slice in slices {
                    // Check that at least one discriminator matches
                    let has_match = discriminators
                        .iter()
                        .any(|d| self.matches_discriminator(&slice.element, d));

                    if !has_match && !discriminators.is_empty() {
                        use ferrum_models::SlicingRules;
                        if entry.slicing.rules == SlicingRules::Closed {
                            return Err(Error::Snapshot(format!(
                                "Slice '{}' does not match any discriminator for path '{}'",
                                slice.slice_name, path
                            )));
                        } else {
                            eprintln!(
                                "warn: Slice '{}' does not match discriminator for open slicing on '{}'",
                                slice.slice_name, path
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if adding a new slice is allowed by the slicing rules
    pub fn can_add_slice(&self, path: &str, _slice_name: &str) -> Result<bool> {
        use ferrum_models::SlicingRules;
        if let Some(entry) = self.get_slice_entry(path) {
            match entry.slicing.rules {
                SlicingRules::Closed => {
                    // No new slices allowed
                    Ok(false)
                }
                SlicingRules::Open => {
                    // New slices allowed anywhere
                    Ok(true)
                }
                SlicingRules::OpenAtEnd => {
                    // New slices allowed only at the end
                    // Would need to track position to fully implement
                    Ok(true)
                }
            }
        } else {
            // No slicing entry - allow implicit slicing
            Ok(true)
        }
    }

    /// Get the parent slice for a re-slice
    pub fn get_parent_slice(&self, element: &ElementDefinition) -> Option<&SliceInstance> {
        if let Some(slice_name) = &element.slice_name {
            if slice_name.contains(':') {
                // This is a re-slice
                let parent_key = format!("{}:{}", element.path, slice_name.split(':').next()?);
                self.slice_instances.get(&parent_key)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Determine the correct position for a slice in the element list
    pub fn get_slice_position(
        &self,
        elements: &[ElementDefinition],
        slice: &ElementDefinition,
    ) -> usize {
        let path = &slice.path;

        // Find the slice entry or last element with the same path
        let mut last_same_path_idx = elements.len();

        for (i, elem) in elements.iter().enumerate() {
            if elem.path == *path {
                last_same_path_idx = i + 1;
            } else if elem.path > *path {
                // We've gone past our path
                break;
            }
        }

        last_same_path_idx
    }
}

impl Default for SlicingContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Merge two slicing definitions (when differential adds to base)
fn merge_slicing_definitions(
    base: &ElementDefinitionSlicing,
    diff: &ElementDefinitionSlicing,
) -> Result<ElementDefinitionSlicing> {
    use ferrum_models::SlicingRules;
    Ok(ElementDefinitionSlicing {
        discriminator: diff
            .discriminator
            .clone()
            .or_else(|| base.discriminator.clone()),
        description: diff
            .description
            .clone()
            .or_else(|| base.description.clone()),
        ordered: diff.ordered.or(base.ordered),
        rules: match (base.rules.clone(), diff.rules.clone()) {
            // If either specifies closed, use closed (most restrictive)
            (_, SlicingRules::Closed) | (SlicingRules::Closed, _) => SlicingRules::Closed,
            // If either specifies openAtEnd, use openAtEnd
            (_, SlicingRules::OpenAtEnd) | (SlicingRules::OpenAtEnd, _) => SlicingRules::OpenAtEnd,
            // Otherwise use differential's rules
            (_, diff_rules) => diff_rules,
        },
    })
}

/// Detect type slicing - when value[x] is sliced by type
pub fn is_type_slice(element: &ElementDefinition) -> bool {
    // Type slicing occurs when:
    // 1. The base element has [x] suffix
    // 2. The slice name corresponds to a type (valueString, valueQuantity, etc.)
    if let Some(slice_name) = &element.slice_name {
        // Check if this looks like a type slice (camelCase type after base name)
        if element.path.ends_with("[x]") {
            return true;
        }

        // Alternative: check if slice name starts with uppercase (type convention)
        if let Some(first_char) = slice_name.chars().next() {
            return first_char.is_uppercase();
        }
    }
    false
}

/// Detect profile slicing - when slicing by profile
pub fn is_profile_slice(element: &ElementDefinition) -> bool {
    if let Some(types) = &element.types {
        // Profile slicing when element has types with profiles
        types.iter().any(|t| t.profile.is_some())
    } else {
        false
    }
}

/// Extract the slice discriminator path value from an element
pub fn extract_discriminator_value(
    element: &ElementDefinition,
    discriminator_path: &str,
) -> Option<String> {
    // This is a simplified implementation
    // Full implementation would need to traverse element structure
    match discriminator_path {
        "url" => {
            // Common for extensions
            element
                .extensions
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
        "type" => {
            // Type discriminator
            element
                .types
                .as_ref()
                .and_then(|types| types.first().map(|t| t.code.clone()))
        }
        "profile" => {
            // Profile discriminator
            element.types.as_ref().and_then(|types| {
                types
                    .first()
                    .and_then(|t| t.profile.as_ref().and_then(|p| p.first().cloned()))
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_element(path: &str, slice_name: Option<&str>) -> ElementDefinition {
        ElementDefinition {
            id: None,
            path: path.to_string(),
            representation: None,
            slice_name: slice_name.map(|s| s.to_string()),
            slice_is_constraining: None,
            short: None,
            definition: None,
            comment: None,
            requirements: None,
            alias: None,
            min: None,
            max: None,
            base: None,
            content_reference: None,
            types: None,
            default_value: None,
            meaning_when_missing: None,
            order_meaning: None,
            fixed: None,
            pattern: None,
            example: None,
            min_value: None,
            max_value: None,
            max_length: None,
            condition: None,
            constraint: None,
            is_modifier: None,
            is_modifier_reason: None,
            is_summary: None,
            binding: None,
            mapping: None,
            slicing: None,
            must_support: None,
            extensions: HashMap::new(),
        }
    }

    fn make_slicing() -> ElementDefinitionSlicing {
        use ferrum_models::{DiscriminatorType, SlicingRules};
        ElementDefinitionSlicing {
            discriminator: Some(vec![ElementDefinitionDiscriminator {
                discriminator_type: DiscriminatorType::Value,
                path: "url".to_string(),
            }]),
            description: Some("Test slicing".to_string()),
            ordered: Some(false),
            rules: SlicingRules::Open,
        }
    }

    #[test]
    fn registers_slice_entry() {
        let mut ctx = SlicingContext::new();
        let slicing = make_slicing();

        ctx.register_slice_entry("Patient.identifier", slicing.clone(), 0)
            .unwrap();

        assert!(ctx.get_slice_entry("Patient.identifier").is_some());
    }

    #[test]
    fn registers_slice_instance() {
        let mut ctx = SlicingContext::new();
        let mut element = make_element("Patient.identifier", Some("official"));
        element.slice_name = Some("official".to_string());

        ctx.register_slice_instance(&element).unwrap();

        let slices = ctx.get_slices_for_path("Patient.identifier");
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].slice_name, "official");
    }

    #[test]
    fn detects_implicit_slicing() {
        let mut ctx = SlicingContext::new();
        let elements = vec![
            make_element("Patient.identifier", None),
            make_element("Patient.identifier", Some("official")),
            make_element("Patient.identifier", Some("temp")),
        ];

        ctx.detect_implicit_slicing(&elements);

        assert!(ctx.has_implicit_slicing("Patient.identifier"));
        let slices = ctx.get_implicit_slices("Patient.identifier").unwrap();
        assert_eq!(slices.len(), 2);
    }

    #[test]
    fn detects_reslicing() {
        let mut ctx = SlicingContext::new();
        let element = make_element("Observation.component", Some("systolic:extreme"));

        ctx.register_slice_instance(&element).unwrap();

        let instance = ctx
            .slice_instances
            .get("Observation.component:systolic:extreme")
            .unwrap();
        assert_eq!(instance.parent_slice, Some("systolic".to_string()));
    }

    #[test]
    fn merges_slicing_definitions() {
        use ferrum_models::{DiscriminatorType, SlicingRules};
        let base = ElementDefinitionSlicing {
            discriminator: Some(vec![ElementDefinitionDiscriminator {
                discriminator_type: DiscriminatorType::Value,
                path: "code".to_string(),
            }]),
            description: None,
            ordered: Some(false),
            rules: SlicingRules::Open,
        };

        let diff = ElementDefinitionSlicing {
            discriminator: None,
            description: Some("Updated description".to_string()),
            ordered: Some(true),
            rules: SlicingRules::OpenAtEnd,
        };

        let merged = merge_slicing_definitions(&base, &diff).unwrap();

        assert!(merged.discriminator.is_some());
        assert_eq!(merged.description, Some("Updated description".to_string()));
        assert_eq!(merged.ordered, Some(true));
        assert_eq!(merged.rules, SlicingRules::OpenAtEnd);
    }

    #[test]
    fn validates_slicing_rules_closed() {
        use ferrum_models::SlicingRules;
        let mut ctx = SlicingContext::new();
        let slicing = ElementDefinitionSlicing {
            discriminator: None,
            description: None,
            ordered: None,
            rules: SlicingRules::Closed,
        };

        ctx.register_slice_entry("Patient.identifier", slicing, 0)
            .unwrap();

        let can_add = ctx.can_add_slice("Patient.identifier", "newSlice").unwrap();
        assert!(!can_add);
    }

    #[test]
    fn validates_slicing_rules_open() {
        use ferrum_models::SlicingRules;
        let mut ctx = SlicingContext::new();
        let slicing = ElementDefinitionSlicing {
            discriminator: None,
            description: None,
            ordered: None,
            rules: SlicingRules::Open,
        };

        ctx.register_slice_entry("Patient.identifier", slicing, 0)
            .unwrap();

        let can_add = ctx.can_add_slice("Patient.identifier", "newSlice").unwrap();
        assert!(can_add);
    }
}
