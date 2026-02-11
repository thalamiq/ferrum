//! Type conversion functions for FHIRPath.
//!
//! This module implements conversion functions like `iif()`, `toBoolean()`, `toInteger()`,
//! `toDecimal()`, `toDate()`, `toDateTime()`, `toTime()`, `toQuantity()`, etc.

use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;

use regex::Regex;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::error::{Error, Result};
use crate::value::{Collection, Value, ValueData};

use super::temporal::{
    is_valid_date_string, is_valid_datetime_string, is_valid_time_string, parse_partial_datetime,
};

fn normalize_quantity_calendar_keyword(unit: &str) -> Option<&'static str> {
    match unit.trim().to_ascii_lowercase().as_str() {
        "year" | "years" => Some("year"),
        "month" | "months" => Some("month"),
        "week" | "weeks" => Some("week"),
        "day" | "days" => Some("day"),
        "hour" | "hours" => Some("hour"),
        "minute" | "minutes" => Some("minute"),
        "second" | "seconds" => Some("second"),
        "millisecond" | "milliseconds" => Some("millisecond"),
        _ => None,
    }
}

fn normalize_quantity_quoted_ucum_unit(unit: &str) -> Option<&str> {
    let u = unit.trim();
    if u.is_empty() {
        return None;
    }
    // HL7 suite expects UCUM month/year units to be rejected for toQuantity().
    if u.eq_ignore_ascii_case("mo") || u.eq_ignore_ascii_case("a") {
        return None;
    }
    Some(u)
}

pub fn iif(
    _collection: Collection,
    arg1: Option<&Collection>,
    arg2: Option<&Collection>,
    arg3: Option<&Collection>,
) -> Result<Collection> {
    // iif(criterion, true-result, otherwise-result?)
    // - criterion: boolean or empty collection
    // - true-result: returned if criterion is true
    // - otherwise-result: optional, returned if criterion is false/empty (default: empty collection)
    //
    // Note: collection parameter is the context ($this) but not used by iif directly

    let criterion =
        arg1.ok_or_else(|| Error::InvalidOperation("iif() requires criterion argument".into()))?;
    let true_result =
        arg2.ok_or_else(|| Error::InvalidOperation("iif() requires true-result argument".into()))?;
    let otherwise_result = arg3;

    // Criterion must be empty or singleton
    if !criterion.is_empty() && criterion.len() > 1 {
        return Err(Error::TypeError(
            "iif() criterion must be empty or singleton".into(),
        ));
    }

    // Evaluate criterion as boolean
    let criterion_bool = if criterion.is_empty() {
        None
    } else {
        // Must be a boolean value
        match criterion.as_boolean() {
            Ok(b) => Some(b),
            Err(_) => {
                return Err(Error::TypeError("iif() criterion must be a boolean".into()));
            }
        }
    };

    // Apply conditional logic
    if criterion_bool == Some(true) {
        Ok(true_result.clone())
    } else {
        // False or empty - return otherwise-result or empty
        Ok(otherwise_result.cloned().unwrap_or_else(Collection::empty))
    }
}

pub fn to_boolean(collection: Collection) -> Result<Collection> {
    // Empty collection returns empty
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Multiple items is an error
    if collection.len() > 1 {
        return Err(Error::TypeError(format!(
            "toBoolean() requires singleton or empty collection, got collection with {} items",
            collection.len()
        )));
    }

    let item = collection.iter().next().unwrap();

    match item.data() {
        ValueData::Boolean(b) => {
            // Boolean: identity operation
            Ok(Collection::singleton(Value::boolean(*b)))
        }
        ValueData::Integer(i) => match *i {
            0 => Ok(Collection::singleton(Value::boolean(false))),
            1 => Ok(Collection::singleton(Value::boolean(true))),
            _ => Ok(Collection::empty()),
        },
        ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            if *d == Decimal::ZERO {
                Ok(Collection::singleton(Value::boolean(false)))
            } else if *d == Decimal::ONE {
                Ok(Collection::singleton(Value::boolean(true)))
            } else {
                Ok(Collection::empty())
            }
        }
        ValueData::String(s) => {
            let s_lower = s.to_lowercase().trim().to_string();
            match s_lower.as_str() {
                "true" | "yes" => Ok(Collection::singleton(Value::boolean(true))),
                "false" | "no" => Ok(Collection::singleton(Value::boolean(false))),
                _ => Ok(Collection::empty()),
            }
        }
        _ => {
            // Other types cannot be converted to boolean
            Ok(Collection::empty())
        }
    }
}

