//! FHIR Content Negotiation
//!
//! Handles content negotiation according to the FHIR specification:
//! - Format selection (_format parameter and Accept header)
//! - Pretty printing (_pretty parameter)
//!
//! Note: _summary and _elements are Search Result Parameters handled by SearchService,
//! not content negotiation. They modify search results, not the response format.
//!
//! See: http://hl7.org/fhir/http.html#parameters

use axum::http::{HeaderMap, HeaderValue};
use std::collections::HashMap;

// ============================================================================
// Content Format
// ============================================================================

/// Supported content formats for FHIR resources
///
/// Per FHIR spec: http://hl7.org/fhir/http.html#parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentFormat {
    /// JSON format (application/fhir+json)
    #[default]
    Json,
    /// XML format (application/fhir+xml)
    Xml,
    /// HTML format (text/html) - for narrative display
    Html,
    /// Turtle RDF format (application/fhir+turtle)
    Turtle,
}

impl ContentFormat {
    /// Parse format from string (e.g., from _format parameter)
    ///
    /// Per FHIR spec (http://hl7.org/fhir/http.html#mime-type), these values should be accepted:
    /// - json, application/json, application/fhir+json -> JSON
    /// - xml, text/xml, application/xml, application/fhir+xml -> XML
    /// - html, text/html -> HTML
    /// - ttl, application/fhir+turtle, text/turtle -> Turtle
    ///
    /// Note: Generic MIME types (application/json, application/xml, text/xml) are also accepted
    /// per the spec for client convenience.
    pub fn parse(s: &str) -> Option<Self> {
        // Strip charset and other parameters from mime type
        let mime_type = s.split(';').next().unwrap_or(s).trim();
        let s_lower = mime_type.to_ascii_lowercase();

        match s_lower.as_str() {
            "json" | "application/json" | "application/fhir+json" => Some(Self::Json),
            "xml" | "text/xml" | "application/xml" | "application/fhir+xml" => Some(Self::Xml),
            "html" | "text/html" => Some(Self::Html),
            "ttl" | "application/fhir+turtle" | "text/turtle" => Some(Self::Turtle),
            _ => None,
        }
    }

    /// Get the MIME type for this format
    ///
    /// Per FHIR spec, the formal MIME types are:
    /// - application/fhir+json for JSON
    /// - application/fhir+xml for XML
    /// - application/fhir+turtle for RDF Turtle
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Json => "application/fhir+json",
            Self::Xml => "application/fhir+xml",
            Self::Html => "text/html",
            Self::Turtle => "application/fhir+turtle",
        }
    }

    /// Get the MIME type with charset parameter
    ///
    /// Per FHIR spec, UTF-8 encoding SHALL be used for FHIR instances.
    pub fn mime_type_with_charset(&self) -> String {
        format!("{}; charset=utf-8", self.mime_type())
    }

    /// Get the Content-Type header value for this format
    pub fn content_type_header(&self) -> HeaderValue {
        HeaderValue::from_static(self.mime_type())
    }

    /// Get browser-friendly MIME type for JSON
    ///
    /// Returns `application/json` instead of `application/fhir+json` for better
    /// browser rendering, while still being valid per FHIR spec.
    pub fn browser_friendly_mime_type(&self) -> &'static str {
        match self {
            Self::Json => "application/json",
            _ => self.mime_type(),
        }
    }

    /// Get the browser-friendly Content-Type header value
    pub fn browser_friendly_content_type_header(&self) -> HeaderValue {
        HeaderValue::from_static(self.browser_friendly_mime_type())
    }

    /// Check if this format is currently supported by the server
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Json | Self::Xml)
    }
}

impl std::str::FromStr for ContentFormat {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

// ============================================================================
// Summary Mode
// ============================================================================

/// Summary mode for resource responses
///
/// Controls how much of the resource is returned.
/// See: http://hl7.org/fhir/search.html#summary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SummaryMode {
    /// Return all parts of the resource (default)
    #[default]
    False,
    /// Return only elements marked as "summary" in the resource definition
    True,
    /// Return only text, id, meta, and top-level mandatory elements
    Text,
    /// Remove the text element
    Data,
    /// Search only: return count of matches without actual resources
    Count,
}

