//! Error types for FHIRPath engine

use thiserror::Error;

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

/// FHIRPath evaluation errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum Error {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Evaluation error: {0}")]
    EvaluationError(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),
}
