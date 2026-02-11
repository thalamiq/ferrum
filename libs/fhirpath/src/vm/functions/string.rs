//! String manipulation functions for FHIRPath.
//!
//! This module implements all string-related functions like `toString()`, `indexOf()`,
//! `substring()`, `upper()`, `lower()`, `matches()`, `replace()`, etc.

use std::sync::Arc;

#[cfg(feature = "regex")]
use regex::Regex;

#[cfg(feature = "base64")]
use base64::{engine::general_purpose, Engine};

#[cfg(feature = "html-escape")]
use html_escape;

use crate::error::{Error, Result};
use crate::value::{Collection, Value, ValueData};
use chrono::Timelike;

/// Format a timezone suffix for a fixed offset (seconds east of UTC).
fn format_timezone_suffix(offset_secs: i32) -> String {
    if offset_secs == 0 {
        return "Z".to_string();
    }
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let abs = offset_secs.abs();
    let hours = abs / 3600;
    let minutes = (abs % 3600) / 60;
    format!("{}{:02}:{:02}", sign, hours, minutes)
}

pub fn to_string(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() == 1 {
        let item = collection.iter().next().unwrap();
        let str_val: Arc<str> = match item.data() {
            ValueData::String(s) => s.clone(),
            ValueData::Integer(i) => format!("{}", i).into(),
            ValueData::Decimal(d) => format!("{}", d).into(),
            ValueData::Boolean(b) => format!("{}", b).into(),
            ValueData::Date {
                value: d,
                precision,
            } => match *precision {
                crate::value::DatePrecision::Year => d.format("%Y").to_string().into(),
                crate::value::DatePrecision::Month => d.format("%Y-%m").to_string().into(),
                crate::value::DatePrecision::Day => d.format("%Y-%m-%d").to_string().into(),
            },
            ValueData::DateTime {
                value: dt,
                precision,
                timezone_offset,
            } => {
                let (dt_str, include_tz) = match *precision {
                    crate::value::DateTimePrecision::Year => (dt.format("%Y").to_string(), false),
                    crate::value::DateTimePrecision::Month => {
                        (dt.format("%Y-%m").to_string(), false)
                    }
                    crate::value::DateTimePrecision::Day => {
                        (dt.format("%Y-%m-%d").to_string(), false)
                    }
                    crate::value::DateTimePrecision::Hour => {
                        (dt.format("%Y-%m-%dT%H").to_string(), true)
                    }
                    crate::value::DateTimePrecision::Minute => {
                        (dt.format("%Y-%m-%dT%H:%M").to_string(), true)
                    }
                    crate::value::DateTimePrecision::Second => {
                        (dt.format("%Y-%m-%dT%H:%M:%S").to_string(), true)
                    }
                    crate::value::DateTimePrecision::Millisecond => {
                        let ms = dt.timestamp_subsec_millis();
                        (
                            format!("{}.{:03}", dt.format("%Y-%m-%dT%H:%M:%S"), ms),
                            true,
                        )
                    }
                };

                if include_tz {
                    if let Some(offset_secs) = timezone_offset {
                        // Convert the instant to the original offset for display.
                        let offset = chrono::FixedOffset::east_opt(*offset_secs)
                            .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
                        let dt_local = dt.with_timezone(&offset);
                        let dt_local_str = match *precision {
                            crate::value::DateTimePrecision::Hour => {
                                dt_local.format("%Y-%m-%dT%H").to_string()
                            }
                            crate::value::DateTimePrecision::Minute => {
                                dt_local.format("%Y-%m-%dT%H:%M").to_string()
                            }
                            crate::value::DateTimePrecision::Second => {
                                dt_local.format("%Y-%m-%dT%H:%M:%S").to_string()
                            }
                            crate::value::DateTimePrecision::Millisecond => {
                                let ms = dt_local.timestamp_subsec_millis();
                                format!("{}.{:03}", dt_local.format("%Y-%m-%dT%H:%M:%S"), ms)
                            }
                            _ => dt_str,
                        };
                        format!("{}{}", dt_local_str, format_timezone_suffix(*offset_secs)).into()
                    } else {
                        // No timezone specified in the value.
                        dt_str.into()
                    }
                } else {
                    dt_str.into()
                }
            }
            ValueData::Time {
                value: t,
                precision,
            } => match *precision {
                crate::value::TimePrecision::Hour => t.format("%H").to_string().into(),
                crate::value::TimePrecision::Minute => t.format("%H:%M").to_string().into(),
                crate::value::TimePrecision::Second => t.format("%H:%M:%S").to_string().into(),
                crate::value::TimePrecision::Millisecond => {
                    let ms = t.nanosecond() / 1_000_000;
                    format!("{}.{:03}", t.format("%H:%M:%S"), ms).into()
                }
            },
            ValueData::Quantity { value, unit } => {
                let unit_str = unit.as_ref();
                if unit_str.is_empty() || unit_str == "1" {
                    format!("{} '1'", value).into()
                } else if matches!(
                    unit_str,
                    "day" | "days" | "week" | "weeks" | "month" | "months" | "year" | "years"
                ) || (unit_str.chars().all(|c| c.is_alphanumeric()) && unit_str.len() > 2)
                {
                    format!("{} {}", value, unit_str).into()
                } else {
                    format!("{} '{}'", value, unit_str).into()
                }
            }
            _ => return Err(Error::TypeError("Cannot convert to string".into())),
        };
        Ok(Collection::singleton(Value::string(str_val)))
    } else {
        Err(Error::TypeError(
            "toString() requires singleton collection".into(),
        ))
    }
}

