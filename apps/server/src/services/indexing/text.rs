//! Text extraction for _text and _content search parameters
//!
//! This module provides extraction logic for:
//! - `_text`: Extracts narrative text (resource.text.div)
//! - `_content`: Extracts all human-readable textual content from a resource
//!
//! See: spec/search/03-02-01-08-01-01-_content.md
//!      spec/search/03-02-01-08-01-13-_text.md

use serde_json::Value;
use std::collections::HashSet;

/// Extract narrative text from a FHIR resource for _text search parameter
///
/// Extracts the narrative XHTML from resource.text.div, strips HTML tags,
/// and returns plain text suitable for full-text search.
///
/// # Spec Reference
/// The _text parameter searches narrative content only (typically resource.text).
pub fn extract_narrative_text(resource: &Value) -> String {
    let mut text_parts = Vec::new();

    // Extract resource.text.div (XHTML narrative)
    if let Some(text_div) = resource.get("text").and_then(|t| t.get("div")) {
        if let Some(div_str) = text_div.as_str() {
            let plain = strip_html(div_str);
            push_clean(&mut text_parts, plain);
        }
    }

    text_parts.join(" ")
}

/// Extract all textual content from a FHIR resource for _content search parameter
///
/// Extracts human-readable text from:
/// - Narrative (resource.text.div)
/// - All .text fields (code.text, note.text, etc.)
/// - All .display fields (code.display, subject.display, etc.)
/// - HumanName fields (given, family, prefix, suffix)
/// - Address fields (line, city, state, postalCode, country)
/// - Annotation.text
/// - ContactPoint.value
///
/// # Spec Reference
/// The _content parameter searches all textual content of a resource, including
/// narratives, display text, notes, and other human-readable fields.
pub fn extract_all_textual_content(resource: &Value) -> String {
    let mut text_parts = Vec::new();

    // 1. Extract narrative text
    let narrative = extract_narrative_text(resource);
    push_clean(&mut text_parts, narrative);

    // 2. Recursively extract text and display fields
    extract_text_and_display_fields(resource, &mut text_parts);

    // 3. Extract from well-known complex types
    extract_from_complex_types(resource, &mut text_parts);

    dedupe_preserve_order(text_parts).join(" ")
}

/// Recursively extract all .text and .display fields from a JSON value
fn extract_text_and_display_fields(value: &Value, accumulator: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                match key.as_str() {
                    // Extract .text fields (but skip resource.text.div since we handle that separately)
                    "text" => {
                        if let Some(s) = val.as_str() {
                            push_clean(accumulator, s.to_string());
                        }
                        extract_text_and_display_fields(val, accumulator);
                        continue;
                    }
                    // Extract .display fields
                    "display" => {
                        if let Some(s) = val.as_str() {
                            push_clean(accumulator, s.to_string());
                        }
                        extract_text_and_display_fields(val, accumulator);
                        continue;
                    }
                    _ => {
                        // Recurse into nested objects and arrays
                        extract_text_and_display_fields(val, accumulator);
                    }
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                extract_text_and_display_fields(item, accumulator);
            }
        }
        _ => {}
    }
}

/// Extract text from complex FHIR data types
fn extract_from_complex_types(value: &Value, accumulator: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            // Check if this is a HumanName
            if has_humanname_fields(map) {
                extract_from_humanname(value, accumulator);
            }
            // Check if this is an Address
            else if has_address_fields(map) {
                extract_from_address(value, accumulator);
            }
            // Check if this is an Annotation
            else if map.contains_key("authorReference") || map.contains_key("authorString") {
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    push_clean(accumulator, text.to_string());
                }
            }
            // Check if this is a ContactPoint
            else if map.get("system").is_some() && map.get("value").is_some() {
                if let Some(value_str) = map.get("value").and_then(|v| v.as_str()) {
                    push_clean(accumulator, value_str.to_string());
                }
            }

            // Recurse into all nested values
            for val in map.values() {
                match val {
                    Value::Array(arr) => {
                        for item in arr {
                            extract_from_complex_types(item, accumulator);
                        }
                    }
                    Value::Object(_) => {
                        extract_from_complex_types(val, accumulator);
                    }
                    _ => {}
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                extract_from_complex_types(item, accumulator);
            }
        }
        _ => {}
    }
}

/// Extract text from HumanName (given, family, prefix, suffix, text)
fn extract_from_humanname(name: &Value, accumulator: &mut Vec<String>) {
    if let Some(obj) = name.as_object() {
        // Extract text field (pre-formatted full name)
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            push_clean(accumulator, text.to_string());
        }

        // Extract family name
        if let Some(family) = obj.get("family").and_then(|v| v.as_str()) {
            push_clean(accumulator, family.to_string());
        }

        // Extract given names (array)
        if let Some(given_arr) = obj.get("given").and_then(|v| v.as_array()) {
            for given in given_arr {
                if let Some(s) = given.as_str() {
                    push_clean(accumulator, s.to_string());
                }
            }
        }

        // Extract prefixes (array)
        if let Some(prefix_arr) = obj.get("prefix").and_then(|v| v.as_array()) {
            for prefix in prefix_arr {
                if let Some(s) = prefix.as_str() {
                    push_clean(accumulator, s.to_string());
                }
            }
        }

        // Extract suffixes (array)
        if let Some(suffix_arr) = obj.get("suffix").and_then(|v| v.as_array()) {
            for suffix in suffix_arr {
                if let Some(s) = suffix.as_str() {
                    push_clean(accumulator, s.to_string());
                }
            }
        }
    }
}

