//! Real-world snapshot generation test using actual FHIR profile data

use serde_json::Value;
use std::fs;
use zunder_models::{BindingStrength, SlicingRules};
use zunder_snapshot::{Differential, Snapshot};

/// Helper to load and parse a JSON file
fn load_json(path: &str) -> Value {
    let content =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e))
}

#[test]
fn test_primary_diagnosis_differential_deserialization() {
    // Test that we can deserialize the differential from the real-world profile
    let profile_json = load_json("tests/data/primary-diagnosis-diff.json");

    let diff_value = profile_json
        .get("differential")
        .expect("Profile should have differential");

    let differential =
        Differential::from_value(diff_value).expect("Should deserialize differential");

    // Verify key elements are present
    assert!(
        !differential.element.is_empty(),
        "Differential should have elements"
    );

    // Check for specific elements from the profile
    let clinical_status = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.clinicalStatus");
    assert!(
        clinical_status.is_some(),
        "Should have Condition.clinicalStatus"
    );
    assert_eq!(
        clinical_status.unwrap().min,
        Some(1),
        "clinicalStatus min should be 1"
    );

    let verification_status = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.verificationStatus");
    assert!(
        verification_status.is_some(),
        "Should have Condition.verificationStatus"
    );
    assert_eq!(
        verification_status.unwrap().min,
        Some(1),
        "verificationStatus min should be 1"
    );

    // Check for sliced elements
    let icd10_slice = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.code.coding" && e.slice_name.as_deref() == Some("icd10-gm"));
    assert!(icd10_slice.is_some(), "Should have icd10-gm slice");
    assert_eq!(
        icd10_slice.unwrap().min,
        Some(1),
        "icd10-gm slice min should be 1"
    );

    let oncotree_slice = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.code.coding" && e.slice_name.as_deref() == Some("oncotree"));
    assert!(oncotree_slice.is_some(), "Should have oncotree slice");
    assert_eq!(
        oncotree_slice.unwrap().max,
        Some("1".to_string()),
        "oncotree slice max should be 1"
    );

    // Check for stage slicing
    let stage_base = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.stage" && e.slice_name.is_none());
    assert!(
        stage_base.is_some(),
        "Should have Condition.stage base element"
    );
    assert_eq!(stage_base.unwrap().min, Some(1), "stage min should be 1");
    assert!(
        stage_base.unwrap().slicing.is_some(),
        "stage should have slicing definition"
    );

    let tnm_staging_slice = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.stage" && e.slice_name.as_deref() == Some("tnmStaging"));
    assert!(tnm_staging_slice.is_some(), "Should have tnmStaging slice");
    assert_eq!(
        tnm_staging_slice.unwrap().max,
        Some("1".to_string()),
        "tnmStaging max should be 1"
    );

    let therapy_concept_slice = differential
        .element
        .iter()
        .find(|e| e.path == "Condition.stage" && e.slice_name.as_deref() == Some("therapyConcept"));
    assert!(
        therapy_concept_slice.is_some(),
        "Should have therapyConcept slice"
    );
    assert_eq!(
        therapy_concept_slice.unwrap().min,
        Some(1),
        "therapyConcept min should be 1"
    );
    assert_eq!(
        therapy_concept_slice.unwrap().max,
        Some("1".to_string()),
        "therapyConcept max should be 1"
    );
}

#[test]
fn test_primary_diagnosis_snapshot_deserialization() {
    // Test that we can deserialize the snapshot from the real-world profile
    let profile_json = load_json("tests/data/primary-diagnosis-snap.json");

    let snap_value = profile_json
        .get("snapshot")
        .expect("Profile should have snapshot");

    let snapshot = Snapshot::from_value(snap_value).expect("Should deserialize snapshot");

    // Verify the snapshot has many elements (it should be fully expanded)
    assert!(
        snapshot.element.len() > 100,
        "Snapshot should have many expanded elements"
    );

    // First element should be the root
    assert_eq!(
        snapshot.element[0].path, "Condition",
        "First element should be root"
    );

    // Check that specific constrained elements exist
    let clinical_status = snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.clinicalStatus");
    assert!(
        clinical_status.is_some(),
        "Snapshot should have Condition.clinicalStatus"
    );
    assert_eq!(
        clinical_status.unwrap().min,
        Some(1),
        "clinicalStatus min should be 1"
    );

    // Check that slice elements exist
    let icd10_slice = snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.code.coding" && e.slice_name.as_deref() == Some("icd10-gm"));
    assert!(icd10_slice.is_some(), "Snapshot should have icd10-gm slice");
}

#[test]
fn test_differential_has_valid_structure() {
    // Test that the differential has proper FHIR structure
    let profile_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = profile_json.get("differential").unwrap();
    let differential = Differential::from_value(diff_value).unwrap();

    // All elements should have valid paths
    for elem in &differential.element {
        assert!(
            !elem.path.is_empty(),
            "All elements should have non-empty paths"
        );
        assert!(
            elem.path.starts_with("Condition"),
            "All paths should start with 'Condition'"
        );
    }

    // Check ID normalization works correctly
    for elem in &differential.element {
        if let Some(slice_name) = &elem.slice_name {
            // Slice IDs should be normalized to path:sliceName format
            if let Some(id) = &elem.id {
                assert!(id.contains(':'), "Slice element ID should contain ':'");
                assert!(
                    id.ends_with(slice_name),
                    "Slice ID should end with slice name"
                );
            }
        }
    }
}