pub fn index_of(collection: Collection, search_arg: Option<&Collection>) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let search_collection = search_arg
        .ok_or_else(|| Error::InvalidOperation("indexOf() requires 1 argument".into()))?;

    if search_collection.is_empty() {
        return Ok(Collection::empty());
    }

    let search_str = search_collection.as_string()?;

    let str_val = collection.as_string()?;
    match str_val.find(search_str.as_ref()) {
        Some(idx) => Ok(Collection::singleton(Value::integer(idx as i64))),
        None => Ok(Collection::singleton(Value::integer(-1))),
    }
}

pub fn last_index_of(
    collection: Collection,
    search_arg: Option<&Collection>,
) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let search_collection = search_arg
        .ok_or_else(|| Error::InvalidOperation("lastIndexOf() requires 1 argument".into()))?;

    if search_collection.is_empty() {
        return Ok(Collection::empty());
    }

    let search_str = search_collection.as_string()?;

    let str_val = collection.as_string()?;
    match str_val.rfind(search_str.as_ref()) {
        Some(idx) => Ok(Collection::singleton(Value::integer(idx as i64))),
        None => Ok(Collection::singleton(Value::integer(-1))),
    }
}

pub fn substring(
    collection: Collection,
    start_arg: Option<&Collection>,
    length_arg: Option<&Collection>,
) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let start_collection = start_arg.ok_or_else(|| {
        Error::InvalidOperation("substring() requires at least 1 argument".into())
    })?;

    if start_collection.is_empty() {
        return Ok(Collection::empty());
    }

    let start = start_collection.as_integer()?;

    let str_val = collection.as_string()?;
    if start < 0 {
        return Ok(Collection::empty());
    }
    let start_idx = start as usize;

    if start_idx >= str_val.len() {
        return Ok(Collection::empty());
    }

    let result = if let Some(length_arg) = length_arg {
        let length = length_arg.as_integer()?;
        if length < 0 {
            return Err(Error::InvalidOperation(
                "substring() length must be non-negative".into(),
            ));
        }
        let end_idx = (start_idx + length as usize).min(str_val.len());
        str_val[start_idx..end_idx].to_string()
    } else {
        str_val[start_idx..].to_string()
    };

    Ok(Collection::singleton(Value::string(result)))
}

