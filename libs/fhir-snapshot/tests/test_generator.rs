//! Comprehensive tests for snapshot and differential generation

use ferrum_context::DefaultFhirContext;
use ferrum_snapshot::{
    generate_deep_snapshot, generate_differential, generate_snapshot, Differential,
    ElementDefinition, Snapshot,
};
mod test_support;

/// Create an R4 context for testing
fn create_test_context() -> &'static DefaultFhirContext {
    test_support::context_r4()
}

fn make_element(path: &str, min: Option<u32>, max: Option<&str>) -> ElementDefinition {
    ElementDefinition {
        id: Some(path.to_string()),
        path: path.to_string(),
        representation: None,
        slice_name: None,
        slice_is_constraining: None,
        short: None,
        definition: None,
        comment: None,
        requirements: None,
        alias: None,
        min,
        max: max.map(|s| s.to_string()),
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
        extensions: std::collections::HashMap::new(),
    }
}

#[test]
fn test_generate_snapshot_merges_existing_element() {
    let ctx = create_test_context();
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
        ],
    };

    let differential = Differential {
        element: vec![make_element("Patient.name", Some(1), Some("1"))],
    };

    let merged = generate_snapshot(&base, &differential, ctx).unwrap();

    assert_eq!(merged.element.len(), 2);
    let name = merged
        .element
        .iter()
        .find(|e| e.path == "Patient.name")
        .unwrap();
    assert_eq!(name.min, Some(1));
    assert_eq!(name.max, Some("1".to_string()));
}

#[test]
fn test_generate_snapshot_adds_new_element() {
    let ctx = create_test_context();
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", None, None),
        ],
    };

    let differential = Differential {
        element: vec![make_element("Patient.birthDate", None, None)],
    };

    let merged = generate_snapshot(&base, &differential, ctx).unwrap();

    assert_eq!(merged.element.len(), 3);
    assert!(merged.element.iter().any(|e| e.path == "Patient.birthDate"));
}

#[test]
fn test_generate_snapshot_multiple_differential_elements() {
    let ctx = create_test_context();
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), None),
        ],
    };

    let differential = Differential {
        element: vec![
            make_element("Patient.name", Some(1), None),
            make_element("Patient.name.family", Some(0), Some("1")),
            make_element("Patient.birthDate", None, None),
        ],
    };

    let merged = generate_snapshot(&base, &differential, ctx).unwrap();

    assert_eq!(merged.element.len(), 4);
    let name = merged
        .element
        .iter()
        .find(|e| e.path == "Patient.name")
        .unwrap();
    assert_eq!(name.min, Some(1));
}

#[test]
fn test_generate_differential_new_element() {
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
        ],
    };

    let snapshot = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
            make_element("Patient.birthDate", None, None),
        ],
    };

    let diff = generate_differential(&base, &snapshot).unwrap();

    assert_eq!(diff.element.len(), 1);
    let birth_date = diff
        .element
        .iter()
        .find(|e| e.path == "Patient.birthDate")
        .unwrap();
    assert_eq!(birth_date.path, "Patient.birthDate");
}

#[test]
fn test_generate_differential_modified_element() {
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
        ],
    };

    let snapshot = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(1), Some("1")),
        ],
    };

    let diff = generate_differential(&base, &snapshot).unwrap();

    assert_eq!(diff.element.len(), 1);
    let name_diff = diff
        .element
        .iter()
        .find(|e| e.path == "Patient.name")
        .unwrap();
    assert_eq!(name_diff.min, Some(1));
    assert_eq!(name_diff.max, Some("1".to_string()));
}

#[test]
fn test_generate_differential_no_changes() {
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
        ],
    };

    let snapshot = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
        ],
    };

    let diff = generate_differential(&base, &snapshot).unwrap();

    // Should be empty since there are no changes
    assert_eq!(diff.element.len(), 0);
}

#[test]
fn test_generate_differential_multiple_changes() {
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
            make_element("Patient.active", None, None),
        ],
    };

    let snapshot_name = make_element("Patient.name", Some(1), Some("1"));
    let mut snapshot_active = make_element("Patient.active", None, None);
    snapshot_active.must_support = Some(true);

    let snapshot = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            snapshot_name,
            snapshot_active,
            make_element("Patient.birthDate", None, None),
        ],
    };

    let diff = generate_differential(&base, &snapshot).unwrap();

    assert_eq!(diff.element.len(), 3);
}

#[test]
fn test_generate_deep_snapshot() {
    let snapshot = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", None, None),
        ],
    };

    let ctx = create_test_context();
    let deep = generate_deep_snapshot(&snapshot, ctx).unwrap();
    assert_eq!(deep.element.len(), 2);
}

#[test]
fn test_roundtrip_snapshot_differential() {
    // Generate snapshot from differential, then generate differential back
    let ctx = create_test_context();
    let base = Snapshot {
        element: vec![
            make_element("Patient", None, None),
            make_element("Patient.name", Some(0), Some("*")),
        ],
    };

    let differential = Differential {
        element: vec![
            make_element("Patient.name", Some(1), None),
            make_element("Patient.birthDate", None, None),
        ],
    };

    let snapshot = generate_snapshot(&base, &differential, ctx).unwrap();
    let roundtrip_diff = generate_differential(&base, &snapshot).unwrap();

    assert_eq!(roundtrip_diff.element.len(), 2);

    // Verify the differential contains the expected changes
    let name_diff = roundtrip_diff
        .element
        .iter()
        .find(|e| e.path == "Patient.name")
        .unwrap();
    assert_eq!(name_diff.min, Some(1));

    let birth_date_diff = roundtrip_diff
        .element
        .iter()
        .find(|e| e.path == "Patient.birthDate")
        .unwrap();
    assert_eq!(birth_date_diff.path, "Patient.birthDate");
}
