//! Search extraction utilities used by indexing

// ============================================================================
// Search Extraction Utilities (inlined from fhir-search-utils)
// ============================================================================

use chrono::{
    DateTime, Duration, FixedOffset, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime,
    TimeZone, Utc,
};
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

/// Token value extracted from FHIR resources (Coding, CodeableConcept, Identifier)
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TokenValue {
    pub(super) system: Option<String>,
    pub(super) code: Option<String>,
    pub(super) display: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct IdentifierOfTypeRow {
    pub(super) type_system: Option<String>,
    pub(super) type_code: Option<String>,
    pub(super) value: Option<String>,
}

/// Quantity value with unit information
///
/// Per FHIR spec, Quantity has two unit-related fields:
/// - code: The coded unit value (e.g., "mg") - typically from a code system like UCUM
/// - unit: The human-readable display (e.g., "milligrams")
///
/// Both are indexed separately to support different search semantics:
/// - When system+code specified: search only against code (precise matching)
/// - When ||code specified: search against BOTH code and unit (flexible matching)
#[derive(Debug, Clone, PartialEq)]
pub(super) struct QuantityValue {
    pub(super) value: Decimal,
    pub(super) system: Option<String>,
    pub(super) code: Option<String>,
    pub(super) unit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ReferenceKind {
    Relative,
    Absolute,
    Canonical,
    Fragment,
}

impl ReferenceKind {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Relative => "relative",
            Self::Absolute => "absolute",
            Self::Canonical => "canonical",
            Self::Fragment => "fragment",
        }
    }
}

/// Reference value with parsed fields for spec-compliant matching
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ReferenceValue {
    pub(super) reference_kind: ReferenceKind,
    pub(super) target_type: String,
    pub(super) target_id: String,
    /// Version part when reference contains `/_history/{version}`.
    /// Empty string means "no version specified".
    pub(super) target_version_id: String,
    /// Full absolute URL for absolute references (normalized, no trailing slash).
    /// Empty string for non-absolute references.
    pub(super) target_url: String,
    /// Canonical URL (normalized, no trailing slash).
    /// Empty string for non-canonical references.
    pub(super) canonical_url: String,
    /// Canonical version part after `|` (may be partial).
    /// Empty string means "no version specified".
    pub(super) canonical_version: String,
    pub(super) display: Option<String>,
}

// ============================================================================
// Token Extraction (from Coding, CodeableConcept, Identifier, ContactPoint)
// ============================================================================

/// Extract token values from JSON
pub(super) fn extract_tokens(value: &Value) -> Vec<TokenValue> {
    let mut tokens = Vec::new();
    extract_tokens_into(value, &mut tokens);
    tokens
}

fn extract_tokens_into(value: &Value, tokens: &mut Vec<TokenValue>) {
    match value {
        Value::Object(obj) => {
            if let Some(codings) = obj.get("coding") {
                extract_tokens_into(codings, tokens);
                return;
            }

            let system = extract_string_field(obj, "system");
            let code = extract_string_field(obj, "code");
            let display = extract_string_field(obj, "display");

            if code.is_some() {
                tokens.push(TokenValue {
                    system,
                    code,
                    display,
                });
                return;
            }

            let value = extract_string_field(obj, "value");
            if value.is_some() {
                let system = match system.as_deref() {
                    Some(system_value) if is_contact_point_system(system_value) => None,
                    _ => system,
                };
                tokens.push(TokenValue {
                    system,
                    code: value,
                    display: None,
                });
            }
        }
        Value::Array(values) => {
            for value in values {
                extract_tokens_into(value, tokens);
            }
        }
        Value::String(value) => {
            tokens.push(TokenValue {
                system: None,
                code: Some(value.clone()),
                display: None,
            });
        }
        Value::Bool(value) => {
            tokens.push(TokenValue {
                system: None,
                code: Some(if *value { "true" } else { "false" }.to_string()),
                display: None,
            });
        }
        Value::Number(value) => {
            tokens.push(TokenValue {
                system: None,
                code: Some(value.to_string()),
                display: None,
            });
        }
        _ => {}
    }
}

pub(super) fn extract_identifier_of_type_rows(value: &Value) -> Vec<IdentifierOfTypeRow> {
    let mut rows = Vec::new();
    extract_identifier_of_type_rows_into(value, &mut rows);
    rows
}

