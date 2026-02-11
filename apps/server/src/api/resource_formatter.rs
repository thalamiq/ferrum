//! Resource Formatting
//!
//! Handles formatting FHIR resources according to content negotiation:
//! - Format conversion (JSON to XML and vice versa)
//! - Pretty printing
//!
//! Note: Resource filtering (_summary, _elements) is handled by SearchService,
//! not here. This module only handles format conversion.
//!
//! See: http://hl7.org/fhir/http.html#parameters

use crate::api::content_negotiation::{ContentFormat, ContentNegotiation};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use serde_json::Value as JsonValue;

/// Error type for resource formatting operations
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("Format conversion failed: {0}")]
    ConversionFailed(#[from] zunder_format::FormatError),

    #[error("Unsupported format: {0:?}")]
    UnsupportedFormat(ContentFormat),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

impl IntoResponse for FormatError {
    fn into_response(self) -> AxumResponse {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

/// Resource formatter that applies content negotiation preferences
pub struct ResourceFormatter {
    negotiation: ContentNegotiation,
}

impl ResourceFormatter {
    /// Create a new formatter with the given content negotiation context
    pub fn new(negotiation: ContentNegotiation) -> Self {
        Self { negotiation }
    }

    /// Format a FHIR resource according to content negotiation preferences
    ///
    /// This only handles format conversion (JSON/XML) and pretty printing.
    /// Resource filtering (_summary, _elements) is handled by SearchService.
    pub fn format_resource(&self, resource: JsonValue) -> Result<Vec<u8>, FormatError> {
        self.convert_format(resource)
    }

    /// Convert resource to the requested format
    fn convert_format(&self, resource: JsonValue) -> Result<Vec<u8>, FormatError> {
        match self.negotiation.format {
            ContentFormat::Json => {
                // JSON format - serialize with optional pretty printing
                if self.negotiation.pretty {
                    Ok(serde_json::to_vec_pretty(&resource)?)
                } else {
                    Ok(serde_json::to_vec(&resource)?)
                }
            }
            ContentFormat::Xml => {
                // XML format - convert JSON to XML using fhir-format
                let json_str = serde_json::to_string(&resource)?;
                let xml_str = zunder_format::json_to_xml(&json_str)?;
                Ok(xml_str.into_bytes())
            }
            ContentFormat::Html | ContentFormat::Turtle => {
                // These formats are not yet supported
                Err(FormatError::UnsupportedFormat(self.negotiation.format))
            }
        }
    }

    /// Get the Content-Type header value for the current format
    ///
    /// Per FHIR spec, UTF-8 encoding SHALL be used for FHIR instances.
    ///
    /// For browser requests with JSON format, returns `application/json` instead of
    /// `application/fhir+json` to ensure browsers display the JSON inline rather than
    /// downloading it. API clients explicitly requesting `application/fhir+json` will
    /// still receive that format.
    pub fn content_type(&self) -> String {
        format!("{}; charset=utf-8", self.negotiation.response_mime_type())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_format_conversion() {
        let resource = serde_json::json!({
            "resourceType": "Patient",
            "id": "123",
            "name": [{
                "family": "Doe",
                "given": ["John"]
            }]
        });

        let negotiation = ContentNegotiation::default();
        let formatter = ResourceFormatter::new(negotiation);
        let formatted = formatter.format_resource(resource.clone()).unwrap();

        // Should be valid JSON
        let parsed: JsonValue = serde_json::from_slice(&formatted).unwrap();
        assert_eq!(parsed, resource);
    }

    #[test]
    fn test_json_pretty_printing() {
        let resource = serde_json::json!({
            "resourceType": "Patient",
            "id": "123"
        });

        let mut negotiation = ContentNegotiation::default();
        negotiation.pretty = true;

        let formatter = ResourceFormatter::new(negotiation);
        let formatted = formatter.format_resource(resource).unwrap();
        let formatted_str = String::from_utf8(formatted).unwrap();

        // Pretty printed JSON should have newlines
        assert!(formatted_str.contains('\n'));
    }

    #[test]
    fn test_browser_friendly_content_type_header() {
        let mut negotiation = ContentNegotiation::default();
        negotiation.is_browser_request = true;
        negotiation.explicit_fhir_format_requested = false;

        let formatter = ResourceFormatter::new(negotiation);
        let content_type = formatter.content_type();

        // Should return application/json for browsers, not application/fhir+json
        assert_eq!(content_type, "application/json; charset=utf-8");
    }

    #[test]
    fn test_explicit_fhir_format_for_browser() {
        let mut negotiation = ContentNegotiation::default();
        negotiation.is_browser_request = true;
        negotiation.explicit_fhir_format_requested = true;

        let formatter = ResourceFormatter::new(negotiation);
        let content_type = formatter.content_type();

        // Should respect explicit FHIR format request even from browsers
        assert_eq!(content_type, "application/fhir+json; charset=utf-8");
    }

    #[test]
    fn test_non_browser_content_type() {
        let mut negotiation = ContentNegotiation::default();
        negotiation.is_browser_request = false;

        let formatter = ResourceFormatter::new(negotiation);
        let content_type = formatter.content_type();

        // Non-browser clients should get FHIR-compliant content type
        assert_eq!(content_type, "application/fhir+json; charset=utf-8");
    }
}
