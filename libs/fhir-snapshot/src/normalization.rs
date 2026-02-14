//! ID and slice name normalization for FHIR elements
//!
//! This module handles the normalization of element IDs and slice names
//! according to FHIR specification rules.

use crate::merge::cleanup_fixed_field;
use ferrum_models::{Differential, ElementDefinition, Snapshot};

/// Normalize IDs and slice names in a snapshot
///
/// FHIR rules:
/// - If sliceName changes, ID must change
/// - If ID is missing, generate from path
/// - For slices, ID should be "path:sliceName"
pub fn normalize_snapshot(snapshot: &mut Snapshot) {
    for element in &mut snapshot.element {
        normalize_element_id(element);
        // Clean up fixed field (move extension data, normalize empty objects)
        cleanup_fixed_field(element);
    }
}

/// Normalize IDs and slice names in a differential
pub fn normalize_differential(differential: &mut Differential) {
    for element in &mut differential.element {
        normalize_element_id(element);
        // Clean up fixed field (move extension data, normalize empty objects)
        cleanup_fixed_field(element);
    }
}

/// Normalize a single element's ID based on path and slice name
fn normalize_element_id(element: &mut ElementDefinition) {
    match &element.slice_name {
        Some(slice_name) => {
            // For slices, ID should be "path:sliceName"
            let expected_id = format!("{}:{}", element.path, slice_name);
            if element.id.as_ref() != Some(&expected_id) {
                element.id = Some(expected_id);
            }
        }
        None => {
            // For non-slices, ID should match path if not already set
            if element.id.is_none() {
                element.id = Some(element.path.clone());
            }
        }
    }
}

/// Generate a slice name from an ID
///
/// If the ID is in the format "path:sliceName", extract the slice name
pub fn extract_slice_name_from_id(id: &str, path: &str) -> Option<String> {
    if id.starts_with(path) && id.len() > path.len() + 1 {
        let suffix = &id[path.len()..];
        if let Some(stripped) = suffix.strip_prefix(':') {
            return Some(stripped.to_string());
        }
    }
    None
}

/// Validate that ID and slice name are consistent
pub fn validate_id_slice_consistency(element: &ElementDefinition) -> bool {
    match (&element.id, &element.slice_name) {
        (Some(id), Some(slice_name)) => {
            let expected_id = format!("{}:{}", element.path, slice_name);
            id == &expected_id
        }
        (Some(id), None) => {
            // ID should not contain a colon for non-slices
            // or should equal the path
            id == &element.path || !id.contains(':')
        }
        (None, _) => {
            // Missing ID is OK, will be normalized
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_element(path: &str, id: Option<&str>, slice_name: Option<&str>) -> ElementDefinition {
        ElementDefinition {
            id: id.map(|s| s.to_string()),
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

    #[test]
    fn normalizes_slice_id() {
        let mut element = make_element("Patient.name", None, Some("official"));
        normalize_element_id(&mut element);

        assert_eq!(element.id, Some("Patient.name:official".to_string()));
    }

    #[test]
    fn normalizes_non_slice_id() {
        let mut element = make_element("Patient.name", None, None);
        normalize_element_id(&mut element);

        assert_eq!(element.id, Some("Patient.name".to_string()));
    }

    #[test]
    fn preserves_correct_slice_id() {
        let mut element = make_element(
            "Patient.name",
            Some("Patient.name:official"),
            Some("official"),
        );
        normalize_element_id(&mut element);

        assert_eq!(element.id, Some("Patient.name:official".to_string()));
    }

    #[test]
    fn corrects_incorrect_slice_id() {
        let mut element = make_element("Patient.name", Some("Patient.name"), Some("official"));
        normalize_element_id(&mut element);

        assert_eq!(element.id, Some("Patient.name:official".to_string()));
    }

    #[test]
    fn extracts_slice_name_from_id() {
        let slice_name = extract_slice_name_from_id("Patient.name:official", "Patient.name");
        assert_eq!(slice_name, Some("official".to_string()));
    }

    #[test]
    fn extracts_none_for_non_slice_id() {
        let slice_name = extract_slice_name_from_id("Patient.name", "Patient.name");
        assert_eq!(slice_name, None);
    }

    #[test]
    fn validates_consistent_id_slice() {
        let element = make_element(
            "Patient.name",
            Some("Patient.name:official"),
            Some("official"),
        );
        assert!(validate_id_slice_consistency(&element));
    }

    #[test]
    fn validates_inconsistent_id_slice() {
        let element = make_element("Patient.name", Some("Patient.name:wrong"), Some("official"));
        assert!(!validate_id_slice_consistency(&element));
    }

    #[test]
    fn normalizes_snapshot() {
        let mut snapshot = Snapshot {
            element: vec![
                make_element("Patient", None, None),
                make_element("Patient.name", None, None),
                make_element("Patient.name", None, Some("official")),
            ],
        };

        normalize_snapshot(&mut snapshot);

        assert_eq!(snapshot.element[0].id, Some("Patient".to_string()));
        assert_eq!(snapshot.element[1].id, Some("Patient.name".to_string()));
        assert_eq!(
            snapshot.element[2].id,
            Some("Patient.name:official".to_string())
        );
    }

    #[test]
    fn normalizes_differential() {
        let mut differential = Differential {
            element: vec![
                make_element("Patient.name", None, None),
                make_element("Patient.name", None, Some("official")),
            ],
        };

        normalize_differential(&mut differential);

        assert_eq!(differential.element[0].id, Some("Patient.name".to_string()));
        assert_eq!(
            differential.element[1].id,
            Some("Patient.name:official".to_string())
        );
    }
}
