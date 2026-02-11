use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("terminology validation is required when using ReferenceMode::Full")]
    TerminologyRequiredForFullRef,

    #[error("FHIR version mismatch: expected {expected:?}, got {got:?}")]
    FhirVersionMismatch {
        expected: crate::FhirVersion,
        got: crate::FhirVersion,
    },

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}