/// Extract text from Address (line, city, district, state, postalCode, country, text)
fn extract_from_address(address: &Value, accumulator: &mut Vec<String>) {
    if let Some(obj) = address.as_object() {
        // Extract text field (pre-formatted address)
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            push_clean(accumulator, text.to_string());
        }

        // Extract line (array of street address lines)
        if let Some(line_arr) = obj.get("line").and_then(|v| v.as_array()) {
            for line in line_arr {
                if let Some(s) = line.as_str() {
                    push_clean(accumulator, s.to_string());
                }
            }
        }

        // Extract individual address components
        for field in ["city", "district", "state", "postalCode", "country"] {
            if let Some(s) = obj.get(field).and_then(|v| v.as_str()) {
                push_clean(accumulator, s.to_string());
            }
        }
    }
}

fn push_clean(acc: &mut Vec<String>, value: String) {
    let value = value.trim();
    if !value.is_empty() {
        acc.push(value.to_string());
    }
}

fn dedupe_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        if seen.insert(v.clone()) {
            out.push(v);
        }
    }
    out
}

/// Check if an object looks like a HumanName
fn has_humanname_fields(map: &serde_json::Map<String, Value>) -> bool {
    map.contains_key("family")
        || map.contains_key("given")
        || map.contains_key("prefix")
        || map.contains_key("suffix")
}

/// Check if an object looks like an Address
fn has_address_fields(map: &serde_json::Map<String, Value>) -> bool {
    map.contains_key("line")
        || map.contains_key("city")
        || map.contains_key("state")
        || map.contains_key("postalCode")
        || map.contains_key("country")
}

/// Strip HTML tags from XHTML content, keeping only plain text
///
/// Simple tag stripper - removes all <tag> and </tag>, converts common HTML entities.
/// This is sufficient for search indexing purposes.
fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut last_was_space = true;

    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            '>' => {
                in_tag = false;
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            }
            _ if !in_tag => {
                result.push(c);
                last_was_space = c.is_whitespace();
            }
            _ => {}
        }
    }

    // Decode common HTML entities
    let decoded = result
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    // Normalize whitespace
    decoded
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_narrative_text() {
        let resource = json!({
            "resourceType": "Patient",
            "text": {
                "status": "generated",
                "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">John Doe, born 1980-01-01</div>"
            }
        });

        let text = extract_narrative_text(&resource);
        assert_eq!(text, "John Doe, born 1980-01-01");
    }

    #[test]
    fn test_strip_html() {
        assert_eq!(strip_html("<div>Hello <b>World</b></div>"), "Hello World");
        assert_eq!(
            strip_html("&lt;tag&gt; &amp; &quot;text&quot;"),
            "<tag> & \"text\""
        );
    }

    #[test]
    fn test_extract_all_textual_content() {
        let resource = json!({
            "resourceType": "Patient",
            "text": {
                "status": "generated",
                "div": "<div>Patient summary</div>"
            },
            "name": [{
                "family": "Doe",
                "given": ["John", "Q"]
            }],
            "address": [{
                "line": ["123 Main St"],
                "city": "Boston",
                "state": "MA",
                "postalCode": "02115"
            }],
            "telecom": [{
                "system": "phone",
                "value": "555-1234"
            }]
        });

        let content = extract_all_textual_content(&resource);
        assert!(content.contains("Patient summary"));
        assert!(content.contains("Doe"));
        assert!(content.contains("John"));
        assert!(content.contains("Boston"));
        assert!(content.contains("555-1234"));
    }

    #[test]
    fn test_extract_text_and_display_fields() {
        let resource = json!({
            "code": {
                "coding": [{
                    "system": "http://loinc.org",
                    "code": "12345-6",
                    "display": "Blood Pressure"
                }],
                "text": "BP Reading"
            },
            "note": [{
                "text": "Patient was anxious"
            }]
        });

        let content = extract_all_textual_content(&resource);
        assert!(content.contains("Blood Pressure"));
        assert!(content.contains("BP Reading"));
        assert!(content.contains("Patient was anxious"));
    }

    #[test]
    fn test_empty_resource() {
        let resource = json!({
            "resourceType": "Patient"
        });

        let narrative = extract_narrative_text(&resource);
        let content = extract_all_textual_content(&resource);

        assert_eq!(narrative, "");
        assert_eq!(content, "");
    }
}