pub fn starts_with(collection: Collection, prefix_arg: Option<&Collection>) -> Result<Collection> {
    // If collection is empty, return empty (per FHIRPath spec)
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Extract prefix argument - must be present
    let prefix_collection = match prefix_arg {
        Some(c) => {
            if c.is_empty() {
                // Empty prefix collection means empty argument, return empty result
                return Ok(Collection::empty());
            }
            c
        }
        None => {
            return Err(Error::InvalidOperation(
                "startsWith() requires 1 argument".into(),
            ));
        }
    };

    // Extract strings from collections
    let str_val = collection
        .as_string()
        .map_err(|e| Error::TypeError(format!("startsWith() requires string collection: {}", e)))?;
    let prefix_str = prefix_collection
        .as_string()
        .map_err(|e| Error::TypeError(format!("startsWith() requires string prefix: {}", e)))?;

    // Empty string prefix always matches (per FHIRPath spec)
    if prefix_str.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    // Perform the starts_with check
    // Note: Both str_val and prefix_str are Arc<str>, so we use as_ref() to get &str
    let result = str_val.starts_with(prefix_str.as_ref());
    Ok(Collection::singleton(Value::boolean(result)))
}

pub fn ends_with(collection: Collection, suffix_arg: Option<&Collection>) -> Result<Collection> {
    let suffix = suffix_arg
        .ok_or_else(|| Error::InvalidOperation("endsWith() requires 1 argument".into()))?
        .as_string()?;

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    Ok(Collection::singleton(Value::boolean(
        str_val.ends_with(suffix.as_ref()),
    )))
}

pub fn contains_str(collection: Collection, substr_arg: Option<&Collection>) -> Result<Collection> {
    let substr = substr_arg
        .ok_or_else(|| Error::InvalidOperation("contains() requires 1 argument".into()))?
        .as_string()?;

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    Ok(Collection::singleton(Value::boolean(
        str_val.contains(substr.as_ref()),
    )))
}

pub fn upper(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    Ok(Collection::singleton(Value::string(str_val.to_uppercase())))
}

pub fn lower(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    Ok(Collection::singleton(Value::string(str_val.to_lowercase())))
}

pub fn replace(
    collection: Collection,
    old_arg: Option<&Collection>,
    new_arg: Option<&Collection>,
) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let old_collection =
        old_arg.ok_or_else(|| Error::InvalidOperation("replace() requires 2 arguments".into()))?;
    let new_collection =
        new_arg.ok_or_else(|| Error::InvalidOperation("replace() requires 2 arguments".into()))?;

    if old_collection.is_empty() || new_collection.is_empty() {
        return Ok(Collection::empty());
    }

    let old_str = old_collection.as_string()?;
    let new_str = new_collection.as_string()?;

    let str_val = collection.as_string()?;
    let result = str_val.replace(old_str.as_ref(), new_str.as_ref());
    Ok(Collection::singleton(Value::string(result)))
}

pub fn matches(collection: Collection, pattern_arg: Option<&Collection>) -> Result<Collection> {
    // matches() returns true when the value matches the given regular expression
    // Regular expressions are case-sensitive and use 'single line' mode (DOTALL)

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let pattern = pattern_arg
        .ok_or_else(|| Error::InvalidOperation("matches() requires 1 argument".into()))?;

    if pattern.is_empty() {
        return Ok(Collection::empty());
    }

    #[cfg(feature = "regex")]
    {
        let input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("matches() requires string input".into()))?;
        let pattern_str = pattern
            .as_string()
            .map_err(|_| Error::TypeError("matches() pattern must be a string".into()))?;

        // Compile regex with DOTALL flag (single line mode) to align with FHIRPath
        let regex = Regex::new(&format!("(?s){}", pattern_str.as_ref()))
            .map_err(|e| Error::InvalidOperation(format!("Invalid regular expression: {}", e)))?;

        // Use is_match which is equivalent to re.search() in Python
        let matched = regex.is_match(input_str.as_ref());
        Ok(Collection::singleton(Value::boolean(matched)))
    }

    #[cfg(not(feature = "regex"))]
    {
        let _input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("matches() requires string input".into()))?;
        let _pattern_str = pattern
            .as_string()
            .map_err(|_| Error::TypeError("matches() pattern must be a string".into()))?;
        Err(Error::Unsupported(
            "matches() requires regex feature to be enabled".into(),
        ))
    }
}

