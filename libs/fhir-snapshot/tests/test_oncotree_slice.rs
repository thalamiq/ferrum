//! Test to verify oncotree slice has all required fields

use serde_json::Value;
use std::fs;
use ferrum_context::DefaultFhirContext;
use ferrum_models::StructureDefinition;
use ferrum_snapshot::generate_structure_definition_snapshot;

mod test_support;

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

fn create_test_context() -> &'static DefaultFhirContext {
    test_support::context_r4()
}

#[test]
fn test_oncotree_slice_has_all_fields() {
    let ctx = create_test_context();

    // Load base and differential
    let base_sd = load_structure_definition("tests/data/primary-diagnosis-base.json");
    let diff_sd = load_structure_definition("tests/data/primary-diagnosis-diff.json");

    // Generate snapshot
    let result = generate_structure_definition_snapshot(Some(&base_sd), &diff_sd, ctx)
        .expect("Failed to generate snapshot");

    let result_json = serde_json::to_value(&result).expect("Failed to serialize generated SD");

    // Find oncotree slice in generated snapshot
    let generated_elements = result_json
        .get("snapshot")
        .and_then(|s| s.get("element"))
        .and_then(|e| e.as_array())
        .expect("Generated snapshot should have elements");

    let oncotree_elem = generated_elements
        .iter()
        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some("Condition.code.coding:oncotree"))
        .expect("Should have oncotree slice");

    println!("\nGenerated oncotree slice:");
    println!("{}\n", serde_json::to_string_pretty(oncotree_elem).unwrap());

    // Load expected snapshot
    let expected_sd = load_json("tests/data/primary-diagnosis-snap.json");
    let expected_elements = expected_sd
        .get("snapshot")
        .and_then(|s| s.get("element"))
        .and_then(|e| e.as_array())
        .expect("Expected snapshot should have elements");

    let expected_oncotree_elem = expected_elements
        .iter()
        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some("Condition.code.coding:oncotree"))
        .expect("Should have expected oncotree slice");

    println!("Expected oncotree slice:");
    println!(
        "{}\n",
        serde_json::to_string_pretty(expected_oncotree_elem).unwrap()
    );

    // Check for key fields
    let generated_obj = oncotree_elem.as_object().unwrap();
    let expected_obj = expected_oncotree_elem.as_object().unwrap();

    // Fields that should be present
    let key_fields = vec![
        "type",
        "short",
        "definition",
        "comment",
        "constraint",
        "mapping",
        "base",
    ];

    for field in &key_fields {
        if expected_obj.contains_key(*field) && !expected_obj[*field].is_null() {
            assert!(
                generated_obj.contains_key(*field),
                "Generated oncotree slice missing field: {}",
                field
            );
            println!("✓ Field '{}' is present", field);
        }
    }

    println!("\n✓ All key fields are present in generated oncotree slice!");
}
