//! Error types for snapshot expansion

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Expansion error: {0}")]
    Expansion(String),

    #[error("Snapshot error: {0}")]
    Snapshot(String),

    #[error("Differential error: {0}")]
    Differential(String),

    #[error("FHIR context error: {0}")]
    FhirContext(#[from] ferrum_context::Error),
}
