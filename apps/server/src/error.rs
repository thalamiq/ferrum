//! Error types for the FHIR server

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Resource not found: {resource_type}/{id}")]
    ResourceNotFound { resource_type: String, id: String },

    #[error("Resource deleted: {resource_type}/{id}")]
    ResourceDeleted {
        resource_type: String,
        id: String,
        version_id: Option<i32>,
    },

    #[error("Invalid resource: {0}")]
    InvalidResource(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Business rule violation: {0}")]
    BusinessRule(String),

    #[error("Method not allowed: {0}")]
    MethodNotAllowed(String),

    #[error("Version conflict: expected {expected}, got {actual}")]
    VersionConflict { expected: i32, actual: i32 },

    #[error("Precondition failed: {0}")]
    PreconditionFailed(String),

    #[error("Version not found: {resource_type}/{id}/_history/{version_id}")]
    VersionNotFound {
        resource_type: String,
        id: String,
        version_id: i32,
    },

    #[error("Search error: {0}")]
    Search(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Job queue error: {0}")]
    JobQueue(String),

    #[error("FHIR context error: {0}")]
    FhirContext(String),

    #[error("FHIRPath error: {0}")]
    FhirPath(String),

    #[error("Invalid reference: {0}")]
    InvalidReference(String),

    #[error("External reference error: {0}")]
    ExternalReference(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Operation too costly: {0}")]
    TooCostly(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, error_message, etag) = match &self {
            Error::ResourceNotFound { .. } => (StatusCode::NOT_FOUND, self.to_string(), None),
            Error::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string(), None),
            Error::ResourceDeleted { version_id, .. } => {
                (StatusCode::GONE, self.to_string(), *version_id)
            }
            Error::VersionNotFound { .. } => (StatusCode::NOT_FOUND, self.to_string(), None),
            Error::InvalidResource(_) | Error::Validation(_) | Error::InvalidReference(_) => {
                (StatusCode::BAD_REQUEST, self.to_string(), None)
            }
            Error::BusinessRule(_) => (StatusCode::CONFLICT, self.to_string(), None),
            Error::MethodNotAllowed(_) => (StatusCode::METHOD_NOT_ALLOWED, self.to_string(), None),
            Error::VersionConflict { .. } => {
                (StatusCode::PRECONDITION_FAILED, self.to_string(), None)
            }
            Error::PreconditionFailed(_) => {
                (StatusCode::PRECONDITION_FAILED, self.to_string(), None)
            }
            Error::Search(_) => (StatusCode::BAD_REQUEST, self.to_string(), None),
            Error::UnsupportedMediaType(_) => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, self.to_string(), None)
            }
            Error::UnprocessableEntity(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.to_string(), None)
            }
            Error::NotImplemented(_) => (StatusCode::NOT_IMPLEMENTED, self.to_string(), None),
            Error::TooCostly(_) => (StatusCode::FORBIDDEN, self.to_string(), None),
            Error::Database(_)
            | Error::JobQueue(_)
            | Error::Internal(_)
            | Error::ExternalReference(_)
            | Error::Other(_) => {
                tracing::error!("Internal error: {}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                    None,
                )
            }
            Error::FhirContext(_) | Error::FhirPath(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string(), None)
            }
        };

        let body = Json(json!({
            "resourceType": "OperationOutcome",
            "issue": [{
                "severity": "error",
                "code": status_to_fhir_code(status),
                "diagnostics": error_message
            }]
        }));

        let mut response = (status, body).into_response();

        // Always emit a FHIR content type for OperationOutcome errors.
        // (We can't reliably negotiate format here because IntoResponse does not have request context.)
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/fhir+json; charset=utf-8"),
        );

        // Per FHIR spec: MAY include ETag on deleted resource errors
        if let Some(version_id) = etag {
            let etag_value = format!("W/\"{}\"", version_id);
            if let Ok(header_value) = etag_value.parse() {
                response.headers_mut().insert(header::ETAG, header_value);
            }
        }

        response
    }
}

fn status_to_fhir_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "invalid",
        StatusCode::NOT_FOUND => "not-found",
        StatusCode::GONE => "deleted",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "not-supported",
        StatusCode::METHOD_NOT_ALLOWED => "not-supported",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PRECONDITION_FAILED => "conflict",
        StatusCode::UNPROCESSABLE_ENTITY => "processing",
        StatusCode::NOT_IMPLEMENTED => "not-supported",
        StatusCode::FORBIDDEN => "too-costly",
        _ => "exception",
    }
}
