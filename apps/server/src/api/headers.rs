//! FHIR HTTP Header Handling
//!
//! Centralized utilities for parsing and formatting FHIR-specific HTTP headers
//! according to the FHIR specification (http://hl7.org/fhir/http.html#headers).
//!
//! # Supported Headers
//!
//! ## Request Headers
//! - `Accept` - Content negotiation (handled by Axum automatically)
//! - `If-Match` - ETag-based conditional requests
//! - `If-Modified-Since` - Date-based conditional read
//! - `If-None-Exist` - Conditional create (HL7 extension)
//! - `If-None-Match` - ETag-based conditional requests
//! - `Prefer` - Request behaviors (return preference, processing preference, etc.)
//!
//! ## Response Headers
//! - `ETag` - Version ID as weak ETag (W/"versionId")
//! - `Last-Modified` - From .meta.lastUpdated
//! - `Location` - Resource location after create/update
//! - `Content-Location` - Async response location

use axum::http::{header, HeaderMap, HeaderValue};
use chrono::{DateTime, Utc};

// ============================================================================
// ETag Handling
// ============================================================================

/// Parse ETag header value to version ID
///
/// FHIR uses weak ETags in the format: `W/"3141"`
/// This function extracts the version ID integer from the ETag string.
///
/// # Examples
/// ```
/// use zunder::api::headers::parse_etag;
/// assert_eq!(parse_etag("W/\"3141\""), Some(3141));
/// assert_eq!(parse_etag("W/\"23\""), Some(23));
/// assert_eq!(parse_etag("invalid"), None);
/// ```
pub fn parse_etag(etag: &str) -> Option<i32> {
    etag.trim_start_matches("W/\"")
        .trim_end_matches('"')
        .parse()
        .ok()
}

/// Format version ID as FHIR ETag header value
///
/// FHIR uses weak ETags prefixed with `W/` and enclosed in quotes.
///
/// # Examples
/// ```
/// use zunder::api::headers::format_etag;
/// assert_eq!(format_etag(3141), "W/\"3141\"");
/// assert_eq!(format_etag(23), "W/\"23\"");
/// ```
pub fn format_etag(version: i32) -> String {
    format!("W/\"{}\"", version)
}

// ============================================================================
// Last-Modified Handling
// ============================================================================

/// Format DateTime as Last-Modified header value
///
/// Converts a FHIR instant (DateTime<Utc>) to RFC 7232 format.
/// FHIR instants are in UTC, so we convert to RFC 2822 format with GMT timezone.
///
/// # Examples
/// ```
/// use chrono::{DateTime, Utc};
/// use zunder::api::headers::format_last_modified;
/// let dt = DateTime::parse_from_rfc3339("2023-01-01T12:00:00Z").unwrap().with_timezone(&Utc);
/// let formatted = format_last_modified(&dt);
/// assert!(formatted.contains("GMT"));
/// ```
pub fn format_last_modified(last_updated: &DateTime<Utc>) -> String {
    last_updated.to_rfc2822().replace("+0000", "GMT")
}

// ============================================================================
// Prefer Header Handling
// ============================================================================

/// Prefer header return preference
///
/// Controls what the server returns in the response body.
/// See: http://hl7.org/fhir/http.html#prefer
///
/// Note: The #[default] attribute provides a fallback value for the Default trait.
/// The actual default behavior when no Prefer header is present should be
/// determined by server configuration (see FhirConfig.default_prefer_return).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferReturn {
    /// Return minimal response (status/location/etag only, no resource body)
    Minimal,
    /// Return full resource representation (recommended default per FHIR spec)
    #[default]
    Representation,
    /// Return OperationOutcome resource containing hints and warnings
    /// instead of the full resource
    OperationOutcome,
}

/// Prefer header handling preference (for search operations)
///
/// Controls how the server handles unknown or unsupported search parameters.
/// See: http://hl7.org/fhir/search.html#errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferHandling {
    /// Ignore unknown or unsupported parameters (default behavior)
    #[default]
    Lenient,
    /// Return an error for any unknown or unsupported parameter
    Strict,
}

/// Complete Prefer header preferences
///
/// Contains both return and handling preferences parsed from the Prefer header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferPreferences {
    pub return_pref: PreferReturn,
    pub handling: PreferHandling,
}

impl Default for PreferPreferences {
    fn default() -> Self {
        Self {
            return_pref: PreferReturn::Representation,
            handling: PreferHandling::default(),
        }
    }
}

