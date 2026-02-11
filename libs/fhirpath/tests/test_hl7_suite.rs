//! Wrapper to run HL7 test suite
//!
//! The test file is located at `fhir-test-cases/r5/fhirpath/tests-fhir-r5.xml`.
//!
//! Run with: `cargo test --test test_hl7_suite -- --ignored`

#[path = "hl7/test_hl7_suite.rs"]
mod hl7_suite;
