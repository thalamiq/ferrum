use serde_json::json;
use zunder_context::DefaultFhirContext;
use zunder_validator::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Three-Phase Validator Architecture ===\n");

    // Phase 1: Declarative Configuration
    println!("Phase 1: Configuration");
    let config = ValidatorConfig::builder()
        .preset(Preset::Server)
        .terminology_mode(TerminologyMode::Local)
        .build();
    println!("  ✓ Config created with preset: Server");
    println!("  ✓ Terminology mode: Local\n");

    // Phase 2: Compiled Validation Plan
    println!("Phase 2: Compilation");
    let plan = config.compile()?;
    println!("  ✓ Plan compiled with {} steps", plan.steps.len());
    println!("  ✓ Fail fast: {}", plan.fail_fast);
    println!("  ✓ Max issues: {}\n", plan.max_issues);

    // Phase 3: Reusable Validator & Stateless Execution
    println!("Phase 3: Execution");

    // Create FHIR context (normally would load FHIR packages)
    // For demo, using empty context - in production, use from_fhir_version() or from_packages()
    let context = DefaultFhirContext::from_packages(vec![]);

    // Create validator (expensive, done once)
    let validator = Validator::new(plan, context);
    println!("  ✓ Validator created (reusable)\n");

    // Validate multiple resources (cheap, repeatable)
    let patient = json!({
        "resourceType": "Patient",
        "name": [{"family": "Smith"}]
    });

    let observation = json!({
        "resourceType": "Observation",
        "status": "final"
    });

    println!("  Validating Patient resource:");
    let outcome1 = validator.validate(&patient);
    println!("    - Resource type: {:?}", outcome1.resource_type);
    println!("    - Valid: {}", outcome1.valid);
    println!("    - Issues: {}", outcome1.issues.len());

    println!("\n  Validating Observation resource:");
    let outcome2 = validator.validate(&observation);
    println!("    - Resource type: {:?}", outcome2.resource_type);
    println!("    - Valid: {}", outcome2.valid);
    println!("    - Issues: {}", outcome2.issues.len());

    // Batch validation
    println!("\n  Batch validation:");
    let resources = vec![patient, observation];
    let outcomes = validator.validate_batch(&resources);
    println!("    - Validated {} resources", outcomes.len());

    // Convert to OperationOutcome
    println!("\n  Converting to OperationOutcome:");
    let op_outcome = outcomes[0].to_operation_outcome();
    println!("{}", serde_json::to_string_pretty(&op_outcome)?);

    println!("\n=== Key Properties ===");
    println!("✓ Configuration is declarative and serializable");
    println!("✓ Plan is compiled once, validated for correctness");
    println!("✓ Validator is reusable across many validations");
    println!("✓ Execution is stateless and deterministic");
    println!("✓ FHIR knowledge delegated to fhir-context");

    Ok(())
}