pub fn matches_full(
    collection: Collection,
    pattern_arg: Option<&Collection>,
) -> Result<Collection> {
    // matchesFull() returns true when the value completely matches the given regular expression
    // This implicitly adds ^ and $ anchors to the pattern (fullmatch)

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let pattern = pattern_arg
        .ok_or_else(|| Error::InvalidOperation("matchesFull() requires 1 argument".into()))?;

    if pattern.is_empty() {
        return Ok(Collection::empty());
    }

    #[cfg(feature = "regex")]
    {
        let input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("matchesFull() requires string input".into()))?;
        let pattern_str = pattern
            .as_string()
            .map_err(|_| Error::TypeError("matchesFull() pattern must be a string".into()))?;

        // Compile regex with DOTALL flag (single line mode) and anchor for full match
        let anchored = format!("(?s)^(?:{})$", pattern_str.as_ref());
        let regex = Regex::new(&anchored)
            .map_err(|e| Error::InvalidOperation(format!("Invalid regular expression: {}", e)))?;

        let matched = regex.is_match(input_str.as_ref());
        Ok(Collection::singleton(Value::boolean(matched)))
    }

    #[cfg(not(feature = "regex"))]
    {
        let _input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("matchesFull() requires string input".into()))?;
        let _pattern_str = pattern
            .as_string()
            .map_err(|_| Error::TypeError("matchesFull() pattern must be a string".into()))?;
        Err(Error::Unsupported(
            "matchesFull() requires regex feature to be enabled".into(),
        ))
    }
}

pub fn replace_matches(
    collection: Collection,
    pattern_arg: Option<&Collection>,
    replacement_arg: Option<&Collection>,
) -> Result<Collection> {
    // replaceMatches() matches the input using the regular expression and replaces each match with substitution
    // The substitution may refer to identified match groups in the regular expression

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let pattern = pattern_arg
        .ok_or_else(|| Error::InvalidOperation("replaceMatches() requires 2 arguments".into()))?;
    let replacement = replacement_arg
        .ok_or_else(|| Error::InvalidOperation("replaceMatches() requires 2 arguments".into()))?;

    if pattern.is_empty() || replacement.is_empty() {
        return Ok(Collection::empty());
    }

    let input_str = collection
        .as_string()
        .map_err(|_| Error::TypeError("replaceMatches() requires string input".into()))?;
    let pattern_str = pattern
        .as_string()
        .map_err(|_| Error::TypeError("replaceMatches() pattern must be a string".into()))?;

    // Special case: if regex is empty string, return original string unchanged
    if pattern_str.is_empty() {
        return Ok(Collection::singleton(Value::string(input_str)));
    }

    #[cfg(feature = "regex")]
    {
        let replacement_str = replacement.as_string().map_err(|_| {
            Error::TypeError("replaceMatches() replacement must be a string".into())
        })?;

        // Compile regex with DOTALL flag (single line mode)
        let regex = Regex::new(pattern_str.as_ref())
            .map_err(|e| Error::InvalidOperation(format!("Invalid regular expression: {}", e)))?;

        // Use replace_all which is equivalent to re.sub() in Python
        // Note: Rust's regex crate uses $name or ${name} for named captures, which matches FHIRPath spec
        let result = regex.replace_all(input_str.as_ref(), replacement_str.as_ref());
        Ok(Collection::singleton(Value::string(result.to_string())))
    }

    #[cfg(not(feature = "regex"))]
    {
        let _replacement_str = replacement.as_string().map_err(|_| {
            Error::TypeError("replaceMatches() replacement must be a string".into())
        })?;
        Err(Error::Unsupported(
            "replaceMatches() requires regex feature to be enabled".into(),
        ))
    }
}

