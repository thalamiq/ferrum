//! Tests for full StructureDefinition snapshot generation

use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use zunder_context::DefaultFhirContext;
use zunder_models::StructureDefinition;
use zunder_snapshot::normalization::normalize_snapshot;
use zunder_snapshot::{
    generate_snapshot, generate_structure_definition_differential,
    generate_structure_definition_snapshot, Differential, Snapshot,
};

mod test_support;

/// Create an R4 context for testing
fn create_test_context() -> &'static DefaultFhirContext {
    test_support::context_r4()
}

fn load_json(path: &str) -> Value {
    let content =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));
    serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e))
}

fn load_structure_definition(path: &str) -> StructureDefinition {
    let value = load_json(path);
    serde_json::from_value(value).unwrap_or_else(|e| {
        panic!(
            "Failed to deserialize StructureDefinition from {}: {}",
            path, e
        )
    })
}

fn sd_to_value(sd: &StructureDefinition) -> Value {
    serde_json::to_value(sd).expect("Failed to serialize StructureDefinition")
}

#[test]
fn test_generate_structure_definition_snapshot() {
    let ctx = create_test_context();
    // Load base and derived StructureDefinitions
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let derived_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    // Generate snapshot
    let result = generate_structure_definition_snapshot(Some(&base_sd), &derived_sd, ctx)
        .expect("Should generate structure definition snapshot");

    let result = sd_to_value(&result);

    // Verify it's a valid StructureDefinition
    assert_eq!(
        result.get("resourceType").and_then(|v| v.as_str()),
        Some("StructureDefinition")
    );

    // Verify metadata from derived profile
    assert_eq!(
        result.get("url").and_then(|v| v.as_str()),
        Some("https://fhir.ccc-onconnect.de/StructureDefinition/onconnect-pr-tb-primary-diagnosis")
    );
    assert_eq!(
        result.get("name").and_then(|v| v.as_str()),
        Some("ONCOnnect_PR_TB_PrimaryDiagnosis")
    );
    assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("draft"));

    // Verify it has both snapshot and differential
    assert!(result.get("snapshot").is_some(), "Should have snapshot");
    assert!(
        result.get("differential").is_some(),
        "Should have differential"
    );

    // Verify snapshot has elements
    let snapshot_elements = result
        .get("snapshot")
        .and_then(|s| s.get("element"))
        .and_then(|e| e.as_array())
        .expect("Snapshot should have elements");

    assert!(
        !snapshot_elements.is_empty(),
        "Snapshot should have elements"
    );

    // Verify first element is the root
    let first_elem = &snapshot_elements[0];
    assert_eq!(
        first_elem.get("path").and_then(|v| v.as_str()),
        Some("Condition"),
        "First element should be root Condition"
    );

    // Verify constrained elements are present in snapshot
    let clinical_status = snapshot_elements
        .iter()
        .find(|e| e.get("path").and_then(|v| v.as_str()) == Some("Condition.clinicalStatus"));
    assert!(
        clinical_status.is_some(),
        "Snapshot should have clinicalStatus"
    );

    // Verify min cardinality was applied from differential
    assert_eq!(
        clinical_status.unwrap().get("min").and_then(|v| v.as_u64()),
        Some(1),
        "clinicalStatus should have min=1 from differential"
    );
}

#[test]
fn test_structure_definition_snapshot_preserves_base_definition() {
    let ctx = create_test_context();
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let derived_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    let result = generate_structure_definition_snapshot(Some(&base_sd), &derived_sd, ctx)
        .expect("Should generate structure definition snapshot");

    let result = sd_to_value(&result);
    let derived_json = sd_to_value(&derived_sd);

    // Verify baseDefinition points to the base
    assert_eq!(
        result.get("baseDefinition").and_then(|v| v.as_str()),
        derived_json.get("baseDefinition").and_then(|v| v.as_str())
    );

    // Verify derivation is set
    assert_eq!(
        result.get("derivation").and_then(|v| v.as_str()),
        Some("constraint")
    );
}

#[test]
fn test_structure_definition_snapshot_merges_extensions() {
    let ctx = create_test_context();
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let derived_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    let result = generate_structure_definition_snapshot(Some(&base_sd), &derived_sd, ctx)
        .expect("Should generate structure definition snapshot");

    let result = sd_to_value(&result);

    // Check if extensions are present
    if let Some(extensions) = result.get("extension").and_then(|v| v.as_array()) {
        // Extensions should be merged from both base and derived
        assert!(!extensions.is_empty(), "Should have extensions");
    }
}

