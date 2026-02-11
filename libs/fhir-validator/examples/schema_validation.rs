use serde_json::json;
use zunder_context::DefaultFhirContext;
use zunder_validator::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Schema Validation Example ===\n");

    // Create config with only schema validation enabled
    let config = ValidatorConfig::builder()
        .schema_mode(SchemaMode::On)
        .constraints_mode(ConstraintsMode::Off)
        .profiles_mode(ProfilesMode::Off)
        .terminology_mode(TerminologyMode::Off)
        .build();

    let plan = config.compile()?;
    println!("Validation plan: {} steps (schema only)", plan.steps.len());

    // Note: In production, load actual FHIR packages with StructureDefinitions
    // For this demo, using empty context - schema validation will report SD not found
    let context = DefaultFhirContext::from_packages(vec![]);
    let validator = Validator::new(plan, context);

    // Test 1: Valid Patient resource
    println!("\n1. Valid Patient resource:");
    let valid_patient = json!({
        "resourceType": "Patient",
        "id": "example",
        "name": [{
            "family": "Smith",
            "given": ["John"]
        }]
    });

    let outcome = validator.validate(&valid_patient);
    print_outcome(&outcome);

    // Test 2: Missing resourceType
    println!("\n2. Missing resourceType:");
    let no_type = json!({
        "id": "example",
        "name": [{"family": "Smith"}]
    });

    let outcome = validator.validate(&no_type);
    print_outcome(&outcome);

    // Test 3: Resource with modifier extensions (if disallowed)
    println!("\n3. Resource with modifier extensions (disallowed by default):");
    let with_modifier = json!({
        "resourceType": "Patient",
        "id": "example",
        "modifierExtension": [{
            "url": "http://example.org/extension",
            "valueString": "test"
        }]
    });

    let config_strict = ValidatorConfig::builder()
        .schema_mode(SchemaMode::On)
        .build();

    // Update schema config to disallow modifier extensions
    let mut cfg = config_strict;
    cfg.schema.allow_modifier_extensions = false;

    let plan_strict = cfg.compile()?;
    let validator_strict = Validator::new(plan_strict, DefaultFhirContext::from_packages(vec![]));
    let outcome = validator_strict.validate(&with_modifier);
    print_outcome(&outcome);

    println!("\n=== Summary ===");
    println!("Schema validation checks:");
    println!("  ✓ resourceType presence");
    println!("  ✓ Base StructureDefinition lookup");
    println!("  ✓ Element cardinality (min/max)");
    println!("  ✓ Data type correctness");
    println!("  ✓ Unknown elements (if disallowed)");
    println!("  ✓ Modifier extensions (if disallowed)");

    Ok(())
}

fn print_outcome(outcome: &ValidationOutcome) {
    println!(
        "  Result: {}",
        if outcome.valid {
            "✓ VALID"
        } else {
            "✗ INVALID"
        }
    );
    println!("  Issues: {}", outcome.issues.len());

    for (i, issue) in outcome.issues.iter().enumerate() {
        println!(
            "    {}. [{}] {} - {}",
            i + 1,
            issue.severity,
            issue.code,
            issue.diagnostics
        );
        if let Some(ref loc) = issue.location {
            println!("       Location: {}", loc);
        }
    }
}
