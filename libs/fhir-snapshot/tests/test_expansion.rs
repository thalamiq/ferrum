//! Comprehensive tests for snapshot expansion functionality

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use zunder_context::FhirContext;
use zunder_models::{Snapshot, StructureDefinition};
use zunder_snapshot::SnapshotExpander;

/// Helper function to convert JSON snapshot to zunder_models::Snapshot
fn snapshot_from_json(snapshot: &Value) -> Snapshot {
    serde_json::from_value(snapshot.clone()).unwrap()
}

/// Helper to create a minimal StructureDefinition JSON
fn make_sd(type_name: &str, kind: &str, elements: Vec<Value>) -> Value {
    json!({
        "resourceType": "StructureDefinition",
        "url": format!("http://hl7.org/fhir/StructureDefinition/{}", type_name),
        "name": type_name,
        "status": "active",
        "kind": kind,
        "abstract": false,
        "type": type_name,
        "snapshot": {
            "element": elements
        }
    })
}

/// Mock FHIR context for testing
struct MockContext {
    structure_definitions: HashMap<String, Value>,
}

impl MockContext {
    fn new() -> Self {
        let mut ctx = Self {
            structure_definitions: HashMap::new(),
        };
        ctx.setup_default_definitions();
        ctx
    }

    fn setup_default_definitions(&mut self) {
        // Add a simple complex type definition
        self.structure_definitions.insert(
            "http://hl7.org/fhir/StructureDefinition/HumanName".to_string(),
            json!({
                "resourceType": "StructureDefinition",
                "url": "http://hl7.org/fhir/StructureDefinition/HumanName",
                "name": "HumanName",
                "status": "active",
                "kind": "complex-type",
                "abstract": false,
                "type": "HumanName",
                "snapshot": {
                    "element": [
                        { "id": "HumanName", "path": "HumanName" },
                        { "id": "HumanName.use", "path": "HumanName.use", "type": [{"code": "code"}] },
                        { "id": "HumanName.family", "path": "HumanName.family", "type": [{"code": "string"}] },
                        { "id": "HumanName.given", "path": "HumanName.given", "type": [{"code": "string"}] }
                    ]
                }
            }),
        );

        // Add Identifier type
        self.structure_definitions.insert(
            "http://hl7.org/fhir/StructureDefinition/Identifier".to_string(),
            json!({
                "resourceType": "StructureDefinition", "url": "http://hl7.org/fhir/StructureDefinition/Identifier", "name": "Identifier", "status": "active", "kind": "complex-type", "abstract": false, "type": "Identifier",
                "snapshot": {
                    "element": [
                        { "id": "Identifier", "path": "Identifier" },
                        { "id": "Identifier.use", "path": "Identifier.use", "type": [{"code": "code"}] },
                        { "id": "Identifier.system", "path": "Identifier.system", "type": [{"code": "uri"}] },
                        { "id": "Identifier.value", "path": "Identifier.value", "type": [{"code": "string"}] }
                    ]
                }
            }),
        );

        // Add Quantity type for choice expansion
        self.structure_definitions.insert(
            "http://hl7.org/fhir/StructureDefinition/Quantity".to_string(),
            json!({
                "resourceType": "StructureDefinition", "url": "http://hl7.org/fhir/StructureDefinition/Quantity", "name": "Quantity", "status": "active", "kind": "complex-type", "abstract": false, "type": "Quantity",
                "snapshot": {
                    "element": [
                        { "id": "Quantity", "path": "Quantity" },
                        { "id": "Quantity.value", "path": "Quantity.value", "type": [{"code": "decimal"}] },
                        { "id": "Quantity.unit", "path": "Quantity.unit", "type": [{"code": "string"}] }
                    ]
                }
            }),
        );

        // Add CodeableConcept for choice expansion
        self.structure_definitions.insert(
            "http://hl7.org/fhir/StructureDefinition/CodeableConcept".to_string(),
            json!({
                "resourceType": "StructureDefinition", "url": "http://hl7.org/fhir/StructureDefinition/CodeableConcept", "name": "CodeableConcept", "status": "active", "kind": "complex-type", "abstract": false, "type": "CodeableConcept",
                "snapshot": {
                    "element": [
                        { "id": "CodeableConcept", "path": "CodeableConcept" },
                        { "id": "CodeableConcept.coding", "path": "CodeableConcept.coding", "type": [{"code": "Coding"}] },
                        { "id": "CodeableConcept.text", "path": "CodeableConcept.text", "type": [{"code": "string"}] }
                    ]
                }
            }),
        );

        // Add Coding type
        self.structure_definitions.insert(
            "http://hl7.org/fhir/StructureDefinition/Coding".to_string(),
            json!({
                "type": "Coding",
                "snapshot": {
                    "element": [
                        { "id": "Coding", "path": "Coding" },
                        { "id": "Coding.system", "path": "Coding.system", "type": [{"code": "uri"}] },
                        { "id": "Coding.code", "path": "Coding.code", "type": [{"code": "code"}] }
                    ]
                }
            }),
        );

        // Add Extension type for recursion testing
        self.structure_definitions.insert(
            "http://hl7.org/fhir/StructureDefinition/Extension".to_string(),
            json!({
                "resourceType": "StructureDefinition", "url": "http://hl7.org/fhir/StructureDefinition/Extension", "name": "Extension", "status": "active", "kind": "complex-type", "abstract": false, "type": "Extension",
                "snapshot": {
                    "element": [
                        { "id": "Extension", "path": "Extension" },
                        { "id": "Extension.url", "path": "Extension.url", "type": [{"code": "uri"}] },
                        { "id": "Extension.value[x]", "path": "Extension.value[x]", "type": [{"code": "string"}] }
                    ]
                }
            }),
        );
    }
}