pub fn length(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    Ok(Collection::singleton(Value::integer(str_val.len() as i64)))
}

pub fn to_chars(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    let mut result = Collection::empty();

    for ch in str_val.chars() {
        result.push(Value::string(ch.to_string()));
    }

    Ok(result)
}

pub fn trim(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    Ok(Collection::singleton(Value::string(
        str_val.trim().to_string(),
    )))
}

pub fn encode(collection: Collection, format_arg: Option<&Collection>) -> Result<Collection> {
    // encode() encodes a singleton string in the given format
    // Available formats: hex, base64, urlbase64

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let format = format_arg
        .ok_or_else(|| Error::InvalidOperation("encode() requires 1 argument (format)".into()))?;

    if format.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "encode() requires singleton collection".into(),
        ));
    }

    #[cfg(feature = "base64")]
    {
        let input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("encode() requires string input".into()))?;
        let format_str = format
            .as_string()
            .map_err(|_| Error::TypeError("encode() format must be a string".into()))?;

        let input_bytes = input_str.as_bytes();

        let result = match format_str.as_ref() {
            "hex" => {
                // Hex encoding
                hex::encode(input_bytes)
            }
            "base64" => {
                // Base64 encoding
                general_purpose::STANDARD.encode(input_bytes)
            }
            "urlbase64" => {
                // URL-safe Base64 encoding
                general_purpose::URL_SAFE.encode(input_bytes)
            }
            _ => {
                // Unknown format
                return Ok(Collection::empty());
            }
        };

        Ok(Collection::singleton(Value::string(result)))
    }

    #[cfg(not(feature = "base64"))]
    {
        let _input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("encode() requires string input".into()))?;
        let _format_str = format
            .as_string()
            .map_err(|_| Error::TypeError("encode() format must be a string".into()))?;
        Err(Error::Unsupported(
            "encode() requires base64 feature to be enabled".into(),
        ))
    }
}

pub fn decode(collection: Collection, format_arg: Option<&Collection>) -> Result<Collection> {
    // decode() decodes a singleton encoded string according to the given format
    // Available formats: hex, base64, urlbase64

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let format = format_arg
        .ok_or_else(|| Error::InvalidOperation("decode() requires 1 argument (format)".into()))?;

    if format.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "decode() requires singleton collection".into(),
        ));
    }

    #[cfg(feature = "base64")]
    {
        let input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("decode() requires string input".into()))?;
        let format_str = format
            .as_string()
            .map_err(|_| Error::TypeError("decode() format must be a string".into()))?;

        let decoded_bytes = match format_str.as_ref() {
            "hex" => {
                // Hex decoding
                hex::decode(input_str.as_ref())
                    .map_err(|e| Error::InvalidOperation(format!("Error decoding hex: {}", e)))?
            }
            "base64" => {
                // Base64 decoding
                general_purpose::STANDARD
                    .decode(input_str.as_ref())
                    .map_err(|e| Error::InvalidOperation(format!("Error decoding base64: {}", e)))?
            }
            "urlbase64" => {
                // URL-safe Base64 decoding
                general_purpose::URL_SAFE
                    .decode(input_str.as_ref())
                    .map_err(|e| {
                        Error::InvalidOperation(format!("Error decoding urlbase64: {}", e))
                    })?
            }
            _ => {
                // Unknown format
                return Ok(Collection::empty());
            }
        };

        // Decode bytes to UTF-8 string
        let result = String::from_utf8(decoded_bytes)
            .map_err(|e| Error::InvalidOperation(format!("Error decoding UTF-8: {}", e)))?;

        Ok(Collection::singleton(Value::string(result)))
    }

    #[cfg(not(feature = "base64"))]
    {
        let _input_str = collection
            .as_string()
            .map_err(|_| Error::TypeError("decode() requires string input".into()))?;
        let _format_str = format
            .as_string()
            .map_err(|_| Error::TypeError("decode() format must be a string".into()))?;
        Err(Error::Unsupported(
            "decode() requires base64 feature to be enabled".into(),
        ))
    }
}

