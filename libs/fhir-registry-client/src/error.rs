//! Error types for registry-client

use thiserror::Error;

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

/// Registry client errors
#[derive(Error, Debug)]
pub enum Error {
    #[error("Package not found: {name}#{version}")]
    PackageNotFound { name: String, version: String },

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid package structure: {0}")]
    InvalidPackage(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Expansion error: {0}")]
    Expansion(String),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Registry error: {0}")]
    Registry(String),

    #[error("Package error: {0}")]
    Package(#[from] zunder_package::PackageError),
}