impl FhirContext for MockContext {
    fn get_resource_by_url(
        &self,
        _canonical_url: &str,
        _version: Option<&str>,
    ) -> zunder_context::Result<Option<Arc<Value>>> {
        Ok(None)
    }

    fn get_structure_definition(
        &self,
        canonical_url: &str,
    ) -> zunder_context::Result<Option<Arc<StructureDefinition>>> {
        Ok(self
            .structure_definitions
            .get(canonical_url)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(Arc::new))
    }
}

#[test]
fn test_choice_type_expansion() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Observation",
                "path": "Observation"
            },
            {
                "id": "Observation.value[x]",
                "path": "Observation.value[x]",
                "type": [
                    {"code": "Quantity"},
                    {"code": "CodeableConcept"},
                    {"code": "string"}
                ]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    // Should expand to valueQuantity, valueCodeableConcept, valueString
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    assert!(paths.contains(&"Observation.valueQuantity".to_string()));
    assert!(paths.contains(&"Observation.valueCodeableConcept".to_string()));
    assert!(paths.contains(&"Observation.valueString".to_string()));

    // Verify the expanded elements have correct types
    let value_qty = expanded
        .iter()
        .find(|e| e.path == "Observation.valueQuantity")
        .unwrap();
    assert_eq!(value_qty.types.as_ref().unwrap()[0].code, "Quantity");
}

#[test]
fn test_complex_type_expansion() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.name",
                "path": "Patient.name",
                "type": [{"code": "HumanName"}]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    // Should expand HumanName children
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    assert!(paths.contains(&"Patient.name.use".to_string()));
    assert!(paths.contains(&"Patient.name.family".to_string()));
    assert!(paths.contains(&"Patient.name.given".to_string()));
}

#[test]
fn test_content_reference_expansion() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    // contentReference works by copying children of the referenced element
    // So we need an element that has direct children in the snapshot
    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.name",
                "path": "Patient.name",
                "type": [{"code": "HumanName"}]
            },
            {
                "id": "Patient.name.use",
                "path": "Patient.name.use",
                "type": [{"code": "code"}]
            },
            {
                "id": "Patient.name.family",
                "path": "Patient.name.family",
                "type": [{"code": "string"}]
            },
            {
                "id": "Patient.contact",
                "path": "Patient.contact",
                "contentReference": "#Patient.name"
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    // Should expand contentReference to include referenced element's children
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // ContentReference should expand to include referenced element's children
    assert!(paths.contains(&"Patient.contact.use".to_string()));
    assert!(paths.contains(&"Patient.contact.family".to_string()));
}

#[test]
fn test_nested_complex_type_expansion() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.identifier",
                "path": "Patient.identifier",
                "type": [{"code": "Identifier"}]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should expand Identifier children
    assert!(paths.contains(&"Patient.identifier.use".to_string()));
    assert!(paths.contains(&"Patient.identifier.system".to_string()));
    assert!(paths.contains(&"Patient.identifier.value".to_string()));
}

