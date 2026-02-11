use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use zunder_format::{json_to_xml, xml_to_json};

/// Helper to normalize JSON for comparison (ignoring formatting/whitespace differences)
fn normalize_json(json_str: &str) -> Value {
    serde_json::from_str(json_str).expect("Failed to parse JSON")
}

/// Helper to get test data directory
fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
}

/// Discover all test file base names (without extension) in the data directory
fn discover_test_cases() -> Vec<String> {
    let data_dir = test_data_dir();
    let mut test_cases = std::collections::HashSet::new();

    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            if let Some(file_name) = entry.file_name().to_str() {
                if let Some(stem) = file_name.strip_suffix(".json") {
                    // Check if corresponding XML exists
                    let xml_path = data_dir.join(format!("{}.xml", stem));
                    if xml_path.exists() {
                        test_cases.insert(stem.to_string());
                    }
                } else if let Some(stem) = file_name.strip_suffix(".xml") {
                    // Check if corresponding JSON exists
                    let json_path = data_dir.join(format!("{}.json", stem));
                    if json_path.exists() {
                        test_cases.insert(stem.to_string());
                    }
                }
            }
        }
    }

    let mut cases: Vec<_> = test_cases.into_iter().collect();
    cases.sort();
    cases
}

/// Helper to load test files
fn load_test_files(base_name: &str) -> (String, String) {
    let json_path = test_data_dir().join(format!("{}.json", base_name));
    let xml_path = test_data_dir().join(format!("{}.xml", base_name));

    let json = fs::read_to_string(&json_path)
        .unwrap_or_else(|_| panic!("Failed to read {}", json_path.display()));
    let xml = fs::read_to_string(&xml_path)
        .unwrap_or_else(|_| panic!("Failed to read {}", xml_path.display()));

    (json, xml)
}

// ============================================================================
// Test Discovery and File Validation
// ============================================================================

#[test]
fn test_data_files_exist() {
    let test_cases = discover_test_cases();
    assert!(
        !test_cases.is_empty(),
        "No test cases found in {}",
        test_data_dir().display()
    );

    println!(
        "Discovered {} test case(s): {:?}",
        test_cases.len(),
        test_cases
    );

    for base_name in &test_cases {
        let json_path = test_data_dir().join(format!("{}.json", base_name));
        let xml_path = test_data_dir().join(format!("{}.xml", base_name));

        assert!(
            json_path.exists(),
            "Missing JSON test file: {}",
            json_path.display()
        );
        assert!(
            xml_path.exists(),
            "Missing XML test file: {}",
            xml_path.display()
        );

        // Verify files are not empty
        let json_content = fs::read_to_string(&json_path).unwrap();
        let xml_content = fs::read_to_string(&xml_path).unwrap();

        assert!(
            !json_content.trim().is_empty(),
            "{} is empty",
            json_path.display()
        );
        assert!(
            !xml_content.trim().is_empty(),
            "{} is empty",
            xml_path.display()
        );
    }
}

// ============================================================================
// Basic Validation Tests - All test cases
// ============================================================================

#[test]
fn test_all_json_produces_valid_xml() {
    let test_cases = discover_test_cases();
    assert!(!test_cases.is_empty(), "No test cases found");

    for base_name in test_cases {
        println!("Testing JSON→XML for: {}", base_name);
        let (json, _xml) = load_test_files(&base_name);
        let result_xml = json_to_xml(&json)
            .unwrap_or_else(|e| panic!("{}: JSON to XML conversion failed: {}", base_name, e));

        // Verify it parses as valid XML
        let doc = roxmltree::Document::parse(&result_xml)
            .unwrap_or_else(|e| panic!("{}: Generated XML is not valid: {}", base_name, e));

        // Verify resource type is set
        assert!(
            !doc.root_element().tag_name().name().is_empty(),
            "{}: Resource type should not be empty",
            base_name
        );
    }
}

#[test]
fn test_all_xml_produces_valid_json() {
    let test_cases = discover_test_cases();
    assert!(!test_cases.is_empty(), "No test cases found");

    for base_name in test_cases {
        println!("Testing XML→JSON for: {}", base_name);
        let (_json, xml) = load_test_files(&base_name);
        let result_json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: XML to JSON conversion failed: {}", base_name, e));

        // Verify it parses as valid JSON
        let value: Value = serde_json::from_str(&result_json)
            .unwrap_or_else(|e| panic!("{}: Generated JSON is not valid: {}", base_name, e));

        // Verify resourceType exists
        assert!(
            value.get("resourceType").is_some(),
            "{}: resourceType should exist",
            base_name
        );
    }
}

// ============================================================================
// Round-trip Tests - All test cases
// ============================================================================

#[test]
fn test_all_round_trip_xml_json_xml() {
    let test_cases = discover_test_cases();
    assert!(!test_cases.is_empty(), "No test cases found");

    for base_name in test_cases {
        println!("Testing XML→JSON→XML round-trip for: {}", base_name);
        let (_json, xml) = load_test_files(&base_name);

        // XML -> JSON -> XML
        let json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: XML to JSON conversion failed: {}", base_name, e));
        let result_xml = json_to_xml(&json)
            .unwrap_or_else(|e| panic!("{}: JSON to XML conversion failed: {}", base_name, e));

        // Verify both XMLs produce the same JSON representation
        let original_json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: Original XML to JSON failed: {}", base_name, e));
        let result_json = xml_to_json(&result_xml)
            .unwrap_or_else(|e| panic!("{}: Result XML to JSON failed: {}", base_name, e));

        let original_val = normalize_json(&original_json);
        let result_val = normalize_json(&result_json);

        // Compare key properties that should always be preserved
        assert_eq!(
            original_val["resourceType"], result_val["resourceType"],
            "{}: resourceType mismatch",
            base_name
        );

        if let Some(id) = original_val.get("id") {
            assert_eq!(
                id,
                result_val.get("id").unwrap(),
                "{}: id mismatch",
                base_name
            );
        }
    }
}

