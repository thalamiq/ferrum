use serde_json::json;
use zunder_context::DefaultFhirContext;
use zunder_validator::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Profile Validation Example ===\n");
    println!("This example demonstrates how profile validation handles multiple profiles:");
    println!("  1. Explicit profiles (from ProfilesConfig.explicit_profiles)");
    println!("  2. meta.profile (from resource)");
    println!("  3. Schema validation uses base profile only\n");

    // Test 1: Explicit profiles in config
    println!("=== Test 1: Explicit Profiles ===");
    println!("Configure validator to check against specific profile URLs");

    let mut config = ValidatorConfig::builder()
        .schema_mode(SchemaMode::On)
        .profiles_mode(ProfilesMode::On)
        .build();

    config.profiles.explicit_profiles = Some(vec![
        "http://example.org/fhir/StructureDefinition/CustomPatient".to_string(),
        "http://hl7.org/fhir/StructureDefinition/Patient".to_string(),
    ]);

    let plan = config.compile()?;
    let context1 = DefaultFhirContext::from_packages(vec![]);
    let validator = Validator::new(plan, context1);

    let patient = json!({
        "resourceType": "Patient",
        "id": "example",
        "name": [{"family": "Smith"}]
    });

    let outcome = validator.validate(&patient);
    print_outcome(&outcome);
    println!("Note: With explicit profiles, validator tries each in order until one succeeds.\n");

    // Test 2: Resource with meta.profile
    println!("=== Test 2: meta.profile in Resource ===");
    println!("Resource declares which profiles it conforms to via meta.profile");

    let config2 = ValidatorConfig::builder()
        .schema_mode(SchemaMode::On)
        .profiles_mode(ProfilesMode::On)
        .build();

    let plan2 = config2.compile()?;
    let context2 = DefaultFhirContext::from_packages(vec![]);
    let validator2 = Validator::new(plan2, context2);

    let patient_with_profile = json!({
        "resourceType": "Patient",
        "id": "example",
        "meta": {
            "profile": [
                "http://example.org/fhir/StructureDefinition/USCorePatient"
            ]
        },
        "name": [{"family": "Smith"}]
    });

    let outcome2 = validator2.validate(&patient_with_profile);
    print_outcome(&outcome2);
    println!(
        "Note: Validator tries meta.profile first, then falls back to base Patient profile.\n"
    );

    // Test 3: Base profile fallback
    println!("=== Test 3: Base Profile Fallback ===");
    println!("No explicit profiles or meta.profile - uses base resourceType");

    let config3 = ValidatorConfig::builder()
        .schema_mode(SchemaMode::On)
        .build();

    let plan3 = config3.compile()?;
    let context3 = DefaultFhirContext::from_packages(vec![]);
    let validator3 = Validator::new(plan3, context3);

    let simple_patient = json!({
        "resourceType": "Patient",
        "id": "example"
    });

    let outcome3 = validator3.validate(&simple_patient);
    print_outcome(&outcome3);
    println!("Note: Without meta.profile, validator uses base http://hl7.org/fhir/StructureDefinition/Patient\n");

    println!("=== Summary ===");
    println!("Profile validation resolution order:");
    println!("  1. ProfilesConfig.explicit_profiles (if provided) - takes highest priority");
    println!("  2. meta.profile (from resource) - if explicit_profiles not provided");
    println!("  3. Schema validation always uses base profile (http://hl7.org/fhir/StructureDefinition/{{resourceType}})");
    println!("\nProfile validation checks constraints beyond base schema (slicing, fixed values, patterns, etc.).");

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