pub fn converts_to_boolean(collection: Collection) -> Result<Collection> {
    // Empty collection returns empty
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Multiple items is an error
    if collection.len() > 1 {
        return Err(Error::TypeError(format!(
            "convertsToBoolean() requires singleton or empty collection, got collection with {} items",
            collection.len()
        )));
    }

    let item = collection.iter().next().unwrap();

    let can_convert = match item.data() {
        ValueData::Boolean(_) => true,               // Boolean always converts
        ValueData::Integer(i) => *i == 0 || *i == 1, // Only 0/1 are convertible
        ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            *d == Decimal::ZERO || *d == Decimal::ONE
        }
        ValueData::String(s) => {
            let s_lower = s.to_lowercase().trim().to_string();
            matches!(s_lower.as_str(), "true" | "false")
        }
        _ => false,
    };

    Ok(Collection::singleton(Value::boolean(can_convert)))
}

pub fn to_integer(collection: Collection) -> Result<Collection> {
    // Empty collection returns empty
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Multiple items is an error
    if collection.len() > 1 {
        return Err(Error::TypeError(format!(
            "toInteger() requires singleton or empty collection, got collection with {} items",
            collection.len()
        )));
    }

    let item = collection.iter().next().unwrap();

    match item.data() {
        ValueData::Integer(i) => {
            // Integer: identity operation
            Ok(Collection::singleton(Value::integer(*i)))
        }
        ValueData::Decimal(d) => {
            // Per FHIRPath spec: toInteger() only succeeds if there is no fractional part.
            if d.fract() != Decimal::ZERO {
                return Ok(Collection::empty());
            }
            if let Some(int_val) = d.to_i64() {
                Ok(Collection::singleton(Value::integer(int_val)))
            } else {
                Ok(Collection::empty())
            }
        }
        ValueData::String(s) => {
            let s = s.trim();
            // Only accept integer lexical form (no decimal point).
            let bytes = s.as_bytes();
            let mut idx = 0;
            if bytes.first() == Some(&b'-') {
                idx = 1;
            }
            if idx >= bytes.len() || !bytes[idx..].iter().all(|b| b.is_ascii_digit()) {
                return Ok(Collection::empty());
            }
            Ok(s.parse::<i64>()
                .map(|int_val| Collection::singleton(Value::integer(int_val)))
                .unwrap_or_else(|_| Collection::empty()))
        }
        ValueData::Boolean(b) => Ok(Collection::singleton(Value::integer(if *b {
            1
        } else {
            0
        }))),
        _ => {
            // Other types cannot be converted to integer
            Ok(Collection::empty())
        }
    }
}

pub fn converts_to_integer(collection: Collection) -> Result<Collection> {
    // Empty collection returns empty
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Multiple items is an error
    if collection.len() > 1 {
        return Err(Error::TypeError(format!(
            "convertsToInteger() requires singleton or empty collection, got collection with {} items",
            collection.len()
        )));
    }

    let item = collection.iter().next().unwrap();

    let can_convert = match item.data() {
        ValueData::Integer(_) => true, // Integer always converts
        ValueData::Decimal(d) => d.fract() == Decimal::ZERO,
        ValueData::String(s) => {
            let s = s.trim();
            let bytes = s.as_bytes();
            let mut idx = 0;
            if bytes.first() == Some(&b'-') {
                idx = 1;
            }
            idx < bytes.len()
                && bytes[idx..].iter().all(|b| b.is_ascii_digit())
                && s.parse::<i64>().is_ok()
        }
        ValueData::Boolean(_) => true,
        _ => false,
    };

    Ok(Collection::singleton(Value::boolean(can_convert)))
}