/// Extract all Prefer header preferences
///
/// Parses the `Prefer` header to extract both return and handling preferences.
/// The Prefer header can contain multiple preferences separated by commas.
///
/// # Examples
/// ```
/// use axum::http::HeaderMap;
/// use zunder::api::headers::{extract_prefer_preferences, PreferHandling, PreferReturn};
/// let mut headers = HeaderMap::new();
/// headers.insert("prefer", "return=minimal, handling=strict".parse().unwrap());
/// let prefs = extract_prefer_preferences(&headers);
/// assert_eq!(prefs.return_pref, PreferReturn::Minimal);
/// assert_eq!(prefs.handling, PreferHandling::Strict);
/// ```
pub fn extract_prefer_preferences(headers: &HeaderMap) -> PreferPreferences {
    let prefer_value = headers
        .get("prefer")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase());

    if let Some(prefer_str) = prefer_value {
        // Parse return preference
        let return_pref = if prefer_str.contains("return=operationoutcome") {
            PreferReturn::OperationOutcome
        } else if prefer_str.contains("return=representation") {
            PreferReturn::Representation
        } else if prefer_str.contains("return=minimal") {
            PreferReturn::Minimal
        } else {
            PreferReturn::default()
        };

        // Parse handling preference
        let handling = if prefer_str.contains("handling=strict") {
            PreferHandling::Strict
        } else if prefer_str.contains("handling=lenient") {
            PreferHandling::Lenient
        } else {
            PreferHandling::default()
        };

        PreferPreferences {
            return_pref,
            handling,
        }
    } else {
        PreferPreferences::default()
    }
}

/// Extract Prefer header return preference
///
/// Parses the `Prefer` header to determine if the client wants
/// `return=minimal`, `return=representation`, or `return=OperationOutcome`.
///
/// # Examples
/// ```
/// use axum::http::HeaderMap;
/// use zunder::api::headers::{extract_prefer_return, PreferReturn};
/// let mut headers = HeaderMap::new();
/// headers.insert("prefer", "return=minimal".parse().unwrap());
/// assert_eq!(extract_prefer_return(&headers), PreferReturn::Minimal);
/// ```
pub fn extract_prefer_return(headers: &HeaderMap) -> PreferReturn {
    extract_prefer_preferences(headers).return_pref
}

/// Extract Prefer header handling preference
///
/// Parses the `Prefer` header to determine if the client wants
/// `handling=strict` or `handling=lenient` for search operations.
///
/// # Examples
/// ```
/// use axum::http::HeaderMap;
/// use zunder::api::headers::{extract_prefer_handling, PreferHandling};
/// let mut headers = HeaderMap::new();
/// headers.insert("prefer", "handling=strict".parse().unwrap());
/// assert_eq!(extract_prefer_handling(&headers), PreferHandling::Strict);
/// ```
pub fn extract_prefer_handling(headers: &HeaderMap) -> PreferHandling {
    extract_prefer_preferences(headers).handling
}

/// Check if client prefers minimal response
///
/// Convenience function that returns true if Prefer header contains `return=minimal`.
pub fn prefer_minimal(headers: &HeaderMap) -> bool {
    extract_prefer_return(headers) == PreferReturn::Minimal
}

/// Check if client prefers OperationOutcome response
///
/// Returns true if Prefer header contains `return=OperationOutcome`.
pub fn prefer_operation_outcome(headers: &HeaderMap) -> bool {
    extract_prefer_return(headers) == PreferReturn::OperationOutcome
}

/// Check if client prefers strict handling
///
/// Returns true if Prefer header contains `handling=strict`.
pub fn prefer_strict_handling(headers: &HeaderMap) -> bool {
    extract_prefer_handling(headers) == PreferHandling::Strict
}

/// Extract full Prefer header value as string
///
/// Returns the raw Prefer header value if present.
pub fn get_prefer_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get("prefer")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

// ============================================================================
// Conditional Request Headers
// ============================================================================

/// Extract If-Match header value as version ID
///
/// Used for conditional updates. Returns the expected version ID if present.
pub fn extract_if_match(headers: &HeaderMap) -> Option<i32> {
    headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_etag)
}