impl SummaryMode {
    /// Parse summary mode from string (from _summary parameter)
    pub fn parse(s: &str) -> Option<Self> {
        let s_lower = s.to_ascii_lowercase();
        match s_lower.as_str() {
            "true" => Some(Self::True),
            "false" => Some(Self::False),
            "text" => Some(Self::Text),
            "data" => Some(Self::Data),
            "count" => Some(Self::Count),
            _ => None,
        }
    }

    /// Check if this mode requires filtering the resource
    pub fn requires_filtering(&self) -> bool {
        !matches!(self, Self::False)
    }
}

impl std::str::FromStr for SummaryMode {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

// ============================================================================
// Content Negotiation Context
// ============================================================================

/// Complete content negotiation context for a request
///
/// Contains all preferences extracted from query parameters and headers.
#[derive(Debug, Clone, Default)]
pub struct ContentNegotiation {
    /// Requested content format
    pub format: ContentFormat,
    /// Requested summary mode
    pub summary: SummaryMode,
    /// Specific elements to include (from _elements parameter)
    pub elements: Option<Vec<String>>,
    /// Pretty print output
    pub pretty: bool,
    /// Whether this is a browser request (for MIME type selection)
    pub is_browser_request: bool,
    /// Whether an explicit FHIR format was requested (via _format param or Accept header)
    pub explicit_fhir_format_requested: bool,
}

impl ContentNegotiation {
    /// Extract content negotiation preferences from request
    ///
    /// Priority for format selection:
    /// 1. _format query parameter (highest)
    /// 2. Accept header (ignored for browser navigation requests)
    /// 3. Default from config
    pub fn from_request(
        query_params: &HashMap<String, String>,
        headers: &HeaderMap,
        default_format: &str,
    ) -> Self {
        // Detect browser requests
        let is_browser_request = Self::is_browser_request(headers);

        // Check if explicit FHIR format was requested
        let explicit_fhir_format_requested = query_params
            .get("_format")
            .map(|s| {
                let s_lower = s.to_lowercase();
                s_lower.contains("fhir+json") || s_lower.contains("fhir+xml")
            })
            .unwrap_or(false)
            || Self::has_explicit_fhir_format_in_accept(headers);

        // Extract format
        // For browser requests, ignore Accept header to avoid matching text/html or application/xml
        // which browsers send when navigating to a URL directly
        let format = query_params
            .get("_format")
            .and_then(|s| ContentFormat::parse(s))
            .or_else(|| {
                if is_browser_request {
                    // Browsers send Accept headers optimized for HTML pages,
                    // so we ignore them and use the default format (JSON)
                    None
                } else {
                    Self::extract_format_from_accept(headers)
                }
            })
            .or_else(|| ContentFormat::parse(default_format))
            .unwrap_or_default();

        // Extract summary mode
        let summary = query_params
            .get("_summary")
            .and_then(|s| SummaryMode::parse(s))
            .unwrap_or_default();

        // Extract elements filter
        let elements = query_params.get("_elements").map(|s| {
            s.split(',')
                .map(|e| e.trim().to_string())
                .collect::<Vec<_>>()
        });

        // Extract pretty flag
        let pretty = query_params
            .get("_pretty")
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false);

        Self {
            format,
            summary,
            elements,
            pretty,
            is_browser_request,
            explicit_fhir_format_requested,
        }
    }

    /// Detect if the request is from a browser
    ///
    /// Checks the User-Agent header for common browser patterns.
    /// This is used to determine whether to use `application/json` (browser-friendly)
    /// or `application/fhir+json` (FHIR-compliant) for JSON responses.
    fn is_browser_request(headers: &HeaderMap) -> bool {
        let user_agent = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // Common browser User-Agent patterns
        user_agent.contains("mozilla")
            && (user_agent.contains("firefox")
                || user_agent.contains("chrome")
                || user_agent.contains("safari")
                || user_agent.contains("edge")
                || user_agent.contains("opera"))
            && !user_agent.contains("bot")
            && !user_agent.contains("crawler")
            && !user_agent.contains("spider")
    }