pub fn escape(collection: Collection, target_arg: Option<&Collection>) -> Result<Collection> {
    // escape() escapes a singleton string for a given target
    // Available targets: html, json

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let target = target_arg
        .ok_or_else(|| Error::InvalidOperation("escape() requires 1 argument (target)".into()))?;

    if target.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "escape() requires singleton collection".into(),
        ));
    }

    let input_str = collection
        .as_string()
        .map_err(|_| Error::TypeError("escape() requires string input".into()))?;
    let target_str = target
        .as_string()
        .map_err(|_| Error::TypeError("escape() target must be a string".into()))?;

    let result: String = match target_str.as_ref() {
        "html" => {
            // HTML escaping (escape quotes as &quot; plus <, >, &)
            #[cfg(feature = "html-escape")]
            {
                html_escape::encode_double_quoted_attribute(input_str.as_ref()).into_owned()
            }
            #[cfg(not(feature = "html-escape"))]
            {
                return Err(Error::InvalidOperation(
                    "html-escape feature is required for HTML escaping".into(),
                ));
            }
        }
        "json" => {
            // JSON escaping - use serde_json to escape, then remove surrounding quotes
            let escaped = serde_json::to_string(input_str.as_ref())
                .map_err(|e| Error::InvalidOperation(format!("Error escaping JSON: {}", e)))?;
            // Remove surrounding quotes
            escaped[1..escaped.len() - 1].to_string()
        }
        "xml" => {
            // XML escaping same as HTML for our purposes
            #[cfg(feature = "html-escape")]
            {
                html_escape::encode_text(input_str.as_ref()).into()
            }
            #[cfg(not(feature = "html-escape"))]
            {
                return Err(Error::InvalidOperation(
                    "html-escape feature is required for XML escaping".into(),
                ));
            }
        }
        "url" => {
            // Percent-encode; treat the whole string, leave reserved as-is per URI encoding
            urlencoding::encode(input_str.as_ref()).into_owned()
        }
        "data" => {
            // Escape for data URLs (same as URL encoding)
            urlencoding::encode(input_str.as_ref()).into_owned()
        }
        _ => {
            // Unknown target
            return Ok(Collection::empty());
        }
    };

    Ok(Collection::singleton(Value::string(result)))
}

pub fn unescape(collection: Collection, target_arg: Option<&Collection>) -> Result<Collection> {
    // unescape() unescapes a singleton string for a given target
    // Available targets: html, json

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let target = target_arg
        .ok_or_else(|| Error::InvalidOperation("unescape() requires 1 argument (target)".into()))?;

    if target.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "unescape() requires singleton collection".into(),
        ));
    }

    let input_str = collection
        .as_string()
        .map_err(|_| Error::TypeError("unescape() requires string input".into()))?;
    let target_str = target
        .as_string()
        .map_err(|_| Error::TypeError("unescape() target must be a string".into()))?;

    let result: String = match target_str.as_ref() {
        "html" => {
            // HTML unescaping
            #[cfg(feature = "html-escape")]
            {
                html_escape::decode_html_entities(input_str.as_ref()).into_owned()
            }
            #[cfg(not(feature = "html-escape"))]
            {
                return Err(Error::InvalidOperation(
                    "html-escape feature is required for HTML unescaping".into(),
                ));
            }
        }
        "json" => {
            // JSON unescaping while preserving literal quotes if they were part of the input
            let raw = input_str.as_ref();
            let has_wrapped_quotes = raw.len() >= 2 && raw.starts_with('\"') && raw.ends_with('\"');
            let decode_target = if has_wrapped_quotes {
                &raw[1..raw.len() - 1]
            } else {
                raw
            };

            let escape_for_json = |s: &str| s.replace('\\', "\\\\").replace('\"', "\\\"");
            let decoded =
                serde_json::from_str::<String>(&format!("\"{}\"", escape_for_json(decode_target)))
                    .unwrap_or_else(|_| decode_target.to_string());

            if has_wrapped_quotes {
                format!("\"{}\"", decoded)
            } else {
                decoded
            }
        }
        "xml" => {
            #[cfg(feature = "html-escape")]
            {
                html_escape::decode_html_entities(input_str.as_ref()).into_owned()
            }
            #[cfg(not(feature = "html-escape"))]
            {
                return Err(Error::InvalidOperation(
                    "html-escape feature is required for XML unescaping".into(),
                ));
            }
        }
        "url" | "data" => urlencoding::decode(input_str.as_ref())
            .map_err(|e| Error::InvalidOperation(format!("Error unescaping URL: {}", e)))?
            .into_owned(),
        _ => {
            // Unknown target
            return Ok(Collection::empty());
        }
    };

    Ok(Collection::singleton(Value::string(result)))
}

