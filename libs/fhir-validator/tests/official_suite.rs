//! Official FHIR Validator Test Suite
//!
//! Uses libtest-mimic to generate one test per manifest entry. Each test
//! validates a resource and compares the error count against the Java
//! reference implementation expectations.
//!
//! ```bash
//! # Run all eligible tests
//! cargo test -p ferrum-validator --test official_suite
//!
//! # Filter by name
//! cargo test -p ferrum-validator --test official_suite -- patient
//!
//! # List tests without running
//! cargo test -p ferrum-validator --test official_suite -- --list
//! ```

mod test_support;

use std::process;
use std::sync::OnceLock;

use ferrum_context::DefaultFhirContext;
use ferrum_validator::{Preset, ValidationOutcome, Validator, ValidatorConfig};
use libtest_mimic::{Arguments, Failed, Trial};

use test_support::{
    block_on, fhir_version_label, is_eligible, load_manifest, load_test_resource,
    resolve_expected_errors, skip_reason, TestCase,
};

/// Stack size for threads that load/validate FHIR resources.
/// Deeply nested StructureDefinitions and FHIRPath evaluation need a lot of stack in debug mode.
const STACK_SIZE: usize = 128 * 1024 * 1024; // 128 MB (debug builds need more stack for snapshot expansion)

// ---------------------------------------------------------------------------
// Shared validators (expensive — created once, reused across all tests)
// ---------------------------------------------------------------------------

struct Validators {
    r4: Validator<DefaultFhirContext>,
    r5: Validator<DefaultFhirContext>,
}

static VALIDATORS: OnceLock<Validators> = OnceLock::new();

/// Initialize validators on a large-stack thread (context loading is deeply recursive).
fn init_validators() {
    VALIDATORS.get_or_init(|| {
        let handle = std::thread::Builder::new()
            .name("validator-init".into())
            .stack_size(STACK_SIZE)
            .spawn(|| {
                let config = ValidatorConfig::preset(Preset::Authoring);

                eprintln!("  Loading R4 context...");
                let ctx_r4 = block_on(DefaultFhirContext::from_fhir_version_async(None, "R4"))
                    .expect("Failed to create R4 context");
                let r4 = Validator::from_config(&config, ctx_r4)
                    .expect("Failed to create R4 validator");

                eprintln!("  Loading R5 context...");
                let ctx_r5 = block_on(DefaultFhirContext::from_fhir_version_async(None, "R5"))
                    .expect("Failed to create R5 context");
                let r5 = Validator::from_config(&config, ctx_r5)
                    .expect("Failed to create R5 validator");

                eprintln!("  Validators ready.");
                Validators { r4, r5 }
            })
            .expect("failed to spawn validator-init thread");

        handle.join().expect("validator-init thread panicked")
    });
}

fn validators() -> &'static Validators {
    VALIDATORS.get().expect("validators not initialized — call init_validators() first")
}

// ---------------------------------------------------------------------------
// Test generation
// ---------------------------------------------------------------------------

fn make_trial(tc: &TestCase) -> Trial {
    let version_label = fhir_version_label(tc.version.as_deref());
    let module = tc.module.as_deref().unwrap_or("base");

    // Test name: "R4::general::allergy"
    let test_name = format!("{}::{}::{}", version_label, module, tc.name);

    if let Some(reason) = skip_reason(tc) {
        return Trial::test(test_name, move || Err(reason.into())).with_ignored_flag(true);
    }

    let java = tc.java.clone().unwrap();
    let file = tc.file.clone();
    let version = tc.version.clone();

    Trial::test(test_name, move || run_single_test(&file, &version, &java))
}

fn run_single_test(
    file: &str,
    version: &Option<String>,
    java: &test_support::JavaExpectation,
) -> Result<(), Failed> {
    let expected = resolve_expected_errors(java)
        .ok_or_else(|| Failed::from("could not resolve expected error count (missing outcome file?)"))?;

    let file = file.to_string();
    let label = fhir_version_label(version.as_deref()).to_string();

    // Run loading + validation on a large-stack thread.
    let handle = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || -> Result<ValidationOutcome, Failed> {
            let resource = load_test_resource(&file)
                .ok_or_else(|| Failed::from(format!("could not load resource: {file}")))?;
            let v = validators();
            Ok(match label.as_str() {
                "R4" => v.r4.validate(&resource),
                _ => v.r5.validate(&resource),
            })
        })
        .map_err(|e| Failed::from(format!("failed to spawn thread: {e}")))?;

    let outcome = handle
        .join()
        .map_err(|_| Failed::from("validation panicked (stack overflow?)"))??;

    let actual = outcome.error_count() as u32;

    if actual == expected {
        Ok(())
    } else {
        let mut msg = format!("error count mismatch: expected {expected}, got {actual}");
        for (i, issue) in outcome.issues.iter().take(5).enumerate() {
            msg.push_str(&format!(
                "\n  [{i}] {}: {} @ {}",
                issue.severity,
                issue.diagnostics,
                issue.location.as_deref().unwrap_or("-"),
            ));
        }
        let remaining = outcome.issues.len().saturating_sub(5);
        if remaining > 0 {
            msg.push_str(&format!("\n  ... and {remaining} more issues"));
        }
        Err(msg.into())
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args = Arguments::from_args();

    let manifest = match load_manifest() {
        Some(m) => m,
        None => {
            eprintln!("fhir-test-cases/validator/manifest.json not found — skipping.");
            eprintln!("Make sure the git submodule is initialized:");
            eprintln!("  git submodule update --init fhir-test-cases");
            process::exit(0);
        }
    };

    let trials: Vec<Trial> = manifest.test_cases.iter().map(make_trial).collect();

    let eligible = manifest.test_cases.iter().filter(|tc| is_eligible(tc)).count();
    eprintln!(
        "Official suite: {} total, {} eligible, {} skipped",
        manifest.test_cases.len(),
        eligible,
        manifest.test_cases.len() - eligible,
    );

    // Eagerly initialize validators before running tests.
    // This avoids the first test paying the full init cost and ensures
    // the init happens on a thread with enough stack.
    if !args.list {
        init_validators();
    }

    libtest_mimic::run(&args, trials).exit();
}
