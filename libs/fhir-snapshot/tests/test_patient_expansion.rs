//! Test snapshot expansion with R5 Patient resource

use std::fs;
use std::path::PathBuf;
use zunder_context::{DefaultFhirContext, FhirContext};
use zunder_registry_client::RegistryClient;
use zunder_snapshot::SnapshotExpander;

mod test_support;

#[test]
#[ignore] // Requires FHIR package cache
fn test_patient_expansion() {
    // Load R5 core package
    let client = RegistryClient::new(None);
    let package =
        test_support::block_on(client.load_or_download_package("hl7.fhir.r5.core", "5.0.0"))
            .expect("Failed to load R5 core package");

    let context = DefaultFhirContext::new(package);

    // Get Patient StructureDefinition
    let patient_sd = context
        .get_structure_definition("http://hl7.org/fhir/StructureDefinition/Patient")
        .expect("context lookup failed")
        .expect("Patient StructureDefinition not found");

    let snapshot = patient_sd
        .snapshot
        .as_ref()
        .expect("Patient StructureDefinition missing snapshot");

    // Count original elements
    let original_elements = &snapshot.element;
    let original_count = original_elements.len();
    println!("Original Patient snapshot has {} elements", original_count);

    // Expand snapshot (convert to zunder_models::Snapshot)
    let fhir_models_snapshot = zunder_models::Snapshot {
        element: snapshot
            .element
            .iter()
            .map(|e| {
                // Convert from fhir-snapshot ElementDefinition to fhir-models ElementDefinition
                // This is a simplified conversion - in practice you'd want a more complete mapping
                zunder_models::ElementDefinition {
                    id: e.id.clone(),
                    path: e.path.clone(),
                    representation: None,
                    slice_name: e.slice_name.clone(),
                    slice_is_constraining: None,
                    short: e.short.clone(),
                    definition: e.definition.clone(),
                    comment: e.comment.clone(),
                    requirements: e.requirements.clone(),
                    alias: e.alias.clone(),
                    min: e.min,
                    max: e.max.clone(),
                    base: e
                        .base
                        .as_ref()
                        .map(|b| zunder_models::ElementDefinitionBase {
                            path: b.path.clone(),
                            min: b.min,
                            max: b.max.clone(),
                        }),
                    content_reference: e.content_reference.clone(),
                    types: e.types.as_ref().map(|types| {
                        types
                            .iter()
                            .map(|t| zunder_models::ElementDefinitionType {
                                code: t.code.clone(),
                                profile: t.profile.clone(),
                                target_profile: t.target_profile.clone(),
                                aggregation: None,
                                versioning: None,
                            })
                            .collect()
                    }),
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
                    is_modifier: e.is_modifier,
                    is_modifier_reason: None,
                    is_summary: e.is_summary,
                    binding: None,
                    mapping: None,
                    slicing: None,
                    must_support: e.must_support,
                    extensions: e.extensions.clone(),
                }
            })
            .collect(),
    };
    let expander = SnapshotExpander::new();
    let expanded_elements = expander
        .expand_snapshot(&fhir_models_snapshot, &context)
        .expect("Failed to expand Patient snapshot");

    let expanded_count = expanded_elements.len();
    println!("Expanded Patient snapshot has {} elements", expanded_count);
    println!(
        "Expansion added {} elements",
        expanded_count - original_count
    );

    // Store expanded result for comparison
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output");
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    // Convert expanded elements back to JSON
    let expanded_elements_json: Vec<serde_json::Value> = expanded_elements
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    let expanded_snapshot = serde_json::json!({
        "element": expanded_elements_json
    });

    let output_file = output_dir.join("patient_expanded.json");
    fs::write(
        &output_file,
        serde_json::to_string_pretty(&expanded_snapshot).unwrap(),
    )
    .expect("Failed to write expanded snapshot");

    println!("Expanded snapshot saved to: {:?}", output_file);

    // Verify expansion worked
    assert!(
        expanded_count > original_count,
        "Expansion should add elements"
    );

    // Check for some expected expanded elements
    let element_paths: Vec<String> = expanded_elements.iter().map(|e| e.path.clone()).collect();

    // Check for choice type expansions (e.g., value[x] → valueQuantity)
    let has_choice_expansions = element_paths.iter().any(|p| p.contains("valueQuantity"));
    println!("Has choice expansions: {}", has_choice_expansions);

    // Check for complex type expansions
    let has_complex_expansions = element_paths.iter().any(|p| {
        p.contains("identifier.use") || p.contains("identifier.type") || p.contains("name.use")
    });
    println!("Has complex expansions: {}", has_complex_expansions);

    // Store summary statistics
    let stats = serde_json::json!({
        "original_count": original_count,
        "expanded_count": expanded_count,
        "added_count": expanded_count - original_count,
        "has_choice_expansions": has_choice_expansions,
        "has_complex_expansions": has_complex_expansions,
        "unique_paths": element_paths.len(),
    });

    let stats_file = output_dir.join("patient_expansion_stats.json");
    fs::write(&stats_file, serde_json::to_string_pretty(&stats).unwrap())
        .expect("Failed to write stats");

    println!("Expansion statistics saved to: {:?}", stats_file);
}