fn extract_identifier_of_type_rows_into(value: &Value, out: &mut Vec<IdentifierOfTypeRow>) {
    match value {
        Value::Array(items) => {
            for item in items {
                extract_identifier_of_type_rows_into(item, out);
            }
        }
        Value::Object(obj) => {
            let identifier_value = obj.get("value").and_then(extract_string_value);
            let Some(type_obj) = obj.get("type") else {
                return;
            };
            let Some(value_str) = identifier_value else {
                return;
            };
            if value_str.trim().is_empty() {
                return;
            }

            // Identifier.type is CodeableConcept; capture each Coding as an of-type row.
            let mut codings = Vec::new();
            if let Value::Object(type_map) = type_obj {
                if let Some(coding_val) = type_map.get("coding") {
                    codings.push(coding_val);
                }
            }

            for coding_val in codings {
                match coding_val {
                    Value::Array(items) => {
                        for item in items {
                            if let Value::Object(coding_obj) = item {
                                let type_system = extract_string_field(coding_obj, "system")
                                    .filter(|s| !s.is_empty());
                                let type_code = extract_string_field(coding_obj, "code")
                                    .filter(|s| !s.is_empty());
                                if type_code.is_none() {
                                    continue;
                                }
                                out.push(IdentifierOfTypeRow {
                                    type_system,
                                    type_code,
                                    value: Some(value_str.clone()),
                                });
                            }
                        }
                    }
                    Value::Object(coding_obj) => {
                        let type_system =
                            extract_string_field(coding_obj, "system").filter(|s| !s.is_empty());
                        let type_code =
                            extract_string_field(coding_obj, "code").filter(|s| !s.is_empty());
                        if type_code.is_none() {
                            continue;
                        }
                        out.push(IdentifierOfTypeRow {
                            type_system,
                            type_code,
                            value: Some(value_str.clone()),
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

// ============================================================================
// String Extraction (from HumanName, Address, etc.)
// ============================================================================

/// Extract string values from JSON
pub(super) fn extract_strings(value: &Value) -> Vec<String> {
    let mut values = Vec::new();
    extract_strings_into(value, &mut values);
    values
}

fn extract_strings_into(value: &Value, values: &mut Vec<String>) {
    match value {
        Value::String(value) => {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                values.push(trimmed.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_strings_into(item, values);
            }
        }
        Value::Object(obj) => {
            if let Some(value) = obj.get("value") {
                extract_strings_into(value, values);
                return;
            }

            if let Some(text) = obj.get("text") {
                extract_strings_into(text, values);
            }

            if let Some(family) = obj.get("family").and_then(|v| v.as_str()) {
                let trimmed = family.trim();
                if !trimmed.is_empty() {
                    for part in split_string_parts(trimmed) {
                        values.push(part);
                    }
                }
            }

            for field in [
                "family",
                "given",
                "prefix",
                "suffix",
                "line",
                "city",
                "state",
                "postalCode",
                "country",
            ] {
                if let Some(value) = obj.get(field) {
                    extract_strings_into(value, values);
                }
            }
        }
        _ => {}
    }
}

fn split_string_parts(input: &str) -> Vec<String> {
    input
        .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | ',' | ';' | '.'))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

// ============================================================================
// Number Extraction
// ============================================================================

/// Extract numeric values from JSON as Decimal to preserve precision
///
/// Per FHIR spec, numbers are stored as exact values. Precision ranges
/// are calculated from search parameters during query time, not indexing time.
pub(super) fn extract_numbers(value: &Value) -> Vec<Decimal> {
    let mut values = Vec::new();
    extract_numbers_into(value, &mut values);
    values
}

fn extract_numbers_into(value: &Value, values: &mut Vec<Decimal>) {
    match value {
        Value::Number(value) => {
            // serde_json::Number doesn't expose the original string, so we need to convert
            // through f64. This is a limitation, but f64 has ~15-17 decimal digits of precision
            // which should be sufficient for most FHIR numeric values.
            if let Some(number) = value.as_f64() {
                // Convert f64 to Decimal using string representation to preserve precision
                // This works well for most cases since f64 preserves significant digits
                if let Ok(decimal) = Decimal::from_str(&number.to_string()) {
                    values.push(decimal);
                }
            }
        }
        Value::String(value) => {
            // Parse string directly as Decimal to preserve precision
            // This is the preferred path when the original JSON had a string number
            if let Ok(decimal) = Decimal::from_str(value) {
                values.push(decimal);
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_numbers_into(item, values);
            }
        }
        Value::Object(obj) => {
            if let Some(value) = obj.get("value") {
                extract_numbers_into(value, values);
            }
        }
        _ => {}
    }
}

// ============================================================================
// Quantity Extraction
// ============================================================================

/// Extract quantity values with units
pub(super) fn extract_quantity_values(value: &Value) -> Vec<QuantityValue> {
    let mut values = Vec::new();
    extract_quantity_values_into(value, &mut values);
    values
}

fn extract_quantity_values_into(value: &Value, values: &mut Vec<QuantityValue>) {
    match value {
        Value::Array(items) => {
            for item in items {
                extract_quantity_values_into(item, values);
            }
        }
        Value::Object(obj) => {
            if let Some(quantity) = obj.get("valueQuantity") {
                extract_quantity_values_into(quantity, values);
                return;
            }

            let numbers = obj.get("value").map(extract_numbers).unwrap_or_default();
            if numbers.is_empty() {
                return;
            }

            let system =
                extract_string_field(obj, "system").filter(|value| !value.trim().is_empty());

            // Extract both code and unit separately per FHIR spec
            let code = extract_string_field(obj, "code").filter(|value| !value.trim().is_empty());
            let unit = extract_string_field(obj, "unit").filter(|value| !value.trim().is_empty());

            for number in numbers {
                values.push(QuantityValue {
                    value: number,
                    system: system.clone(),
                    code: code.clone(),
                    unit: unit.clone(),
                });
            }
        }
        Value::Number(_) | Value::String(_) => {
            for number in extract_numbers(value) {
                values.push(QuantityValue {
                    value: number,
                    system: None,
                    code: None,
                    unit: None,
                });
            }
        }
        _ => {}
    }
}

// ============================================================================
// Reference Extraction
// ============================================================================

/// Extract and parse reference values
pub(super) fn extract_reference_values(value: &Value) -> Vec<ReferenceValue> {
    let mut values = Vec::new();
    extract_reference_values_into(value, &mut values);
    values
}

fn extract_reference_values_into(value: &Value, values: &mut Vec<ReferenceValue>) {
    match value {
        Value::Array(items) => {
            for item in items {
                extract_reference_values_into(item, values);
            }
        }
        Value::Object(obj) => {
            let display = obj
                .get("display")
                .and_then(extract_string_value)
                .filter(|s| !s.is_empty());
            if let Some(reference) = obj.get("reference").and_then(extract_string_value) {
                if let Some(parsed) = parse_reference(&reference, display.as_deref()) {
                    values.push(parsed);
                }
            } else if let Some(reference) = obj.get("value").and_then(extract_string_value) {
                if let Some(parsed) = parse_reference(&reference, display.as_deref()) {
                    values.push(parsed);
                }
            }
        }
        Value::String(reference) => {
            if let Some(parsed) = parse_reference(reference, None) {
                values.push(parsed);
            }
        }
        _ => {}
    }
}

fn looks_like_absolute_url(s: &str) -> bool {
    s.contains("://")
}

/// Parse a FHIR reference string into indexable fields.
///
/// Supports:
/// - Relative: `id`, `type/id`, `type/id/_history/version`
/// - Absolute: `http(s)://.../type/id`, `http(s)://.../type/id/_history/version`
/// - Canonical: `url`, `url|version`
fn parse_reference(reference: &str, display: Option<&str>) -> Option<ReferenceValue> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }

    // Canonical `url|version` (keep both parts).
    if let Some((base, version)) = reference.split_once('|') {
        let base = base.trim_end_matches('/').trim();
        if !base.is_empty() && (looks_like_absolute_url(base) || base.starts_with("urn:")) {
            return Some(ReferenceValue {
                reference_kind: ReferenceKind::Canonical,
                target_type: String::new(),
                target_id: base.to_string(),
                target_version_id: String::new(),
                target_url: String::new(),
                canonical_url: base.to_string(),
                canonical_version: version.trim().to_string(),
                display: display.map(|s| s.to_string()),
            });
        }
        // Not a canonical URL - fall through (treat `|` as part of value, or ignore later).
    }

    if let Some(reference) = reference.strip_prefix('#') {
        return Some(ReferenceValue {
            reference_kind: ReferenceKind::Fragment,
            target_type: String::new(),
            target_id: reference.to_string(),
            target_version_id: String::new(),
            target_url: String::new(),
            canonical_url: String::new(),
            canonical_version: String::new(),
            display: display.map(|s| s.to_string()),
        });
    }

    let reference = reference.trim_end_matches('/');
    let parts: Vec<&str> = reference
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if parts.is_empty() {
        return None;
    }

    // Absolute URLs: keep the URL, but also extract type/id/version from the tail segments.
    let is_absolute = looks_like_absolute_url(reference) || reference.starts_with("urn:");
    let reference_kind = if is_absolute {
        ReferenceKind::Absolute
    } else {
        ReferenceKind::Relative
    };

    let mut target_type: Option<&str> = None;
    let mut target_id: Option<&str> = None;
    let mut target_version_id: Option<&str> = None;

    if parts.len() >= 2 {
        if parts.len() >= 4 && parts[parts.len() - 2] == "_history" {
            target_type = Some(parts[parts.len() - 4]);
            target_id = Some(parts[parts.len() - 3]);
            target_version_id = Some(parts[parts.len() - 1]);
        } else {
            target_type = Some(parts[parts.len() - 2]);
            target_id = Some(parts[parts.len() - 1]);
        }
    } else if parts.len() == 1 {
        target_id = Some(parts[0]);
    }

    let target_id = target_id?;
    Some(ReferenceValue {
        reference_kind,
        target_type: target_type.unwrap_or("").to_string(),
        target_id: target_id.to_string(),
        target_version_id: target_version_id.unwrap_or("").to_string(),
        target_url: if matches!(reference_kind, ReferenceKind::Absolute) {
            reference.to_string()
        } else {
            String::new()
        },
        canonical_url: String::new(),
        canonical_version: String::new(),
        display: display.map(|s| s.to_string()),
    })
}

pub(super) fn extract_reference_identifier_tokens(value: &Value) -> Vec<TokenValue> {
    let mut tokens = Vec::new();
    extract_reference_identifier_tokens_into(value, &mut tokens);
    tokens
}

fn extract_reference_identifier_tokens_into(value: &Value, out: &mut Vec<TokenValue>) {
    match value {
        Value::Array(items) => {
            for item in items {
                extract_reference_identifier_tokens_into(item, out);
            }
        }
        Value::Object(obj) => {
            if let Some(identifier) = obj.get("identifier") {
                out.extend(extract_tokens(identifier));
            }
        }
        _ => {}
    }
}

// ============================================================================
// Date Range Extraction
// ============================================================================
//
// Date search implementation per FHIR spec section 3.2.1.5.9
//
// Key requirements:
// 1. Dates are converted to "periods" (start_date, end_date) based on precision
// 2. Date-only values (no timezone) are treated as UTC calendar dates
// 3. DateTime values with timezone are converted to UTC
// 4. DateTime values without timezone use server's local timezone
// 5. Ranges are stored as [start, end) half-open intervals for efficient comparison
// 6. Period types respect explicit start/end with missing boundaries treated as open-ended
// 7. Timing types use the lowest and highest dateTime from events
//
// Precision handling examples:
// - "2013" → [2013-01-01T00:00:00Z, 2014-01-01T00:00:00Z)
// - "2013-04" → [2013-04-01T00:00:00Z, 2013-05-01T00:00:00Z)
// - "2013-04-04" → [2013-04-04T00:00:00Z, 2013-04-05T00:00:00Z)
// - "2013-04-04T10:30" → [2013-04-04T10:30:00Z, 2013-04-04T10:31:00Z)
// - "2013-04-04T10:30:45" → [2013-04-04T10:30:45Z, 2013-04-04T10:30:46Z)
// - "2013-04-04T10:30:45.123" → [2013-04-04T10:30:45.123Z, 2013-04-04T10:30:45.124Z)
//
// ============================================================================

/// Extract date ranges from FHIR date/dateTime values
pub(crate) fn extract_date_ranges(value: &Value) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut values = Vec::new();
    extract_date_ranges_into(value, &mut values);
    values
}

/// Extract date ranges from FHIR value based on type
///
/// Per FHIR spec (3.2.1.5.9), date parameters can represent:
/// - date: Both start and end set to the date value (expanded by precision)
/// - dateTime: Both start and end set to the dateTime value (expanded by precision)
/// - instant: Both start and end set to the instant value
/// - Period: Explicit start/end, with missing boundaries treated as open-ended
/// - Timing: Start = lowest dateTime, End = highest dateTime from events
fn extract_date_ranges_into(value: &Value, values: &mut Vec<(DateTime<Utc>, DateTime<Utc>)>) {
    match value {
        Value::Array(items) => {
            for item in items {
                extract_date_ranges_into(item, values);
            }
        }
        Value::String(value) => {
            // Direct date/dateTime/instant string value
            if let Some(range) = parse_date_range(value) {
                values.push(range);
            }
        }
        Value::Object(obj) => {
            // Period type: { "start": "...", "end": "..." }
            // Per spec: "A missing lower boundary is 'less than' any actual date.
            // A missing upper boundary is 'greater than' any actual date."
            if obj.contains_key("start") || obj.contains_key("end") {
                let start = obj
                    .get("start")
                    .and_then(|value| extract_date_ranges(value).into_iter().next())
                    .map(|(start, _)| start)
                    .unwrap_or_else(min_datetime);
                let end = obj
                    .get("end")
                    .and_then(|value| extract_date_ranges(value).into_iter().next())
                    .map(|(_, end)| end)
                    .unwrap_or_else(max_datetime);
                values.push((start, end));
                return;
            }

            // Timing type: { "event": [...] }
            // Per spec: "The period has a start of the lowest dateTime within the timing
            // and an end of the highest dateTime within the timing"
            if let Some(event) = obj.get("event") {
                let mut event_ranges = Vec::new();
                extract_date_ranges_into(event, &mut event_ranges);
                if let Some((start, end)) = collapse_date_ranges(&event_ranges) {
                    values.push((start, end));
                    return;
                }
            }

            // Timing.repeat.boundsPeriod: extract from the bounds
            if let Some(repeat) = obj.get("repeat") {
                if let Some(bounds) = repeat.get("boundsPeriod") {
                    extract_date_ranges_into(bounds, values);
                    return;
                }
            }

            // Try common value fields (valueDateTime, valueDate, valueInstant)
            for key in ["value", "valueDateTime", "valueDate", "valueInstant"] {
                if let Some(value) = obj.get(key) {
                    extract_date_ranges_into(value, values);
                    return;
                }
            }
        }
        _ => {}
    }
}

/// Collapse multiple date ranges into a single range spanning all
fn collapse_date_ranges(
    ranges: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let mut iter = ranges.iter();
    let (mut min_start, mut max_end) = iter.next().cloned()?;
    for (start, end) in iter {
        if *start < min_start {
            min_start = *start;
        }
        if *end > max_end {
            max_end = *end;
        }
    }
    Some((min_start, max_end))
}

/// Parse a FHIR date/dateTime string into a UTC range
///
/// Per FHIR spec:
/// - Date-only values (no time component) are treated as calendar dates in UTC
///   without timezone conversion
/// - DateTime values with timezone are converted to UTC
/// - DateTime values without timezone use server's local timezone
/// - The range represents [start, end) as a half-open interval for efficient
///   comparison (end is exclusive upper bound)
fn parse_date_range(value: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some((date_part, time_part)) = value.split_once('T') {
        // DateTime or instant value - has time component
        let (date_start, _) = parse_date_part(date_part)?;
        let (time_part, offset_seconds) = split_time_and_offset(time_part);
        let (time, increment) = parse_time_part(&time_part)?;
        let naive = NaiveDateTime::new(date_start, time);
        let start = match offset_seconds {
            Some(offset) => offset_datetime_to_utc(naive, offset)?,
            None => local_datetime_to_utc(naive)?,
        };
        let end = start + increment;
        Some((start, end))
    } else {
        // Date-only value - treat as UTC calendar date without timezone conversion
        // Per spec: "Dates do not have timezones, and timezones SHOULD NOT be considered"
        let (start_date, end_date) = parse_date_part(value)?;
        let start_naive = start_date.and_hms_opt(0, 0, 0)?;
        let end_naive = end_date.and_hms_opt(0, 0, 0)?;
        // Use UTC directly for date-only values (no timezone conversion)
        let start = Utc.from_utc_datetime(&start_naive);
        let end = Utc.from_utc_datetime(&end_naive);
        Some((start, end))
    }
}

/// Parse date component into a range based on precision
///
/// Per FHIR spec, dates are expanded to periods based on their precision:
/// - "2013" → [2013-01-01, 2014-01-01) - represents all of year 2013
/// - "2013-04" → [2013-04-01, 2013-05-01) - represents all of April 2013
/// - "2013-04-04" → [2013-04-04, 2013-04-05) - represents all of April 4, 2013
///
/// The end date is the first instant of the next period (exclusive upper bound)
/// which is mathematically equivalent to the last instant of the current period
/// for range-based searching.
fn parse_date_part(value: &str) -> Option<(NaiveDate, NaiveDate)> {
    let value = value.trim();

    // Year only: "2013"
    // Spec: "2000 is equivalent to an interval that starts at the first instant
    // of January 1st to the last instant of December 31st"
    if value.len() == 4 && value.chars().all(|c| c.is_ascii_digit()) {
        let year: i32 = value.parse().ok()?;
        let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
        let end = NaiveDate::from_ymd_opt(year + 1, 1, 1)?;
        return Some((start, end));
    }

    // Year-Month or Year-Month-Day
    if value.len() >= 7 && value.as_bytes().get(4) == Some(&b'-') {
        let year: i32 = value[0..4].parse().ok()?;
        let month: u32 = value[5..7].parse().ok()?;
        let start = NaiveDate::from_ymd_opt(year, month, 1)?;

        // Year-Month only: "2013-04"
        // Spec: "2000-04 is equivalent to an interval that starts at the first instant
        // of the first day of the month and ends on the last instant of the last day of the month"
        if value.len() == 7 {
            let (next_year, next_month) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            let end = NaiveDate::from_ymd_opt(next_year, next_month, 1)?;
            return Some((start, end));
        }

        // Year-Month-Day: "2013-04-04"
        // Spec: "2000-04-04 is equivalent to an interval that starts at the first instant
        // of day and ends on the last instant of the day"
        if value.len() >= 10 && value.as_bytes().get(7) == Some(&b'-') {
            let day: u32 = value[8..10].parse().ok()?;
            let start = NaiveDate::from_ymd_opt(year, month, day)?;
            let end = start.checked_add_signed(Duration::days(1))?;
            return Some((start, end));
        }
    }

    // Fallback: try standard date parsing
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| {
            date.checked_add_signed(Duration::days(1))
                .map(|end| (date, end))
        })
}

/// Parse time component into time value and precision increment
///
/// Returns (time, increment) where increment represents the precision:
/// - "10" (hour only) → (10:00:00, 1 hour)
/// - "10:30" (hour:minute) → (10:30:00, 1 minute)
/// - "10:30:45" (hour:minute:second) → (10:30:45, 1 second)
/// - "10:30:45.123" (with fractional seconds) → (10:30:45.123, precision based on digits)
///
/// The increment is used to calculate the exclusive upper bound of the time range.
/// Per FHIR spec: "minutes SHALL be present if an hour is present"
fn parse_time_part(value: &str) -> Option<(NaiveTime, Duration)> {
    let mut parts = value.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute = parts.next();

    // Hour only (spec says minutes SHALL be present if hour is present,
    // but we handle it for robustness)
    if minute.is_none() {
        let time = NaiveTime::from_hms_opt(hour, 0, 0)?;
        return Some((time, Duration::hours(1)));
    }

    let minute: u32 = minute?.parse().ok()?;
    let second = parts.next();

    // Hour:Minute only (most common for dateTime without seconds)
    if second.is_none() {
        let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
        return Some((time, Duration::minutes(1)));
    }

    // Hour:Minute:Second[.fraction]
    let second = second?;
    let (second, fraction) = second.split_once('.').unwrap_or((second, ""));
    let second: u32 = second.parse().ok()?;

    // Handle fractional seconds (up to nanosecond precision)
    let (nanos, increment) = if fraction.is_empty() {
        (0, Duration::seconds(1))
    } else {
        let digits = fraction.len().min(9);
        let fraction = &fraction[..digits];
        let parsed: u32 = fraction.parse().ok()?;
        let nanos = parsed * 10_u32.pow(9 - digits as u32);
        // Increment is the smallest unit based on precision
        // e.g., .123 (3 digits) → increment of 1 millisecond
        let increment = Duration::nanoseconds(10_i64.pow(9 - digits as u32));
        (nanos, increment)
    };

    let time = NaiveTime::from_hms_nano_opt(hour, minute, second, nanos)?;
    Some((time, increment))
}

fn split_time_and_offset(value: &str) -> (String, Option<i32>) {
    let value = value.trim();
    if let Some(time) = value.strip_suffix('Z') {
        return (time.to_string(), Some(0));
    }

    if let Some(pos) = value.rfind(|c| ['+', '-'].contains(&c)) {
        if pos > 0 {
            let (time_part, offset_part) = value.split_at(pos);
            if let Some(offset) = parse_offset_seconds(offset_part) {
                return (time_part.to_string(), Some(offset));
            }
        }
    }

    (value.to_string(), None)
}

fn parse_offset_seconds(value: &str) -> Option<i32> {
    let mut chars = value.chars();
    let sign = match chars.next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (hours, minutes) = if let Some((hours, minutes)) = rest.split_once(':') {
        (hours, minutes)
    } else if rest.len() == 2 {
        (rest.as_str(), "0")
    } else if rest.len() == 4 {
        (&rest[0..2], &rest[2..4])
    } else {
        return None;
    };
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

fn local_datetime_to_utc(value: NaiveDateTime) -> Option<DateTime<Utc>> {
    match Local.from_local_datetime(&value) {
        LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        LocalResult::None => None,
    }
}

fn offset_datetime_to_utc(value: NaiveDateTime, offset_seconds: i32) -> Option<DateTime<Utc>> {
    let offset = FixedOffset::east_opt(offset_seconds)?;
    let dt = offset.from_local_datetime(&value).single()?;
    Some(dt.with_timezone(&Utc))
}

/// Minimum datetime value for open-ended ranges
fn min_datetime() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap())
}

/// Maximum datetime value for open-ended ranges
fn max_datetime() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(9999, 12, 31, 23, 59, 59)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap())
}