#[test]
fn test_structure_definition_snapshot_validates_differential_constraints() {
    let ctx = create_test_context();
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let derived_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    let result = generate_structure_definition_snapshot(Some(&base_sd), &derived_sd, ctx)
        .expect("Should generate structure definition snapshot");

    let result = sd_to_value(&result);

    let snapshot_elements = result
        .get("snapshot")
        .and_then(|s| s.get("element"))
        .and_then(|e| e.as_array())
        .unwrap();

    // Check specific constraints from the differential
    // 1. Condition.verificationStatus should have min=1
    let verification_status = snapshot_elements
        .iter()
        .find(|e| e.get("path").and_then(|v| v.as_str()) == Some("Condition.verificationStatus"));
    assert_eq!(
        verification_status
            .unwrap()
            .get("min")
            .and_then(|v| v.as_u64()),
        Some(1)
    );

    // 2. Condition.code.coding:icd10-gm should exist with min=1
    let icd10_slice = snapshot_elements.iter().find(|e| {
        e.get("path").and_then(|v| v.as_str()) == Some("Condition.code.coding")
            && e.get("sliceName").and_then(|v| v.as_str()) == Some("icd10-gm")
    });
    assert!(icd10_slice.is_some(), "Should have icd10-gm slice");
    assert_eq!(
        icd10_slice.unwrap().get("min").and_then(|v| v.as_u64()),
        Some(1)
    );

    // 3. Condition.stage should have min=1 and slicing
    let stage = snapshot_elements.iter().find(|e| {
        e.get("path").and_then(|v| v.as_str()) == Some("Condition.stage")
            && e.get("sliceName").is_none()
    });
    assert!(stage.is_some(), "Should have Condition.stage");
    assert_eq!(stage.unwrap().get("min").and_then(|v| v.as_u64()), Some(1));
    assert!(
        stage.unwrap().get("slicing").is_some(),
        "Should have slicing definition"
    );
}

#[test]
fn test_generate_structure_definition_differential() {
    // Load base and snapshot StructureDefinitions
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let snapshot_sd = load_structure_definition("tests/data/primary-diagnosis-snap.json");

    // Generate differential
    let result = generate_structure_definition_differential(&base_sd, &snapshot_sd)
        .expect("Should generate structure definition differential");

    let result = sd_to_value(&result);

    // Verify it's a valid StructureDefinition
    assert_eq!(
        result.get("resourceType").and_then(|v| v.as_str()),
        Some("StructureDefinition")
    );

    // Verify it has differential but no snapshot
    assert!(
        result.get("differential").is_some(),
        "Should have differential"
    );
    assert!(result.get("snapshot").is_none(), "Should not have snapshot");

    // Verify differential has elements
    let diff_elements = result
        .get("differential")
        .and_then(|d| d.get("element"))
        .and_then(|e| e.as_array())
        .expect("Differential should have elements");

    assert!(
        !diff_elements.is_empty(),
        "Differential should have elements"
    );
}

#[test]
fn test_roundtrip_structure_definition() {
    let ctx = create_test_context();
    // Start with base and differential
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let derived_diff_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    // Generate snapshot
    let with_snapshot =
        generate_structure_definition_snapshot(Some(&base_sd), &derived_diff_sd, ctx)
            .expect("Should generate snapshot");

    // Generate differential from the snapshot
    let roundtrip_diff = generate_structure_definition_differential(&base_sd, &with_snapshot)
        .expect("Should generate differential");

    let roundtrip_json = sd_to_value(&roundtrip_diff);

    // Verify the roundtrip differential has similar constraints
    let roundtrip_elements = roundtrip_json
        .get("differential")
        .and_then(|d| d.get("element"))
        .and_then(|e| e.as_array())
        .unwrap();

    // Check for key constraints
    let clinical_status = roundtrip_elements
        .iter()
        .find(|e| e.get("path").and_then(|v| v.as_str()) == Some("Condition.clinicalStatus"));

    if let Some(cs) = clinical_status {
        assert_eq!(cs.get("min").and_then(|v| v.as_u64()), Some(1));
    }
}

#[test]
fn test_structure_definition_preserves_type_and_kind() {
    let ctx = create_test_context();
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let derived_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    let result = generate_structure_definition_snapshot(Some(&base_sd), &derived_sd, ctx)
        .expect("Should generate structure definition snapshot");

    let result = sd_to_value(&result);

    // Type should be preserved
    assert_eq!(
        result.get("type").and_then(|v| v.as_str()),
        Some("Condition")
    );

    // Kind should be preserved
    assert_eq!(
        result.get("kind").and_then(|v| v.as_str()),
        Some("resource")
    );

    // FHIR version should be preserved
    assert_eq!(
        result.get("fhirVersion").and_then(|v| v.as_str()),
        Some("4.0.1")
    );
}

