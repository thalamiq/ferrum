use serde::de::DeserializeOwned;
use std::{fs::File, path::PathBuf};
use zunder_models::common::{
    CodeSystem, CodeSystemContentMode, StructureDefinition, StructureDefinitionKind,
    TypeDerivationRule, ValueSet,
};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fhir-test-cases")
}

fn load_fixture<T: DeserializeOwned>(relative: &str) -> T {
    let path = fixtures_root().join(relative);
    assert!(path.exists(), "fixture missing at {:?}", path);

    let file = File::open(&path).expect("failed to open fixture");
    serde_json::from_reader(file).expect("failed to deserialize fixture")
}

#[test]
fn parse_r4_value_set_example() {
    let vs: ValueSet = load_fixture("r4/examples/valueset-example.json");

    assert_eq!(vs.resource_type, "ValueSet");
    assert_eq!(vs.url, "http://hl7.org/fhir/ValueSet/example-extensional");

    let compose = vs.compose.expect("compose should be present");
    assert_eq!(compose.include.len(), 1);

    let concepts = compose.include[0]
        .concept
        .as_ref()
        .expect("concepts should be present");
    assert_eq!(concepts.len(), 4);

    assert!(vs.extensions.contains_key("meta"));
    assert!(vs.extensions.contains_key("text"));
}

#[test]
fn parse_r4_code_system_example() {
    let cs: CodeSystem = load_fixture("r4/examples/codesystem-example.json");

    assert_eq!(cs.resource_type, "CodeSystem");
    assert_eq!(cs.url, "http://hl7.org/fhir/CodeSystem/example");
    assert_eq!(cs.content, CodeSystemContentMode::Complete);

    let concepts = cs.concept.as_ref().expect("concepts should be present");
    assert_eq!(concepts.len(), 3);

    assert!(cs.extensions.contains_key("text"));
}

#[test]
fn parse_r5_structure_definition_example() {
    let sd: StructureDefinition =
        load_fixture("r5/examples/structuredefinition-example-composition.json");

    assert_eq!(sd.resource_type, "StructureDefinition");
    assert_eq!(sd.kind, StructureDefinitionKind::ComplexType);
    assert_eq!(sd.derivation, Some(TypeDerivationRule::Constraint));
    assert_eq!(sd.type_, "Composition");

    let differential = sd
        .differential
        .as_ref()
        .expect("differential should be present");
    assert!(!differential.element.is_empty());

    assert!(sd.extensions.contains_key("text"));
}
