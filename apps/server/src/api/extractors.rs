//! Custom Axum extractors for FHIR content types.

use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::Value as JsonValue;

/// Axum extractor that accepts both `application/fhir+json` and `application/fhir+xml`
/// (plus their generic variants `application/json` and `application/xml`/`text/xml`).
///
/// XML bodies are converted to JSON via `zunder_format::xml_to_json` so that
/// downstream handlers always work with `serde_json::Value`.
pub struct FhirBody(pub JsonValue);

/// Error type for [`FhirBody`] extraction failures.
pub struct FhirBodyRejection {
    status: StatusCode,
    message: String,
}

impl IntoResponse for FhirBodyRejection {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "resourceType": "OperationOutcome",
            "issue": [{
                "severity": "error",
                "code": "invalid",
                "diagnostics": self.message,
            }]
        });
        (self.status, axum::Json(body)).into_response()
    }
}

#[async_trait]
impl<S> FromRequest<S> for FhirBody
where
    S: Send + Sync,
{
    type Rejection = FhirBodyRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Extract just the media type (before any ;charset= etc.)
        let media_type = content_type.split(';').next().unwrap_or("").trim();

        let is_xml = matches!(
            media_type,
            "application/fhir+xml" | "application/xml" | "text/xml"
        );

        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            FhirBodyRejection {
                status: StatusCode::BAD_REQUEST,
                message: format!("Failed to read request body: {}", e),
            }
        })?;

        if is_xml {
            let xml_str = std::str::from_utf8(&bytes).map_err(|_| FhirBodyRejection {
                status: StatusCode::BAD_REQUEST,
                message: "Request body is not valid UTF-8".to_string(),
            })?;

            let json_str =
                zunder_format::xml_to_json(xml_str).map_err(|e| FhirBodyRejection {
                    status: StatusCode::BAD_REQUEST,
                    message: format!("Invalid FHIR XML: {}", e),
                })?;

            let value: JsonValue =
                serde_json::from_str(&json_str).map_err(|e| FhirBodyRejection {
                    status: StatusCode::BAD_REQUEST,
                    message: format!(
                        "Failed to parse converted XML as JSON (internal error): {}",
                        e
                    ),
                })?;

            Ok(FhirBody(value))
        } else {
            // Default: treat as JSON (covers application/fhir+json, application/json, and missing content-type)
            let value: JsonValue =
                serde_json::from_slice(&bytes).map_err(|e| FhirBodyRejection {
                    status: StatusCode::BAD_REQUEST,
                    message: format!("Invalid JSON in request body: {}", e),
                })?;

            Ok(FhirBody(value))
        }
    }
}

/// Returns `true` if the Content-Type header indicates an XML FHIR body.
fn is_xml_content_type(headers: &HeaderMap) -> bool {
    let ct = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let media_type = ct.split(';').next().unwrap_or("").trim().to_lowercase();
    matches!(
        media_type.as_str(),
        "application/fhir+xml" | "application/xml" | "text/xml"
    )
}

/// Parse a FHIR resource body from raw bytes, converting from XML if needed.
///
/// Use this in handlers that manually read the request body instead of using
/// the [`FhirBody`] extractor.
pub fn parse_fhir_body(bytes: &[u8], headers: &HeaderMap) -> crate::Result<JsonValue> {
    if is_xml_content_type(headers) {
        let xml_str = std::str::from_utf8(bytes).map_err(|_| {
            crate::Error::InvalidResource("Request body is not valid UTF-8".to_string())
        })?;
        let json_str = zunder_format::xml_to_json(xml_str)
            .map_err(|e| crate::Error::InvalidResource(format!("Invalid FHIR XML: {}", e)))?;
        serde_json::from_str(&json_str).map_err(|e| {
            crate::Error::Internal(format!(
                "Failed to parse converted XML as JSON (internal error): {}",
                e
            ))
        })
    } else {
        serde_json::from_slice(bytes).map_err(|e| {
            crate::Error::InvalidResource(format!("Invalid JSON in request body: {}", e))
        })
    }
}