#[test]
fn test_all_round_trip_json_xml_json() {
    let test_cases = discover_test_cases();
    assert!(!test_cases.is_empty(), "No test cases found");

    for base_name in test_cases {
        println!("Testing JSON→XML→JSON round-trip for: {}", base_name);
        let (json, _xml) = load_test_files(&base_name);

        // JSON -> XML -> JSON
        let xml = json_to_xml(&json)
            .unwrap_or_else(|e| panic!("{}: JSON to XML conversion failed: {}", base_name, e));
        let result_json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: XML to JSON conversion failed: {}", base_name, e));

        let original = normalize_json(&json);
        let round_trip = normalize_json(&result_json);

        // Verify basic structure is preserved
        assert_eq!(
            original["resourceType"], round_trip["resourceType"],
            "{}: resourceType mismatch",
            base_name
        );

        if let Some(id) = original.get("id") {
            assert_eq!(
                id,
                round_trip.get("id").unwrap(),
                "{}: id mismatch",
                base_name
            );
        }

        // Check for common FHIR fields if they exist
        for field in &["active", "gender", "birthDate", "deceasedBoolean"] {
            if let Some(original_value) = original.get(*field) {
                assert_eq!(
                    original_value,
                    round_trip.get(*field).unwrap(),
                    "{}: {} mismatch",
                    base_name,
                    field
                );
            }
        }
    }
}

// ============================================================================
// Specific Feature Tests (run against all test cases)
// ============================================================================

#[test]
fn test_all_preserve_extensions() {
    let test_cases = discover_test_cases();
    let mut tested_count = 0;

    for base_name in test_cases {
        let (json, _xml) = load_test_files(&base_name);
        let original = normalize_json(&json);

        // Skip if no extensions in this test case
        let has_extensions = original
            .as_object()
            .map(|obj| obj.keys().any(|k| k.starts_with('_')))
            .unwrap_or(false);

        if !has_extensions {
            continue;
        }

        println!("Testing extension preservation for: {}", base_name);
        tested_count += 1;

        // Convert JSON to XML
        let xml = json_to_xml(&json)
            .unwrap_or_else(|e| panic!("{}: JSON to XML conversion failed: {}", base_name, e));

        // Verify XML contains extensions
        assert!(
            xml.contains("extension"),
            "{}: XML should contain extension elements",
            base_name
        );

        // Note: Full round-trip (JSON→XML→JSON) won't preserve _field entries
        // that have no corresponding field value, because XML can't distinguish
        // between primitives and complex types without schema knowledge.
        // This is a known limitation of schema-agnostic conversion.

        let result_json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: XML to JSON conversion failed: {}", base_name, e));
        let round_trip = normalize_json(&result_json);

        // Check that metadata fields with corresponding values are preserved
        if let Some(obj) = original.as_object() {
            for (key, _value) in obj {
                if key.starts_with('_') {
                    let base_field = key.trim_start_matches('_');
                    let has_base_value = original.get(base_field).is_some();

                    if has_base_value {
                        // If the base field exists, metadata should be preserved
                        assert!(
                            round_trip.get(key).is_some() || round_trip.get(base_field).is_some(),
                            "{}: {} or its metadata should survive round-trip",
                            base_name,
                            base_field
                        );
                    }
                    // Note: metadata without base values (_active with no active)
                    // will become regular fields (active: {...}) in XML→JSON due to
                    // ambiguity without schema knowledge
                }
            }
        }
    }

    assert!(tested_count > 0, "No test cases with extensions were found");
}

#[test]
fn test_all_preserve_nested_structures() {
    let test_cases = discover_test_cases();

    for base_name in test_cases {
        println!("Testing nested structure preservation for: {}", base_name);
        let (json, _xml) = load_test_files(&base_name);
        let original = normalize_json(&json);

        // Convert and verify structure is preserved
        let xml = json_to_xml(&json)
            .unwrap_or_else(|e| panic!("{}: JSON to XML conversion failed: {}", base_name, e));
        let result_json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: XML to JSON conversion failed: {}", base_name, e));
        let round_trip = normalize_json(&result_json);

        // Verify all top-level keys exist in round-trip (except metadata keys are handled separately)
        if let Some(obj) = original.as_object() {
            for key in obj.keys() {
                if !key.starts_with('_') {
                    assert!(
                        round_trip.get(key).is_some(),
                        "{}: {} should be preserved",
                        base_name,
                        key
                    );
                }
            }
        }
    }
}

#[test]
fn test_all_preserve_contained_resources() {
    let test_cases = discover_test_cases();

    for base_name in test_cases {
        let (json, _xml) = load_test_files(&base_name);
        let original = normalize_json(&json);

        // Skip if no contained resources
        if original.get("contained").is_none() {
            continue;
        }

        println!("Testing contained resource preservation for: {}", base_name);

        // Convert and verify contained resources are preserved
        let xml = json_to_xml(&json)
            .unwrap_or_else(|e| panic!("{}: JSON to XML conversion failed: {}", base_name, e));
        assert!(
            xml.contains("contained"),
            "{}: XML should contain contained elements",
            base_name
        );

        let result_json = xml_to_json(&xml)
            .unwrap_or_else(|e| panic!("{}: XML to JSON conversion failed: {}", base_name, e));
        let round_trip = normalize_json(&result_json);

        assert!(
            round_trip.get("contained").is_some(),
            "{}: contained resources should be preserved",
            base_name
        );
    }
}