pub fn to_decimal(collection: Collection) -> Result<Collection> {
    // Empty collection returns empty
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Multiple items is an error
    if collection.len() > 1 {
        return Err(Error::TypeError(format!(
            "toDecimal() requires singleton or empty collection, got collection with {} items",
            collection.len()
        )));
    }

    let item = collection.iter().next().unwrap();

    match item.data() {
        ValueData::Decimal(d) => {
            // Decimal: identity operation
            Ok(Collection::singleton(Value::decimal(*d)))
        }
        ValueData::Integer(i) => {
            // Integer: convert to decimal
            use rust_decimal::Decimal;
            Ok(Collection::singleton(Value::decimal(Decimal::from(*i))))
        }
        ValueData::String(s) => {
            // String: parse as decimal
            match Decimal::from_str(s.as_ref()) {
                Ok(dec_val) => Ok(Collection::singleton(Value::decimal(dec_val))),
                Err(_) => Ok(Collection::empty()),
            }
        }
        ValueData::Boolean(b) => {
            use rust_decimal::Decimal;
            Ok(Collection::singleton(Value::decimal(if *b {
                Decimal::ONE
            } else {
                Decimal::ZERO
            })))
        }
        _ => {
            // Other types cannot be converted to decimal
            Ok(Collection::empty())
        }
    }
}

pub fn converts_to_decimal(collection: Collection) -> Result<Collection> {
    // Empty collection returns empty
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Multiple items is an error
    if collection.len() > 1 {
        return Err(Error::TypeError(format!(
            "convertsToDecimal() requires singleton or empty collection, got collection with {} items",
            collection.len()
        )));
    }

    let item = collection.iter().next().unwrap();

    let can_convert = match item.data() {
        ValueData::Decimal(_) => true, // Decimal always converts
        ValueData::Integer(_) => true, // Integer can be converted to decimal
        ValueData::String(s) => {
            // Check if string can be parsed as decimal
            Decimal::from_str(s.as_ref()).is_ok()
        }
        ValueData::Boolean(_) => true,
        _ => false,
    };

    Ok(Collection::singleton(Value::boolean(can_convert)))
}