pub fn split(collection: Collection, separator_arg: Option<&Collection>) -> Result<Collection> {
    let separator = separator_arg
        .ok_or_else(|| Error::InvalidOperation("split() requires 1 argument".into()))?
        .as_string()?;

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let str_val = collection.as_string()?;
    let mut result = Collection::empty();

    for part in str_val.split(separator.as_ref()) {
        result.push(Value::string(part.to_string()));
    }

    Ok(result)
}

pub fn join(collection: Collection, separator_arg: Option<&Collection>) -> Result<Collection> {
    let separator = separator_arg
        .ok_or_else(|| Error::InvalidOperation("join() requires 1 argument".into()))?
        .as_string()?;

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let mut parts = Vec::new();
    for item in collection.iter() {
        let str_val = item
            .data()
            .as_string()
            .ok_or_else(|| Error::TypeError("join() requires string collection".into()))?;
        parts.push(str_val.to_string());
    }

    let result = parts.join(separator.as_ref());
    Ok(Collection::singleton(Value::string(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::vm::functions::type_helpers::matches_type_specifier;
    use rust_decimal::Decimal;

    fn ctx() -> Context {
        Context::new(Value::empty())
    }

    #[test]
    fn test_starts_with_direct() {
        let hello_col = Collection::singleton(Value::string("hello"));
        let he_col = Collection::singleton(Value::string("he"));

        let result = starts_with(hello_col.clone(), Some(&he_col)).unwrap();
        assert!(result.as_boolean().unwrap());

        let lo_col = Collection::singleton(Value::string("lo"));
        let result = starts_with(hello_col, Some(&lo_col)).unwrap();
        assert!(!result.as_boolean().unwrap());
    }

    #[test]
    pub fn matches_fhir_integer_with_path_hint() {
        let val = Value::integer(1);
        assert!(matches_type_specifier(
            &val,
            "FHIR.integer",
            Some("parameter.valueInteger"),
            None,
            &ctx()
        ));
    }

    #[test]
    pub fn matches_fhir_integer_without_hint() {
        let val = Value::integer(1);
        assert!(matches_type_specifier(
            &val,
            "FHIR.integer",
            None,
            None,
            &ctx()
        ));
    }

    #[test]
    pub fn matches_fhir_uuid_and_uri_with_hint() {
        let uuid_val = Value::string("urn:uuid:79a14950-442c-11ed-b878-0242ac120002");
        assert!(matches_type_specifier(
            &uuid_val,
            "FHIR.uuid",
            Some("parameter.valueUuid"),
            None,
            &ctx()
        ));

        let uri_val = Value::string("http://hl7.org/fhir/ValueSet/administrative-gender");
        assert!(matches_type_specifier(
            &uri_val,
            "FHIR.uri",
            Some("parameter.valueUri"),
            None,
            &ctx()
        ));
    }

    #[test]
    pub fn matches_fhir_decimal_with_hint() {
        let val = Value::decimal(Decimal::from(1));
        assert!(matches_type_specifier(
            &val,
            "FHIR.decimal",
            Some("parameter.valueDecimal"),
            None,
            &ctx()
        ));
    }
}