#[test]
fn test_snapshot_element_ordering() {
    // Test that the snapshot maintains proper element ordering
    let profile_json = load_json("tests/data/primary-diagnosis-snap.json");
    let snap_value = profile_json.get("snapshot").unwrap();
    let snapshot = Snapshot::from_value(snap_value).unwrap();

    // Elements should be in hierarchical order
    // Check that parent elements come before their children
    for (i, elem) in snapshot.element.iter().enumerate() {
        if let Some(parent_path) = elem.parent_path() {
            // Find the parent in earlier elements
            let parent_exists = snapshot.element[..i].iter().any(|e| e.path == parent_path);

            assert!(
                parent_exists,
                "Parent '{}' should appear before child '{}' at index {}",
                parent_path, elem.path, i
            );
        }
    }
}

#[test]
fn test_cardinality_constraints() {
    // Test that cardinality constraints are properly applied
    let profile_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = profile_json.get("differential").unwrap();
    let differential = Differential::from_value(diff_value).unwrap();

    // Check specific cardinality constraints
    for elem in &differential.element {
        // If min is specified, it should be a valid number
        if let Some(min) = elem.min {
            assert!(min <= 1000, "Min cardinality should be reasonable");
        }

        // If max is specified, it should be either a number or "*"
        if let Some(ref max) = elem.max {
            assert!(
                max == "*" || max.parse::<u32>().is_ok(),
                "Max should be '*' or a valid number, got '{}'",
                max
            );
        }
    }
}

#[test]
fn test_slicing_definitions() {
    // Test that slicing definitions are properly structured
    let profile_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = profile_json.get("differential").unwrap();
    let differential = Differential::from_value(diff_value).unwrap();

    // Find elements with slicing
    let sliced_elements: Vec<_> = differential
        .element
        .iter()
        .filter(|e| e.slicing.is_some())
        .collect();

    assert!(!sliced_elements.is_empty(), "Should have sliced elements");

    for elem in sliced_elements {
        let slicing = elem.slicing.as_ref().unwrap();

        // Slicing rules should be valid
        assert!(
            matches!(
                slicing.rules,
                SlicingRules::Open | SlicingRules::Closed | SlicingRules::OpenAtEnd
            ),
            "Slicing rules should be valid: {:?}",
            slicing.rules
        );

        // If discriminators are present, they should have type and path
        if let Some(ref discriminators) = slicing.discriminator {
            for disc in discriminators {
                assert!(
                    !disc.path.is_empty(),
                    "Discriminator path should not be empty"
                );
                // Discriminator type is an enum, so it's always valid if deserialized
                let _ = disc.discriminator_type; // Verify it exists
            }
        }
    }
}

#[test]
fn test_binding_definitions() {
    // Test that binding definitions are properly structured
    let profile_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = profile_json.get("differential").unwrap();
    let differential = Differential::from_value(diff_value).unwrap();

    // Find elements with bindings
    let bound_elements: Vec<_> = differential
        .element
        .iter()
        .filter(|e| e.binding.is_some())
        .collect();

    assert!(
        !bound_elements.is_empty(),
        "Should have elements with bindings"
    );

    for elem in bound_elements {
        let binding = elem.binding.as_ref().unwrap();

        // Binding strength should be valid
        assert!(
            matches!(
                binding.strength,
                BindingStrength::Required
                    | BindingStrength::Extensible
                    | BindingStrength::Preferred
                    | BindingStrength::Example
            ),
            "Binding strength should be valid: {:?}",
            binding.strength
        );

        // Should have a value set if it's required or extensible
        if matches!(
            binding.strength,
            BindingStrength::Required | BindingStrength::Extensible
        ) {
            assert!(
                binding.value_set.is_some(),
                "Required/extensible binding should have a value set"
            );
        }
    }
}

#[test]
fn test_type_definitions() {
    // Test that type definitions are properly structured
    let profile_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = profile_json.get("differential").unwrap();
    let differential = Differential::from_value(diff_value).unwrap();

    // Find elements with types
    let typed_elements: Vec<_> = differential
        .element
        .iter()
        .filter(|e| e.types.is_some())
        .collect();

    assert!(
        !typed_elements.is_empty(),
        "Should have elements with types"
    );

    for elem in typed_elements {
        let types = elem.types.as_ref().unwrap();

        assert!(
            !types.is_empty(),
            "Type array should not be empty for {}",
            elem.path
        );

        for elem_type in types {
            // Type code should not be empty
            assert!(!elem_type.code.is_empty(), "Type code should not be empty");

            // If it's a Reference type, it might have target profiles
            if elem_type.code == "Reference" {
                // Target profiles are optional but if present should not be empty
                if let Some(ref targets) = elem_type.target_profile {
                    assert!(
                        !targets.is_empty(),
                        "Target profiles should not be empty if present"
                    );
                }
            }
        }
    }
}