pub fn converts_to_string(collection: Collection) -> Result<Collection> {
    // convertsToString() returns true if all items can be converted to string
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Boolean(_)
            | ValueData::Integer(_)
            | ValueData::Decimal(_)
            | ValueData::String(_)
            | ValueData::Date { .. }
            | ValueData::DateTime {
                value: _,
                precision: _,
                timezone_offset: _,
            }
            | ValueData::Time {
                value: _,
                precision: _,
            } => {
                // These can all be converted to string
                continue;
            }
            ValueData::Quantity { .. } => {
                // Quantities can be converted to string
                continue;
            }
            ValueData::Object(_) => {
                // Objects cannot be converted to string
                return Ok(Collection::singleton(Value::boolean(false)));
            }
            ValueData::LazyJson { .. } => {
                // Materialize lazy JSON first (will become Object)
                let materialized = item.materialize();
                match materialized.data() {
                    ValueData::Object(_) => {
                        return Ok(Collection::singleton(Value::boolean(false)));
                    }
                    _ => {
                        // After materialization, check if it can be converted
                        // Recursively check the materialized value
                        return converts_to_string(Collection::singleton(materialized));
                    }
                }
            }
            ValueData::Empty => {
                // Empty cannot be converted
                return Ok(Collection::singleton(Value::boolean(false)));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn to_date(collection: Collection) -> Result<Collection> {
    // toDate() converts strings and DateTimes to Date
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let mut result = Collection::empty();

    for item in collection.iter() {
        match item.data() {
            ValueData::Date { .. } => {
                // Already a date
                result.push(item.clone());
            }
            ValueData::String(s) => {
                // Try to parse string as date
                let s = s.as_ref();
                if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y-%m"))
                    .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y"))
                {
                    let precision = match s.len() {
                        4 => crate::value::DatePrecision::Year,
                        7 => crate::value::DatePrecision::Month,
                        _ => crate::value::DatePrecision::Day,
                    };
                    result.push(Value::date_with_precision(date, precision));
                }
                // If parsing fails, don't add anything (empty result for that item)
            }
            ValueData::DateTime {
                value: dt,
                precision,
                timezone_offset: _,
            } => {
                // Extract date from datetime
                let date_precision = match precision {
                    crate::value::DateTimePrecision::Year => crate::value::DatePrecision::Year,
                    crate::value::DateTimePrecision::Month => crate::value::DatePrecision::Month,
                    _ => crate::value::DatePrecision::Day,
                };
                result.push(Value::date_with_precision(dt.date_naive(), date_precision));
            }
            _ => {
                // Other types cannot be converted to date
                // Don't add anything (empty result for that item)
            }
        }
    }

    Ok(result)
}

pub fn converts_to_date(collection: Collection) -> Result<Collection> {
    // convertsToDate() returns true if all items can be converted to Date
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Date { .. }
            | ValueData::DateTime {
                value: _,
                precision: _,
                timezone_offset: _,
            } => {
                // These can be converted to date
                continue;
            }
            ValueData::String(s) => {
                // Check if string is a valid date (YYYY, YYYY-MM, or YYYY-MM-DD)
                if is_valid_date_string(s.as_ref()) {
                    continue;
                }
                return Ok(Collection::singleton(Value::boolean(false)));
            }
            _ => {
                // Other types cannot be converted to date
                return Ok(Collection::singleton(Value::boolean(false)));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn to_datetime(collection: Collection) -> Result<Collection> {
    // toDateTime() converts strings and Dates to DateTime
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let mut result = Collection::empty();

    for item in collection.iter() {
        match item.data() {
            ValueData::DateTime {
                value: _,
                precision: _,
                timezone_offset: _,
            } => {
                // Already a datetime
                result.push(item.clone());
            }
            ValueData::String(s) => {
                // Try to parse string as datetime
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s.as_ref()) {
                    // Preserve timezone offset from parsed datetime
                    let offset_seconds = dt.offset().local_minus_utc();
                    let dt_utc = dt.with_timezone(&chrono::Utc);
                    result.push(Value::datetime_with_precision_and_offset(
                        dt_utc,
                        crate::value::DateTimePrecision::Second,
                        Some(offset_seconds),
                    ));
                } else if let Some(dt) = parse_partial_datetime(s.as_ref()) {
                    result.push(Value::datetime(dt));
                }
                // If parsing fails, don't add anything
            }
            ValueData::Date {
                value: d,
                precision,
            } => {
                // Convert date to datetime (midnight, no timezone)
                let dt_naive = d.and_hms_opt(0, 0, 0).unwrap();
                let dt_utc = chrono::DateTime::from_naive_utc_and_offset(dt_naive, chrono::Utc);
                let dt_precision = match precision {
                    crate::value::DatePrecision::Year => crate::value::DateTimePrecision::Year,
                    crate::value::DatePrecision::Month => crate::value::DateTimePrecision::Month,
                    crate::value::DatePrecision::Day => crate::value::DateTimePrecision::Day,
                };
                result.push(Value::datetime_with_precision_and_offset(
                    dt_utc,
                    dt_precision,
                    None,
                ));
            }
            _ => {
                // Other types cannot be converted to datetime
            }
        }
    }

    Ok(result)
}

pub fn converts_to_datetime(collection: Collection) -> Result<Collection> {
    // convertsToDateTime() returns true if all items can be converted to DateTime
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::DateTime {
                value: _,
                precision: _,
                timezone_offset: _,
            }
            | ValueData::Date { .. } => {
                // These can be converted to datetime
                continue;
            }
            ValueData::String(s) => {
                // Check if string is a valid datetime (including partial dates and datetimes)
                if is_valid_datetime_string(s.as_ref()) {
                    continue;
                }
                return Ok(Collection::singleton(Value::boolean(false)));
            }
            _ => {
                // Other types cannot be converted to datetime
                return Ok(Collection::singleton(Value::boolean(false)));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn to_time(collection: Collection) -> Result<Collection> {
    // toTime() converts strings and DateTimes to Time
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let mut result = Collection::empty();

    for item in collection.iter() {
        match item.data() {
            ValueData::Time {
                value: _,
                precision: _,
            } => {
                // Already a time
                result.push(item.clone());
            }
            ValueData::String(s) => {
                // Try to parse string as time
                if let Ok(time) = chrono::NaiveTime::parse_from_str(s.as_ref(), "%H:%M:%S%.f")
                    .or_else(|_| chrono::NaiveTime::parse_from_str(s.as_ref(), "%H:%M:%S"))
                    .or_else(|_| chrono::NaiveTime::parse_from_str(s.as_ref(), "%H:%M"))
                    .or_else(|_| chrono::NaiveTime::parse_from_str(s.as_ref(), "%H"))
                {
                    result.push(Value::time(time));
                }
            }
            ValueData::DateTime {
                value: dt,
                precision: _,
                timezone_offset: _,
            } => {
                // Extract time from datetime
                result.push(Value::time(dt.time()));
            }
            _ => {
                // Other types cannot be converted to time
            }
        }
    }

    Ok(result)
}

pub fn converts_to_time(collection: Collection) -> Result<Collection> {
    // convertsToTime() returns true if all items can be converted to Time
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Time {
                value: _,
                precision: _,
            }
            | ValueData::DateTime {
                value: _,
                precision: _,
                timezone_offset: _,
            } => {
                // These can be converted to time
                continue;
            }
            ValueData::String(s) => {
                // Check if string is a valid time (HH, HH:MM, HH:MM:SS, or HH:MM:SS.fff)
                if is_valid_time_string(s.as_ref()) {
                    continue;
                }
                return Ok(Collection::singleton(Value::boolean(false)));
            }
            _ => {
                // Other types cannot be converted to time
                return Ok(Collection::singleton(Value::boolean(false)));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn to_quantity(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() != 1 {
        return Err(Error::TypeError(
            "toQuantity() requires singleton input collection".into(),
        ));
    }

    static TO_QUANTITY_RE: OnceLock<Regex> = OnceLock::new();
    let re = TO_QUANTITY_RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?P<value>(\+|-)?\d+(?:\.\d+)?)\s*(?:'(?P<unit>[^']+)'|(?P<time>[a-zA-Z]+))?\s*$",
        )
        .expect("toQuantity regex must compile")
    });

    let item = collection.iter().next().unwrap();
    match item.data() {
        ValueData::Quantity { .. } => Ok(collection),
        ValueData::Integer(i) => Ok(Collection::singleton(Value::quantity(
            Decimal::from(*i),
            Arc::from("1"),
        ))),
        ValueData::Decimal(d) => Ok(Collection::singleton(Value::quantity(*d, Arc::from("1")))),
        ValueData::Boolean(b) => Ok(Collection::singleton(Value::quantity(
            if *b { Decimal::ONE } else { Decimal::ZERO },
            Arc::from("1"),
        ))),
        ValueData::String(s) => {
            let Some(caps) = re.captures(s.as_ref()) else {
                return Ok(Collection::empty());
            };
            let Some(value_str) = caps.name("value").map(|m| m.as_str()) else {
                return Ok(Collection::empty());
            };
            let Ok(value) = Decimal::from_str(value_str) else {
                return Ok(Collection::empty());
            };

            let unit = if let Some(unit) = caps.name("unit").map(|m| m.as_str()) {
                let Some(unit) = normalize_quantity_quoted_ucum_unit(unit) else {
                    return Ok(Collection::empty());
                };
                Arc::from(unit)
            } else if let Some(time) = caps.name("time").map(|m| m.as_str()) {
                let Some(unit) = normalize_quantity_calendar_keyword(time) else {
                    return Ok(Collection::empty());
                };
                Arc::from(unit)
            } else {
                Arc::from("1")
            };

            Ok(Collection::singleton(Value::quantity(value, unit)))
        }
        _ => Ok(Collection::empty()),
    }
}

pub fn converts_to_quantity(collection: Collection) -> Result<Collection> {
    // convertsToQuantity() returns true if all items can be converted to Quantity
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    if collection.len() != 1 {
        return Err(Error::TypeError(
            "convertsToQuantity() requires singleton input collection".into(),
        ));
    }

    static TO_QUANTITY_RE: OnceLock<Regex> = OnceLock::new();
    let re = TO_QUANTITY_RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?P<value>(\+|-)?\d+(?:\.\d+)?)\s*(?:'(?P<unit>[^']+)'|(?P<time>[a-zA-Z]+))?\s*$",
        )
        .expect("toQuantity regex must compile")
    });

    let item = collection.iter().next().unwrap();
    let ok = match item.data() {
        ValueData::Quantity { .. }
        | ValueData::Integer(_)
        | ValueData::Decimal(_)
        | ValueData::Boolean(_) => true,
        ValueData::String(s) => re.captures(s.as_ref()).is_some_and(|caps| {
            if let Some(unit) = caps.name("unit").map(|m| m.as_str()) {
                normalize_quantity_quoted_ucum_unit(unit).is_some()
            } else if let Some(time) = caps.name("time").map(|m| m.as_str()) {
                normalize_quantity_calendar_keyword(time).is_some()
            } else {
                true
            }
        }),
        _ => false,
    };

    Ok(Collection::singleton(Value::boolean(ok)))
}
