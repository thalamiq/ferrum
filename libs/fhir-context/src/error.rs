//! Error types for FHIR context

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("StructureDefinition not found: {0}")]
    StructureDefinitionNotFound(String),

    #[error("Invalid StructureDefinition: {0}")]
    InvalidStructureDefinition(String),

    #[error("Element not found: {0}")]
    ElementNotFound(String),

    #[error("Invalid FHIR version: {0}")]
    InvalidFhirVersion(String),

    #[error("Package loader error: {0}")]
    PackageLoader(String),

    #[error("No package loader configured (enable `registry-loader` feature or pass a loader)")]
    PackageLoaderUnavailable,

    #[error("Conformance store error: {0}")]
    ConformanceStore(String),

    #[error("Async runtime unavailable (create the context inside a Tokio runtime or use `with_handle`)")]
    AsyncRuntimeUnavailable,

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Lock file error: {0}")]
    LockFileError(String),

    #[error("Package version mismatch: expected {expected}, got {actual} for package {name}")]
    PackageVersionMismatch {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
