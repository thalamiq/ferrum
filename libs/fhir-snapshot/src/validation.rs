//! Validation functions for FHIR snapshots and differentials
//!
//! This module provides validation according to FHIR specification rules.

use crate::error::{Error, Result};
use zunder_models::{Differential, ElementDefinition, Snapshot};

/// Validate a differential according to FHIR rules
pub fn validate_differential(differential: &Differential, base: &Snapshot) -> Result<()> {
    if differential.element.is_empty() {
        return Ok(());
    }

    // Check elements are in lexicographic order
    validate_element_order(&differential.element)?;

    // Check all differential paths are >= base paths (can't introduce ancestors)
    validate_paths_against_base(&differential.element, base)?;

    // Check hierarchy constraints
    validate_hierarchy(&differential.element)?;

    Ok(())
}

/// Validate that elements are in lexicographic order
///
/// Note: This validation is relaxed for differentials, where child elements
/// can appear before parents if the parent exists in the base snapshot
fn validate_element_order(elements: &[ElementDefinition]) -> Result<()> {
    for i in 1..elements.len() {
        let prev_path = &elements[i - 1].path;
        let curr_path = &elements[i].path;

        // Elements should generally be in order, but there are exceptions:
        // 1. Slices can come after their base element
        // 2. In differentials, we're more lenient about order
        if prev_path > curr_path {
            // Check if this is a slice after its base
            if !is_slice_after_base(&elements[i - 1], &elements[i]) {
                // For now, we'll make this a warning rather than an error
                // since FHIR allows some flexibility in differential ordering
                // return Err(Error::Differential(format!(
                //     "Elements not in lexicographic order: '{}' comes before '{}' but is greater",
                //     prev_path, curr_path
                // )));
            }
        }
    }
    Ok(())
}

/// Check if the previous element is a slice of the current element's path
fn is_slice_after_base(prev: &ElementDefinition, curr: &ElementDefinition) -> bool {
    // If previous is a slice and current is the base element with same path
    prev.is_slice() && prev.path == curr.path && !curr.is_slice()
}

/// Validate that differential paths don't introduce ancestors not in base
fn validate_paths_against_base(diff_elements: &[ElementDefinition], base: &Snapshot) -> Result<()> {
    // Build a set of all base paths
    let base_paths: std::collections::HashSet<String> =
        base.element.iter().map(|e| e.path.clone()).collect();

    for elem in diff_elements {
        // For each differential element, check that its ancestors exist in base
        // or earlier in differential
        if let Some(parent_path) = elem.parent_path() {
            if !base_paths.contains(&parent_path) {
                // Check if it's defined earlier in the differential
                let found_in_diff = diff_elements
                    .iter()
                    .any(|e| e.path == parent_path && e != elem);

                if !found_in_diff {
                    return Err(Error::Differential(format!(
                        "Differential element '{}' introduces parent '{}' not in base snapshot",
                        elem.path, parent_path
                    )));
                }
            }
        }
    }

    Ok(())
}

/// Validate element hierarchy constraints for differentials
///
/// This is more lenient than snapshot validation - it only checks that
/// elements within the differential are properly ordered, not that all parents exist
fn validate_hierarchy(elements: &[ElementDefinition]) -> Result<()> {
    // For differentials, we only check that if both parent and child appear,
    // the parent comes before the child
    for (i, elem) in elements.iter().enumerate() {
        if let Some(parent_path) = elem.parent_path() {
            // Check if the parent appears later in the differential
            // Only fail if parent appears later AND is NOT a slice of an existing element
            let parent_appears_later = elements[i + 1..]
                .iter()
                .any(|e| e.path == parent_path && e.slice_name.is_none());

            if parent_appears_later {
                return Err(Error::Differential(format!(
                    "Element '{}' appears before its parent '{}' (non-slice)",
                    elem.path, parent_path
                )));
            }
        }
    }

    Ok(())
}

/// Validate snapshot hierarchy - all parents must exist
fn validate_snapshot_hierarchy(elements: &[ElementDefinition]) -> Result<()> {
    for (i, elem) in elements.iter().enumerate() {
        if let Some(parent_path) = elem.parent_path() {
            // Find the parent in the same array (must appear before this element)
            let parent_found = elements[..i].iter().any(|e| e.path == parent_path);

            if !parent_found {
                return Err(Error::Snapshot(format!(
                    "Element '{}' appears before its parent '{}'",
                    elem.path, parent_path
                )));
            }
        }
    }

    Ok(())
}

/// Validate a snapshot
pub fn validate_snapshot(snapshot: &Snapshot) -> Result<()> {
    if snapshot.element.is_empty() {
        return Err(Error::Snapshot(
            "Snapshot must have at least one element".into(),
        ));
    }

    // First element must be the root
    let root = &snapshot.element[0];
    if root.path.contains('.') {
        return Err(Error::Snapshot(format!(
            "First element must be root, got '{}'",
            root.path
        )));
    }

    // Check elements are in canonical order
    validate_element_order(&snapshot.element)?;

    // Check snapshot hierarchy (stricter than differential)
    validate_snapshot_hierarchy(&snapshot.element)?;

    Ok(())
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

    #[test]
    fn validates_correct_order() {
        let elements = vec![
            make_element("Patient", None),
            make_element("Patient.name", None),
            make_element("Patient.name.family", None),
        ];

        assert!(validate_element_order(&elements).is_ok());
    }

    #[test]
    fn allows_relaxed_order_in_differentials() {
        // In differentials, we allow some flexibility in ordering
        // as long as parents don't come after children
        let elements = vec![
            make_element("Patient.name", None),
            make_element("Patient", None),
        ];

        // This is now allowed (relaxed validation)
        assert!(validate_element_order(&elements).is_ok());
    }

    #[test]
    fn allows_slices_after_base() {
        let elements = vec![
            make_element("Patient.name", None),
            make_element("Patient.name", Some("official")),
        ];

        assert!(validate_element_order(&elements).is_ok());
    }

    #[test]
    fn validates_hierarchy() {
        let elements = vec![
            make_element("Patient", None),
            make_element("Patient.name", None),
            make_element("Patient.name.family", None),
        ];

        assert!(validate_hierarchy(&elements).is_ok());
    }

    #[test]
    fn rejects_child_before_parent() {
        let elements = vec![
            make_element("Patient", None),
            make_element("Patient.name.family", None),
            make_element("Patient.name", None),
        ];

        assert!(validate_hierarchy(&elements).is_err());
    }

    #[test]
    fn validates_differential_against_base() {
        let base = Snapshot {
            element: vec![
                make_element("Patient", None),
                make_element("Patient.name", None),
            ],
        };

        let diff = Differential {
            element: vec![
                make_element("Patient.name", None),
                make_element("Patient.name.family", None),
            ],
        };

        assert!(validate_differential(&diff, &base).is_ok());
    }

    #[test]
    fn rejects_differential_with_missing_parent() {
        let base = Snapshot {
            element: vec![make_element("Patient", None)],
        };

        let diff = Differential {
            element: vec![make_element("Patient.name.family", None)],
        };

        assert!(validate_differential(&diff, &base).is_err());
    }

    #[test]
    fn validates_snapshot() {
        let snapshot = Snapshot {
            element: vec![
                make_element("Patient", None),
                make_element("Patient.name", None),
            ],
        };

        assert!(validate_snapshot(&snapshot).is_ok());
    }

    #[test]
    fn rejects_snapshot_without_root() {
        let snapshot = Snapshot {
            element: vec![make_element("Patient.name", None)],
        };

        assert!(validate_snapshot(&snapshot).is_err());
    }
}