    /// Extract format from Accept header
    fn extract_format_from_accept(headers: &HeaderMap) -> Option<ContentFormat> {
        let accept = headers.get("accept")?.to_str().ok()?;

        // Parse Accept header and find highest priority supported format
        // Simple implementation - doesn't handle full q-values
        for part in accept.split(',') {
            let media_type = part.split(';').next()?.trim();
            if let Some(format) =
                ContentFormat::parse(media_type).filter(ContentFormat::is_supported)
            {
                return Some(format);
            }
        }

        None
    }

    /// Check if Accept header explicitly requests a FHIR format
    fn has_explicit_fhir_format_in_accept(headers: &HeaderMap) -> bool {
        let accept = match headers.get("accept").and_then(|v| v.to_str().ok()) {
            Some(accept) => accept.to_lowercase(),
            None => return false,
        };

        accept.contains("application/fhir+json") || accept.contains("application/fhir+xml")
    }

    /// Check if format conversion is needed
    pub fn needs_format_conversion(&self) -> bool {
        self.format != ContentFormat::Json
    }

    /// Check if summary filtering is needed
    pub fn needs_summary_filtering(&self) -> bool {
        self.summary.requires_filtering()
    }

    /// Check if elements filtering is needed
    pub fn needs_elements_filtering(&self) -> bool {
        self.elements.is_some()
    }

    /// Get the appropriate Content-Type header based on browser detection
    ///
    /// For browser requests that didn't explicitly request FHIR format,
    /// returns `application/json` instead of `application/fhir+json` for
    /// better browser rendering.
    pub fn response_content_type_header(&self) -> HeaderValue {
        if self.format == ContentFormat::Json
            && self.is_browser_request
            && !self.explicit_fhir_format_requested
        {
            self.format.browser_friendly_content_type_header()
        } else {
            self.format.content_type_header()
        }
    }

    /// Get the appropriate MIME type string based on browser detection
    pub fn response_mime_type(&self) -> &'static str {
        if self.format == ContentFormat::Json
            && self.is_browser_request
            && !self.explicit_fhir_format_requested
        {
            self.format.browser_friendly_mime_type()
        } else {
            self.format.mime_type()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_format_from_str() {
        assert_eq!(ContentFormat::parse("json"), Some(ContentFormat::Json));
        assert_eq!(
            ContentFormat::parse("application/json"),
            Some(ContentFormat::Json)
        );
        assert_eq!(
            ContentFormat::parse("application/fhir+json"),
            Some(ContentFormat::Json)
        );

        assert_eq!(ContentFormat::parse("xml"), Some(ContentFormat::Xml));
        assert_eq!(
            ContentFormat::parse("application/fhir+xml"),
            Some(ContentFormat::Xml)
        );

        assert_eq!(ContentFormat::parse("html"), Some(ContentFormat::Html));
        assert_eq!(ContentFormat::parse("ttl"), Some(ContentFormat::Turtle));

        assert_eq!(ContentFormat::parse("invalid"), None);
    }

    #[test]
    fn test_summary_mode_from_str() {
        assert_eq!(SummaryMode::parse("true"), Some(SummaryMode::True));
        assert_eq!(SummaryMode::parse("false"), Some(SummaryMode::False));
        assert_eq!(SummaryMode::parse("text"), Some(SummaryMode::Text));
        assert_eq!(SummaryMode::parse("data"), Some(SummaryMode::Data));
        assert_eq!(SummaryMode::parse("count"), Some(SummaryMode::Count));
        assert_eq!(SummaryMode::parse("invalid"), None);
    }

    #[test]
    fn test_content_negotiation_from_query() {
        let mut params = HashMap::new();
        params.insert("_format".to_string(), "xml".to_string());
        params.insert("_summary".to_string(), "true".to_string());
        params.insert("_pretty".to_string(), "true".to_string());

        let headers = HeaderMap::new();
        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        assert_eq!(cn.format, ContentFormat::Xml);
        assert_eq!(cn.summary, SummaryMode::True);
        assert!(cn.pretty);
    }

    #[test]
    fn test_content_negotiation_from_accept_header() {
        let params = HashMap::new();
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/fhir+xml".parse().unwrap());

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        assert_eq!(cn.format, ContentFormat::Xml);
    }

    #[test]
    fn test_content_negotiation_default() {
        let params = HashMap::new();
        let headers = HeaderMap::new();
        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        assert_eq!(cn.format, ContentFormat::Json);
        assert_eq!(cn.summary, SummaryMode::False);
        assert!(!cn.pretty);
    }

    #[test]
    fn test_elements_parsing() {
        let mut params = HashMap::new();
        params.insert("_elements".to_string(), "id,name,gender".to_string());

        let headers = HeaderMap::new();
        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        assert_eq!(
            cn.elements,
            Some(vec![
                "id".to_string(),
                "name".to_string(),
                "gender".to_string()
            ])
        );
    }

    #[test]
    fn test_browser_friendly_content_type() {
        let params = HashMap::new();
        let mut headers = HeaderMap::new();
        // Simulate Chrome browser
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                .parse()
                .unwrap(),
        );

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        // Browser request without explicit FHIR format should get application/json
        assert!(cn.is_browser_request);
        assert!(!cn.explicit_fhir_format_requested);
        assert_eq!(cn.response_mime_type(), "application/json");
    }