/// Extract If-None-Match header value as ETag string
///
/// Used for conditional reads. Returns the ETag value if present.
pub fn extract_if_none_match(headers: &HeaderMap) -> Option<String> {
    headers
        .get("if-none-match")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Extract If-Modified-Since header value as string
///
/// Used for conditional reads. Returns the date string if present.
pub fn extract_if_modified_since(headers: &HeaderMap) -> Option<String> {
    headers
        .get("if-modified-since")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Extract If-None-Exist header value
///
/// HL7-defined extension header for conditional create.
/// Returns the query string value if present.
pub fn extract_if_none_exist(headers: &HeaderMap) -> Option<String> {
    headers
        .get("if-none-exist")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

// ============================================================================
// Response Header Building
// ============================================================================

/// Builder for FHIR response headers
///
/// Provides a convenient way to build standard FHIR response headers.
#[derive(Debug, Clone)]
pub struct FhirResponseHeaders {
    pub location: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_location: Option<String>,
    pub cache_control: Option<String>,
}

impl FhirResponseHeaders {
    /// Create a new empty header builder
    pub fn new() -> Self {
        Self {
            location: None,
            etag: None,
            last_modified: None,
            content_location: None,
            cache_control: None,
        }
    }

    /// Set Location header
    pub fn with_location(mut self, location: String) -> Self {
        self.location = Some(location);
        self
    }

    /// Set ETag header from version ID
    pub fn with_etag(mut self, version_id: i32) -> Self {
        self.etag = Some(format_etag(version_id));
        self
    }

    /// Set Last-Modified header from DateTime
    pub fn with_last_modified(mut self, last_updated: &DateTime<Utc>) -> Self {
        self.last_modified = Some(format_last_modified(last_updated));
        self
    }

    /// Set Content-Location header
    pub fn with_content_location(mut self, content_location: String) -> Self {
        self.content_location = Some(content_location);
        self
    }

    /// Set Cache-Control header
    pub fn with_cache_control(mut self, value: impl Into<String>) -> Self {
        self.cache_control = Some(value.into());
        self
    }

    pub fn with_cache_control_no_cache(self) -> Self {
        self.with_cache_control("no-cache")
    }

    pub fn with_cache_control_max_age(self, seconds: u32) -> Self {
        self.with_cache_control(format!("max-age={}", seconds))
    }

    pub fn with_cache_control_immutable(self, seconds: u32) -> Self {
        self.with_cache_control(format!("max-age={}, immutable", seconds))
    }

    /// Build headers for create/update response
    ///
    /// Sets Location, ETag, and Last-Modified headers.
    pub fn for_create_update(
        resource_type: &str,
        resource_id: &str,
        version_id: i32,
        last_updated: &DateTime<Utc>,
    ) -> Self {
        Self::new()
            .with_location(format!(
                "{}/{}/_history/{}",
                resource_type, resource_id, version_id
            ))
            .with_etag(version_id)
            .with_last_modified(last_updated)
    }

    /// Build headers for read response
    ///
    /// Sets ETag and Last-Modified headers.
    pub fn for_read(version_id: i32, last_updated: &DateTime<Utc>) -> Self {
        Self::new()
            .with_etag(version_id)
            .with_last_modified(last_updated)
    }

    /// Convert to Axum header array for use in responses
    ///
    /// Returns an array of (HeaderName, HeaderValue) tuples that can be used
    /// with Axum's response builders. The array size is determined at compile time
    /// based on which headers are set.
    pub fn to_header_array(&self) -> Vec<(header::HeaderName, HeaderValue)> {
        let mut headers = Vec::new();

        if let Some(ref location) = self.location {
            if let Ok(value) = HeaderValue::from_str(location) {
                headers.push((header::LOCATION, value));
            }
        }

        if let Some(ref etag) = self.etag {
            if let Ok(value) = HeaderValue::from_str(etag) {
                headers.push((header::ETAG, value));
            }
        }

        if let Some(ref last_modified) = self.last_modified {
            if let Ok(value) = HeaderValue::from_str(last_modified) {
                headers.push((header::LAST_MODIFIED, value));
            }
        }

        if let Some(ref content_location) = self.content_location {
            if let Ok(value) = HeaderValue::from_str(content_location) {
                headers.push((header::CONTENT_LOCATION, value));
            }
        }

        if let Some(ref cache_control) = self.cache_control {
            if let Ok(value) = HeaderValue::from_str(cache_control) {
                headers.push((header::CACHE_CONTROL, value));
            }
        }

        headers
    }

    /// Convert to Axum response with headers
    ///
    /// Helper method that builds a Response with the headers set.
    /// This can be used directly with `into_response()`.
    pub fn apply_to_response(
        &self,
        mut response: axum::response::Response,
    ) -> axum::response::Response {
        let headers = response.headers_mut();

        if let Some(ref location) = self.location {
            if let Ok(value) = HeaderValue::from_str(location) {
                headers.insert(header::LOCATION, value);
            }
        }

        if let Some(ref etag) = self.etag {
            if let Ok(value) = HeaderValue::from_str(etag) {
                headers.insert(header::ETAG, value);
            }
        }

        if let Some(ref last_modified) = self.last_modified {
            if let Ok(value) = HeaderValue::from_str(last_modified) {
                headers.insert(header::LAST_MODIFIED, value);
            }
        }

        if let Some(ref content_location) = self.content_location {
            if let Ok(value) = HeaderValue::from_str(content_location) {
                headers.insert(header::CONTENT_LOCATION, value);
            }
        }

        if let Some(ref cache_control) = self.cache_control {
            if let Ok(value) = HeaderValue::from_str(cache_control) {
                headers.insert(header::CACHE_CONTROL, value);
            }
        }

        response
    }
}

impl Default for FhirResponseHeaders {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_parse_etag() {
        assert_eq!(parse_etag("W/\"3141\""), Some(3141));
        assert_eq!(parse_etag("W/\"23\""), Some(23));
        assert_eq!(parse_etag("W/\"1\""), Some(1));
        assert_eq!(parse_etag("invalid"), None);
        assert_eq!(parse_etag(""), None);
    }

    #[test]
    fn test_format_etag() {
        assert_eq!(format_etag(3141), "W/\"3141\"");
        assert_eq!(format_etag(23), "W/\"23\"");
        assert_eq!(format_etag(1), "W/\"1\"");
    }

    #[test]
    fn test_extract_prefer_return() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            extract_prefer_return(&headers),
            PreferReturn::Representation
        );

        headers.insert("prefer", "return=minimal".parse().unwrap());
        assert_eq!(extract_prefer_return(&headers), PreferReturn::Minimal);

        headers.insert("prefer", "return=representation".parse().unwrap());
        assert_eq!(
            extract_prefer_return(&headers),
            PreferReturn::Representation
        );

        headers.insert("prefer", "return=OperationOutcome".parse().unwrap());
        assert_eq!(
            extract_prefer_return(&headers),
            PreferReturn::OperationOutcome
        );

        headers.insert("prefer", "return=minimal, respond-async".parse().unwrap());
        assert_eq!(extract_prefer_return(&headers), PreferReturn::Minimal);
    }

    #[test]
    fn test_extract_prefer_handling() {
        let mut headers = HeaderMap::new();
        assert_eq!(extract_prefer_handling(&headers), PreferHandling::Lenient);

        headers.insert("prefer", "handling=strict".parse().unwrap());
        assert_eq!(extract_prefer_handling(&headers), PreferHandling::Strict);

        headers.insert("prefer", "handling=lenient".parse().unwrap());
        assert_eq!(extract_prefer_handling(&headers), PreferHandling::Lenient);
    }

    #[test]
    fn test_extract_prefer_preferences() {
        let mut headers = HeaderMap::new();
        let prefs = extract_prefer_preferences(&headers);
        assert_eq!(prefs.return_pref, PreferReturn::Representation);
        assert_eq!(prefs.handling, PreferHandling::Lenient);

        headers.insert(
            "prefer",
            "return=representation, handling=strict".parse().unwrap(),
        );
        let prefs = extract_prefer_preferences(&headers);
        assert_eq!(prefs.return_pref, PreferReturn::Representation);
        assert_eq!(prefs.handling, PreferHandling::Strict);

        headers.insert(
            "prefer",
            "return=OperationOutcome, handling=lenient".parse().unwrap(),
        );
        let prefs = extract_prefer_preferences(&headers);
        assert_eq!(prefs.return_pref, PreferReturn::OperationOutcome);
        assert_eq!(prefs.handling, PreferHandling::Lenient);
    }

    #[test]
    fn test_prefer_operation_outcome() {
        let mut headers = HeaderMap::new();
        assert!(!prefer_operation_outcome(&headers));

        headers.insert("prefer", "return=OperationOutcome".parse().unwrap());
        assert!(prefer_operation_outcome(&headers));
    }

    #[test]
    fn test_prefer_strict_handling() {
        let mut headers = HeaderMap::new();
        assert!(!prefer_strict_handling(&headers));

        headers.insert("prefer", "handling=strict".parse().unwrap());
        assert!(prefer_strict_handling(&headers));
    }

    #[test]
    fn test_fhir_response_headers() {
        let dt = DateTime::parse_from_rfc3339("2023-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let headers = FhirResponseHeaders::for_create_update("Patient", "123", 5, &dt);
        let header_array = headers.to_header_array();

        assert_eq!(header_array.len(), 3);
        assert!(headers.location.is_some());
        assert!(headers.etag.is_some());
        assert!(headers.last_modified.is_some());
    }
}