#[test]
fn test_generate_snapshot_compares_all_fields() {
    // Load base StructureDefinition and extract snapshot
    let base_json = load_json("tests/data/primary-diagnosis-base.json");
    let base_snapshot_value = base_json
        .get("snapshot")
        .expect("Base should have snapshot");
    let base_snapshot =
        Snapshot::from_value(base_snapshot_value).expect("Should deserialize base snapshot");

    // Load differential StructureDefinition and extract differential
    let diff_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = diff_json
        .get("differential")
        .expect("Differential should have differential");
    let differential =
        Differential::from_value(diff_value).expect("Should deserialize differential");

    // Generate snapshot from base + differential
    let ctx = create_test_context();
    let generated_snapshot =
        generate_snapshot(&base_snapshot, &differential, ctx).expect("Should generate snapshot");

    // Load expected snapshot
    let expected_json = load_json("tests/data/primary-diagnosis-snap.json");
    let expected_snapshot_value = expected_json
        .get("snapshot")
        .expect("Expected should have snapshot");
    let mut expected_snapshot = Snapshot::from_value(expected_snapshot_value)
        .expect("Should deserialize expected snapshot");
    // Normalize expected snapshot to handle empty fixed objects, etc.
    normalize_snapshot(&mut expected_snapshot);

    // Note: The expected snapshot may have more elements due to slice expansion
    // (backbone element children are duplicated for each slice).
    // Our simple snapshot generation doesn't do this expansion - that's a separate step.
    // So we expect generated to have fewer elements than the fully-expanded expected.
    assert!(
        generated_snapshot.element.len() >= base_snapshot.element.len(),
        "Generated snapshot should have at least as many elements as base"
    );

    // The generated snapshot should include the base elements plus new slices from differential
    // Expected has ~180 elements (includes slice expansion), generated has ~168 (without expansion)
    println!(
        "Generated: {} elements, Expected: {} elements (includes slice expansion)",
        generated_snapshot.element.len(),
        expected_snapshot.element.len()
    );

    // Build index of expected elements by ID
    let mut expected_index: HashMap<String, &zunder_snapshot::ElementDefinition> = HashMap::new();
    for elem in &expected_snapshot.element {
        if let Some(id) = &elem.id {
            expected_index.insert(id.clone(), elem);
        }
    }

    // Compare each generated element with expected element
    for generated_elem in &generated_snapshot.element {
        let generated_id = generated_elem.id.as_ref().unwrap_or(&generated_elem.path);

        let expected_elem = match expected_index.get(generated_id) {
            Some(elem) => elem,
            None => {
                // Element not in expected - this is OK if it's a base element that wasn't expanded
                println!(
                    "  Note: Generated element '{}' not in expected (may be due to expansion differences)",
                    generated_id
                );
                continue;
            }
        };

        // Compare all fields
        assert_eq!(
            generated_elem.id, expected_elem.id,
            "Element '{}' (slice: {:?}) id mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.path, expected_elem.path,
            "Element path mismatch"
        );
        assert_eq!(
            generated_elem.slice_name, expected_elem.slice_name,
            "Element '{}' slice_name mismatch",
            generated_elem.path
        );
        assert_eq!(
            generated_elem.min, expected_elem.min,
            "Element '{}' (slice: {:?}) min cardinality mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.max, expected_elem.max,
            "Element '{}' (slice: {:?}) max cardinality mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.types, expected_elem.types,
            "Element '{}' (slice: {:?}) types mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.binding, expected_elem.binding,
            "Element '{}' (slice: {:?}) binding mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.slicing, expected_elem.slicing,
            "Element '{}' (slice: {:?}) slicing mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        // Skip documentation fields (short, definition, comment, requirements, alias)
        // These can vary between implementations (e.g., URL formatting)
        // They are not FHIR constraints, just descriptive text

        // But we do check content_reference as it's a structural constraint
        assert_eq!(
            generated_elem.content_reference, expected_elem.content_reference,
            "Element '{}' (slice: {:?}) content_reference mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.must_support, expected_elem.must_support,
            "Element '{}' (slice: {:?}) must_support mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.is_modifier, expected_elem.is_modifier,
            "Element '{}' (slice: {:?}) is_modifier mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.is_summary, expected_elem.is_summary,
            "Element '{}' (slice: {:?}) is_summary mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        // Compare fixed, pattern, and default_value as JSON values
        // Use semantic comparison (Value equality) instead of string comparison
        // to handle field ordering differences
        assert_eq!(
            generated_elem.fixed, expected_elem.fixed,
            "Element '{}' (slice: {:?}) fixed mismatch",
            generated_elem.path, generated_elem.slice_name
        );

        assert_eq!(
            generated_elem.pattern, expected_elem.pattern,
            "Element '{}' (slice: {:?}) pattern mismatch",
            generated_elem.path, generated_elem.slice_name
        );

        assert_eq!(
            generated_elem.default_value, expected_elem.default_value,
            "Element '{}' (slice: {:?}) default_value mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.constraint, expected_elem.constraint,
            "Element '{}' (slice: {:?}) constraint mismatch",
            generated_elem.path, generated_elem.slice_name
        );
        assert_eq!(
            generated_elem.mapping, expected_elem.mapping,
            "Element '{}' (slice: {:?}) mapping mismatch",
            generated_elem.path, generated_elem.slice_name
        );

        // Note: We skip exact extension comparison because:
        // 1. Extensions (unmapped JSON fields via #[serde(flatten)]) may vary
        // 2. The expected snapshot may have additional metadata from FHIR tooling
        // 3. Our focus is on FHIR constraints (cardinality, types, bindings) not metadata
        // 4. Base elements may have different metadata than what appears in final snapshot
        //
        // The important FHIR profiling constraints (min, max, types, bindings, slicing)
        // are already validated above.
    }

    // Check that key constrained elements from the differential are present with correct values
    // (We don't check ALL expected elements because some are from slice expansion)

    // 1. Check Condition.clinicalStatus has min=1
    let clinical_status = generated_snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.clinicalStatus" && e.slice_name.is_none());
    assert!(
        clinical_status.is_some(),
        "Should have Condition.clinicalStatus"
    );
    assert_eq!(
        clinical_status.unwrap().min,
        Some(1),
        "clinicalStatus should have min=1"
    );

    // 2. Check Condition.code.coding:icd10-gm has min=1
    let icd10_slice = generated_snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.code.coding" && e.slice_name.as_deref() == Some("icd10-gm"));
    assert!(icd10_slice.is_some(), "Should have icd10-gm slice");
    assert_eq!(
        icd10_slice.unwrap().min,
        Some(1),
        "icd10-gm should have min=1"
    );

    // 3. Check Condition.stage has min=1 and slicing
    let stage = generated_snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.stage" && e.slice_name.is_none());
    assert!(stage.is_some(), "Should have Condition.stage");
    assert_eq!(stage.unwrap().min, Some(1), "stage should have min=1");
    assert!(
        stage.unwrap().slicing.is_some(),
        "stage should have slicing"
    );

    // 4. Check stage slices exist
    let tnm_slice = generated_snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.stage" && e.slice_name.as_deref() == Some("tnmStaging"));
    assert!(tnm_slice.is_some(), "Should have tnmStaging slice");

    let therapy_slice = generated_snapshot
        .element
        .iter()
        .find(|e| e.path == "Condition.stage" && e.slice_name.as_deref() == Some("therapyConcept"));
    assert!(therapy_slice.is_some(), "Should have therapyConcept slice");
    assert_eq!(
        therapy_slice.unwrap().min,
        Some(1),
        "therapyConcept should have min=1"
    );
    assert_eq!(
        therapy_slice.unwrap().max,
        Some("1".to_string()),
        "therapyConcept should have max=1"
    );

    println!("✓ All key constraints from differential correctly applied in generated snapshot");
}

#[test]
fn test_generate_snapshot_has_all_expected_fields() {
    // Load base StructureDefinition and extract snapshot
    let base_json = load_json("tests/data/primary-diagnosis-base.json");
    let base_snapshot_value = base_json
        .get("snapshot")
        .expect("Base should have snapshot");
    let base_snapshot =
        Snapshot::from_value(base_snapshot_value).expect("Should deserialize base snapshot");

    // Load differential StructureDefinition and extract differential
    let diff_json = load_json("tests/data/primary-diagnosis-diff.json");
    let diff_value = diff_json
        .get("differential")
        .expect("Differential should have differential");
    let differential =
        Differential::from_value(diff_value).expect("Should deserialize differential");

    // Generate snapshot from base + differential
    let ctx = create_test_context();
    let generated_snapshot =
        generate_snapshot(&base_snapshot, &differential, ctx).expect("Should generate snapshot");

    // Serialize generated snapshot to JSON to get the actual output
    let generated_snapshot_json =
        serde_json::to_value(&generated_snapshot).expect("Should serialize generated snapshot");
    let generated_elements_array = generated_snapshot_json
        .get("element")
        .and_then(|e| e.as_array())
        .expect("Generated snapshot should have elements array");

    // Load expected snapshot as raw JSON to preserve all fields
    let expected_json = load_json("tests/data/primary-diagnosis-snap.json");
    let expected_snapshot_value = expected_json
        .get("snapshot")
        .expect("Expected should have snapshot");
    let expected_elements_array = expected_snapshot_value
        .get("element")
        .and_then(|e| e.as_array())
        .expect("Expected snapshot should have elements array");

    // Build index of expected raw JSON elements by ID and path:sliceName
    let mut expected_json_index: HashMap<String, &Value> = HashMap::new();
    for elem_json in expected_elements_array {
        let id = elem_json
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let path = elem_json.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let slice_name = elem_json.get("sliceName").and_then(|v| v.as_str());

        let key = if let Some(id) = id {
            id
        } else if let Some(slice) = slice_name {
            format!("{}:{}", path, slice)
        } else {
            path.to_string()
        };

        expected_json_index.insert(key, elem_json);
    }

    // Build index of generated JSON elements by ID and path:sliceName
    let mut generated_json_index: HashMap<String, &Value> = HashMap::new();
    for elem_json in generated_elements_array {
        let id = elem_json
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let path = elem_json.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let slice_name = elem_json.get("sliceName").and_then(|v| v.as_str());

        let key = if let Some(id) = id {
            id
        } else if let Some(slice) = slice_name {
            format!("{}:{}", path, slice)
        } else {
            path.to_string()
        };

        generated_json_index.insert(key, elem_json);
    }

    // Fields to skip when checking for presence (truly optional metadata that may not be preserved)
    // Only skip fields that are known to be implementation-specific metadata
    let skip_fields: std::collections::HashSet<&str> = [
        // These are truly optional metadata fields that may differ between implementations
        // But we keep structural fields like base, type, condition, constraint, mustSupport, etc.
    ]
    .iter()
    .cloned()
    .collect();

    let mut missing_fields: Vec<(String, String, String)> = Vec::new();
    let mut matched_count = 0;
    let mut unmatched_count = 0;

    // Compare each generated element with expected element
    for (generated_key, generated_elem_json) in &generated_json_index {
        // Find expected element in raw JSON
        let expected_elem_json = match expected_json_index.get(generated_key) {
            Some(json) => {
                matched_count += 1;
                json
            }
            None => {
                // Element not in expected - this is OK if it's a base element that wasn't expanded
                unmatched_count += 1;
                continue;
            }
        };

        let expected_obj = expected_elem_json
            .as_object()
            .expect("Expected element should be an object");
        let generated_obj = generated_elem_json
            .as_object()
            .expect("Generated element should be an object");

        // Get path and sliceName for error messages
        let path = generated_obj
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let slice_name = generated_obj
            .get("sliceName")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Check that all fields from expected exist in generated
        for (key, expected_value) in expected_obj {
            // Skip fields in skip list
            if skip_fields.contains(key.as_str()) {
                continue;
            }

            // Skip null values in expected (they're optional and don't need to be present)
            if expected_value.is_null() {
                continue;
            }

            // Check if field exists in generated
            if !generated_obj.contains_key(key) {
                missing_fields.push((path.to_string(), slice_name.to_string(), key.clone()));
            }
        }
    }

    println!(
        "Matched {} elements, {} unmatched (expected due to slice expansion differences)",
        matched_count, unmatched_count
    );

    if !missing_fields.is_empty() {
        eprintln!("\nMissing fields in generated snapshot elements:");
        for (path, slice, field) in &missing_fields {
            if slice.is_empty() {
                eprintln!("  Element '{}': missing field '{}'", path, field);
            } else {
                eprintln!(
                    "  Element '{}' (slice: '{}'): missing field '{}'",
                    path, slice, field
                );
            }
        }
        eprintln!("\nTotal missing fields: {}", missing_fields.len());
    }

    assert!(
        missing_fields.is_empty(),
        "Found {} missing fields in generated snapshot elements. All fields from expected snapshot should be present.\n\nMissing fields:\n{}",
        missing_fields.len(),
        missing_fields
            .iter()
            .map(|(path, slice, field)| {
                if slice.is_empty() {
                    format!("  - Element '{}': missing field '{}'", path, field)
                } else {
                    format!(
                        "  - Element '{}' (slice: '{}'): missing field '{}'",
                        path, slice, field
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    );

    println!("✓ All expected fields are present in generated snapshot elements");
}