// ============================================================================
// Helper Functions (private)
// ============================================================================

fn extract_string_field(obj: &serde_json::Map<String, Value>, field: &str) -> Option<String> {
    obj.get(field).and_then(extract_string_value)
}

fn extract_string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(if *value { "true" } else { "false" }.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(values) => values.iter().find_map(extract_string_value),
        Value::Object(obj) => obj.get("value").and_then(extract_string_value),
        _ => None,
    }
}

fn is_contact_point_system(system: &str) -> bool {
    matches!(
        system,
        "phone" | "fax" | "email" | "pager" | "url" | "sms" | "other"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reference_indexes_relative_absolute_canonical_and_version() {
        let r = parse_reference("Patient/123", None).unwrap();
        assert_eq!(r.reference_kind, ReferenceKind::Relative);
        assert_eq!(r.target_type, "Patient");
        assert_eq!(r.target_id, "123");
        assert_eq!(r.target_version_id, "");
        assert_eq!(r.target_url, "");
        assert_eq!(r.canonical_url, "");
        assert_eq!(r.canonical_version, "");

        let r = parse_reference("Patient/123/_history/1", None).unwrap();
        assert_eq!(r.reference_kind, ReferenceKind::Relative);
        assert_eq!(r.target_type, "Patient");
        assert_eq!(r.target_id, "123");
        assert_eq!(r.target_version_id, "1");

        let r = parse_reference("http://example.org/fhir/Patient/123", None).unwrap();
        assert_eq!(r.reference_kind, ReferenceKind::Absolute);
        assert_eq!(r.target_type, "Patient");
        assert_eq!(r.target_id, "123");
        assert_eq!(r.target_url, "http://example.org/fhir/Patient/123");

        let r = parse_reference("http://example.org/canon|1.2.3", None).unwrap();
        assert_eq!(r.reference_kind, ReferenceKind::Canonical);
        assert_eq!(r.target_type, "");
        assert_eq!(r.target_id, "http://example.org/canon");
        assert_eq!(r.canonical_url, "http://example.org/canon");
        assert_eq!(r.canonical_version, "1.2.3");

        let r = parse_reference("#contained", Some("Display")).unwrap();
        assert_eq!(r.reference_kind, ReferenceKind::Fragment);
        assert_eq!(r.target_id, "contained");
        assert_eq!(r.display.as_deref(), Some("Display"));
    }
}