    #[test]
    fn test_browser_with_explicit_fhir_format() {
        let mut params = HashMap::new();
        params.insert("_format".to_string(), "application/fhir+json".to_string());

        let mut headers = HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                .parse()
                .unwrap(),
        );

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        // Browser request WITH explicit FHIR format should get application/fhir+json
        assert!(cn.is_browser_request);
        assert!(cn.explicit_fhir_format_requested);
        assert_eq!(cn.response_mime_type(), "application/fhir+json");
    }

    #[test]
    fn test_non_browser_request() {
        let params = HashMap::new();
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "FHIR-Client/1.0".parse().unwrap());

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        // Non-browser request should get application/fhir+json
        assert!(!cn.is_browser_request);
        assert_eq!(cn.response_mime_type(), "application/fhir+json");
    }

    #[test]
    fn test_browser_with_html_accept_header() {
        let params = HashMap::new();
        let mut headers = HeaderMap::new();
        // Simulate Chrome browser with typical navigation Accept header
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36"
                .parse()
                .unwrap(),
        );
        headers.insert(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
                .parse()
                .unwrap(),
        );

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        // Browser request should ignore Accept header and use default JSON format
        assert!(cn.is_browser_request);
        assert_eq!(cn.format, ContentFormat::Json);
        assert_eq!(cn.response_mime_type(), "application/json");
    }

    #[test]
    fn test_non_browser_with_xml_accept_header() {
        let params = HashMap::new();
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "FHIR-Client/1.0".parse().unwrap());
        headers.insert("accept", "application/fhir+xml".parse().unwrap());

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        // Non-browser API client should respect Accept header
        assert!(!cn.is_browser_request);
        assert_eq!(cn.format, ContentFormat::Xml);
    }

    #[test]
    fn test_real_chrome_navigation_headers() {
        // Test with actual Chrome navigation headers from user's curl command
        let params = HashMap::new();
        let mut headers = HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36"
                .parse()
                .unwrap(),
        );
        headers.insert(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7"
                .parse()
                .unwrap(),
        );

        let cn = ContentNegotiation::from_request(&params, &headers, "json");

        // Even though Accept header includes application/xml, browser should get JSON
        assert!(cn.is_browser_request);
        assert_eq!(cn.format, ContentFormat::Json);
        assert_eq!(cn.response_mime_type(), "application/json");
    }
}
