use ferrum_validator::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example 1: Using presets
    let ingestion_cfg = ValidatorConfig::preset(Preset::Ingestion);
    let plan = ingestion_cfg.compile()?;
    println!("Ingestion plan has {} steps", plan.steps.len());

    // Example 2: Builder pattern
    let custom_cfg = ValidatorConfig::builder()
        .preset(Preset::Server)
        .terminology_mode(TerminologyMode::Local)
        .reference_mode(ReferenceMode::TypeOnly)
        .fail_fast(true)
        .max_issues(500)
        .build();

    let plan = custom_cfg.compile()?;
    println!("Custom plan has {} steps", plan.steps.len());

    // Example 3: YAML configuration
    let yaml = r#"
preset: Authoring
fhir:
  version: R5
terminology:
  mode: Local
  timeout: 2000
constraints:
  mode: Full
  best_practice: Warn
  suppress:
    - "dom-6"
exec:
  fail_fast: false
  max_issues: 1000
"#;

    let cfg = ValidatorConfig::from_yaml(yaml)?;
    let plan = cfg.compile()?;
    println!("YAML plan has {} steps", plan.steps.len());

    // Example 4: Error handling
    let invalid_cfg = ValidatorConfig::builder()
        .reference_mode(ReferenceMode::Full)
        .terminology_mode(TerminologyMode::Off)
        .build();

    match invalid_cfg.compile() {
        Ok(_) => println!("Should not happen"),
        Err(e) => println!("Caught expected error: {}", e),
    }

    // Example 5: Export to YAML
    let cfg = ValidatorConfig::preset(Preset::Publication);
    let yaml_output = cfg.to_yaml()?;
    println!("\nPublication preset as YAML:\n{}", yaml_output);

    Ok(())
}