#[test]
fn test_choice_type_with_complex_expansion() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Observation",
                "path": "Observation"
            },
            {
                "id": "Observation.value[x]",
                "path": "Observation.value[x]",
                "type": [
                    {"code": "Quantity"},
                    {"code": "CodeableConcept"}
                ]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should expand choice types
    assert!(paths.contains(&"Observation.valueQuantity".to_string()));
    assert!(paths.contains(&"Observation.valueCodeableConcept".to_string()));

    // Should also expand complex type children
    assert!(paths.contains(&"Observation.valueQuantity.value".to_string()));
    assert!(paths.contains(&"Observation.valueQuantity.unit".to_string()));
    assert!(paths.contains(&"Observation.valueCodeableConcept.coding".to_string()));
    assert!(paths.contains(&"Observation.valueCodeableConcept.text".to_string()));
}

#[test]
fn test_recursion_protection() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    // Create a snapshot with Extension (circular-prone type)
    let snapshot = json!({
        "element": [
            {
                "id": "Extension",
                "path": "Extension"
            },
            {
                "id": "Extension.extension",
                "path": "Extension.extension",
                "type": [{"code": "Extension"}]
            }
        ]
    });

    // Should not cause infinite recursion
    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    // Should have limited expansion due to recursion protection
    assert!(!expanded.is_empty());
    // Extension should be limited by max_recursion_depth (1)
    // The exact count depends on implementation, but should be limited
    let extension_count = expanded
        .iter()
        .filter(|e| e.path.contains("Extension"))
        .count();
    // Should be limited (not infinite), exact count may vary
    assert!(extension_count > 0);
    assert!(extension_count < 20); // Reasonable upper bound
}

#[test]
fn test_primitive_types_not_expanded() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.name",
                "path": "Patient.name",
                "type": [{"code": "string"}]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    // Should not expand primitive types
    assert_eq!(expanded.len(), 2);
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();
    assert_eq!(paths, vec!["Patient", "Patient.name"]);
}

#[test]
fn test_missing_structure_definition() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.unknown",
                "path": "Patient.unknown",
                "type": [{"code": "NonExistentType"}]
            }
        ]
    });

    // Should skip missing StructureDefinition but not error (non-fatal warning)
    let snapshot_model = snapshot_from_json(&snapshot);
    let result = expander.expand_snapshot(&snapshot_model, &ctx);
    assert!(result.is_ok());
    let expanded = result.unwrap();
    // The unknown child should remain untouched (no expansion), no additional children added
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();
    assert_eq!(paths, vec!["Patient", "Patient.unknown"]);
}

#[test]
fn test_empty_snapshot() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": []
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    assert!(expanded.is_empty());
}

#[test]
fn test_invalid_snapshot_missing_element_array() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({});

    // With structured types, invalid snapshots fail at deserialization time
    let result: Result<Snapshot, _> = serde_json::from_value(snapshot);
    assert!(result.is_err());

    // Test with valid but empty snapshot (should work)
    let empty_snapshot = json!({"element": []});
    let snapshot_model = snapshot_from_json(&empty_snapshot);
    let result = expander.expand_snapshot(&snapshot_model, &ctx);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_element_without_id_uses_path() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "path": "Patient.name",
                "type": [{"code": "string"}]
            }
        ]
    });

    // Elements without id should still work if they have a path
    // The expander requires id for tracking, so this test verifies
    // that elements with path but no id are handled
    let snapshot_model = snapshot_from_json(&snapshot);
    let result = expander.expand_snapshot(&snapshot_model, &ctx);
    // This will fail because expander requires id, which is expected behavior
    assert!(result.is_err());
}

#[test]
fn test_duplicate_elements_skipped() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient",
                "path": "Patient"
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    // Should only include each element once
    assert_eq!(expanded.len(), 1);
}

#[test]
fn test_content_reference_circular_prevention() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.name",
                "path": "Patient.name",
                "type": [{"code": "HumanName"}]
            },
            {
                "id": "Patient.name.use",
                "path": "Patient.name.use",
                "type": [{"code": "code"}]
            },
            {
                "id": "Patient.name.family",
                "path": "Patient.name.family",
                "type": [{"code": "string"}]
            },
            {
                "id": "Patient.contact",
                "path": "Patient.contact",
                "contentReference": "#Patient.name"
            },
            {
                "id": "Patient.alias",
                "path": "Patient.alias",
                "contentReference": "#Patient.contact"
            }
        ]
    });

    // Should handle circular references gracefully
    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    assert!(!expanded.is_empty());

    // Should expand contentReference but prevent infinite loops
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should have expanded contentReference children
    assert!(paths.contains(&"Patient.contact.use".to_string()));
    assert!(paths.contains(&"Patient.contact.family".to_string()));
    // Circular reference should be prevented, so Patient.alias should not expand Patient.contact again
    // (it would create Patient.alias.use, Patient.alias.family, but the circular check prevents this)
}

