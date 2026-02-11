//! Error types for FHIR models

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid FHIR resource: {0}")]
    InvalidResource(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field value: {0}")]
    InvalidFieldValue(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Invalid element path: {0}")]
    InvalidPath(String),

    #[error("Element not found: {0}")]
    ElementNotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