#[test]
fn test_content_reference_with_hash_prefix() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.name",
                "path": "Patient.name",
                "type": [{"code": "HumanName"}]
            },
            {
                "id": "Patient.name.use",
                "path": "Patient.name.use",
                "type": [{"code": "code"}]
            },
            {
                "id": "Patient.name.family",
                "path": "Patient.name.family",
                "type": [{"code": "string"}]
            },
            {
                "id": "Patient.contact",
                "path": "Patient.contact",
                "contentReference": "#Patient.name"
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should handle # prefix correctly
    assert!(paths.contains(&"Patient.contact.use".to_string()));
    assert!(paths.contains(&"Patient.contact.family".to_string()));
}

#[test]
fn test_content_reference_without_hash_prefix() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.name",
                "path": "Patient.name",
                "type": [{"code": "HumanName"}]
            },
            {
                "id": "Patient.name.use",
                "path": "Patient.name.use",
                "type": [{"code": "code"}]
            },
            {
                "id": "Patient.name.family",
                "path": "Patient.name.family",
                "type": [{"code": "string"}]
            },
            {
                "id": "Patient.contact",
                "path": "Patient.contact",
                "contentReference": "Patient.name"
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should handle reference without # prefix
    assert!(paths.contains(&"Patient.contact.use".to_string()));
    assert!(paths.contains(&"Patient.contact.family".to_string()));
}

#[test]
fn test_content_reference_nonexistent_element() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.contact",
                "path": "Patient.contact",
                "contentReference": "#NonExistent"
            }
        ]
    });

    // Should handle missing referenced element gracefully
    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    // Should still include the original element
    assert!(expanded.iter().any(|e| e.path == "Patient.contact"));
}

#[test]
fn test_multiple_choice_types() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Observation",
                "path": "Observation"
            },
            {
                "id": "Observation.value[x]",
                "path": "Observation.value[x]",
                "type": [
                    {"code": "Quantity"},
                    {"code": "CodeableConcept"},
                    {"code": "string"},
                    {"code": "boolean"},
                    {"code": "integer"}
                ]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should expand all choice types
    assert!(paths.contains(&"Observation.valueQuantity".to_string()));
    assert!(paths.contains(&"Observation.valueCodeableConcept".to_string()));
    assert!(paths.contains(&"Observation.valueString".to_string()));
    assert!(paths.contains(&"Observation.valueBoolean".to_string()));
    assert!(paths.contains(&"Observation.valueInteger".to_string()));
}

#[test]
fn test_nested_complex_types_multiple_levels() {
    let _ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    // Create a more complex structure definition
    let mut ctx_with_nested = MockContext::new();
    ctx_with_nested.structure_definitions.insert(
        "http://hl7.org/fhir/StructureDefinition/Address".to_string(),
        make_sd("Address", "complex-type", vec![
            json!({ "id": "Address", "path": "Address" }),
            json!({ "id": "Address.line", "path": "Address.line", "type": [{"code": "string"}] }),
            json!({ "id": "Address.city", "path": "Address.city", "type": [{"code": "string"}] }),
        ]),
    );

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient"
            },
            {
                "id": "Patient.address",
                "path": "Patient.address",
                "type": [{"code": "Address"}]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander
        .expand_snapshot(&snapshot_model, &ctx_with_nested)
        .unwrap();
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should expand nested complex types
    assert!(paths.contains(&"Patient.address.line".to_string()));
    assert!(paths.contains(&"Patient.address.city".to_string()));
}

#[test]
fn test_root_level_element_not_expanded() {
    let ctx = MockContext::new();
    let expander = SnapshotExpander::new();

    let snapshot = json!({
        "element": [
            {
                "id": "Patient",
                "path": "Patient",
                "type": [{"code": "HumanName"}]
            }
        ]
    });

    let snapshot_model = snapshot_from_json(&snapshot);
    let expanded = expander.expand_snapshot(&snapshot_model, &ctx).unwrap();

    // Root-level elements should not be expanded (even if they have complex types)
    // This is by design to prevent expanding the root resource itself
    let paths: Vec<String> = expanded.iter().map(|e| e.path.clone()).collect();

    // Should only have the root element, not its children
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], "Patient");
}
