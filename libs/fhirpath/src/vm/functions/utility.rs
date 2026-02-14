//! Utility functions for FHIRPath.
//!
//! This module implements various utility functions like `trace()`, `now()`, `today()`,
//! `sort()`, `type()`, `conformsTo()`, etc.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::TimeZone;
use rust_decimal::Decimal;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::hir::HirBinaryOperator;
use crate::resolver::ResourceResolver;
use crate::value::{Collection, Value, ValueData};
use crate::vm::operations::execute_binary_op;
use ferrum_context::FhirContext;

use super::type_helpers::{
    choose_declared_type_for_value, infer_type_descriptor, normalize_type_code, type_info_value,
    TypeDescriptor,
};

pub fn trace(
    collection: Collection,
    name_arg: Option<&Collection>,
    projection_arg: Option<&Collection>,
) -> Result<Collection> {
    // trace() is a debugging function that logs the collection and returns it unchanged
    // The name argument is used as a label for the trace
    // The projection argument (if provided) is evaluated and traced instead of the collection

    // Extract trace name
    let name = if let Some(name_arg) = name_arg {
        if name_arg.is_empty() {
            "trace".to_string()
        } else {
            name_arg
                .as_string()
                .map_err(|_| Error::TypeError("trace() name argument must be a string".into()))?
                .to_string()
        }
    } else {
        "trace".to_string()
    };

    // If projection provided, evaluate it (but we can't evaluate expressions here, so just use collection)
    // In a full implementation, this would evaluate the projection expression
    let value_to_trace = if projection_arg.is_some() {
        // Projection would be evaluated in the evaluator/engine, not here
        // For now, just trace the original collection
        &collection
    } else {
        &collection
    };

    // Log the trace (using eprintln for now - could be configurable)
    eprintln!(
        "[FHIRPath trace: {}] Collection with {} items",
        name,
        value_to_trace.len()
    );

    // Always return the original collection unchanged
    Ok(collection)
}

pub fn now() -> Result<Collection> {
    // Returns the current date and time, including timezone offset
    // To ensure deterministic evaluation, this function returns the same DateTime
    // value regardless of how many times it is evaluated within any given expression
    use chrono::{Timelike, Utc};

    // Get current UTC time
    let datetime = Utc::now()
        .with_nanosecond(0)
        .and_then(|dt| dt.with_second(dt.second()))
        .unwrap_or_else(Utc::now); // truncate to seconds to avoid spurious precision differences

    Ok(Collection::singleton(Value::datetime(datetime)))
}

pub fn today() -> Result<Collection> {
    // Returns the current date
    // To ensure deterministic evaluation, this function returns the same Date
    // value regardless of how many times it is evaluated within any given expression
    use chrono::Utc;

    let utc_now = Utc::now();
    let date = utc_now.date_naive();

    Ok(Collection::singleton(Value::date(date)))
}

pub fn time_of_day() -> Result<Collection> {
    // Returns the current time
    // To ensure deterministic evaluation, this function returns the same Time
    // value regardless of how many times it is evaluated within any given expression
    use chrono::Utc;

    let utc_now = Utc::now();
    let time = utc_now.time();

    Ok(Collection::singleton(Value::time(time)))
}

pub fn sort(collection: Collection, order_arg: Option<&Collection>) -> Result<Collection> {
    // sort() sorts collection items according to FHIRPath sort semantics
    // If no order argument provided, does natural sorting
    // Order argument can be 'asc' or 'desc' (defaults to 'asc')

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Determine sort order
    let descending = if let Some(order_arg) = order_arg {
        if order_arg.is_empty() {
            false // Default to ascending
        } else {
            let order_str = order_arg
                .as_string()
                .map_err(|_| Error::TypeError("sort() order argument must be a string".into()))?;
            order_str.to_lowercase() == "desc"
        }
    } else {
        false // Default to ascending
    };

    // Collect items into a vector for sorting
    let mut items: Vec<Value> = collection.iter().cloned().collect();

    // Sort items using natural ordering
    items.sort_by(|a, b| {
        let cmp = compare_values(a, b);
        if descending {
            cmp.reverse()
        } else {
            cmp
        }
    });

    // Build result collection
    let mut result = Collection::empty();
    for item in items {
        result.push(item);
    }

    Ok(result)
}

// Helper function to compare two values for sorting
fn compare_values(left: &Value, right: &Value) -> std::cmp::Ordering {
    use rust_decimal::Decimal;
    use std::cmp::Ordering;

    match (left.data(), right.data()) {
        // Empty values sort last
        (ValueData::Empty, ValueData::Empty) => Ordering::Equal,
        (ValueData::Empty, _) => Ordering::Greater,
        (_, ValueData::Empty) => Ordering::Less,

        // Boolean: false < true
        (ValueData::Boolean(l), ValueData::Boolean(r)) => l.cmp(r),

        // Numeric types: compare numerically (with type ordering)
        (ValueData::Integer(l), ValueData::Integer(r)) => l.cmp(r),
        (ValueData::Decimal(l), ValueData::Decimal(r)) => l.cmp(r),
        (ValueData::Integer(l), ValueData::Decimal(r)) => Decimal::from(*l).cmp(r),
        (ValueData::Decimal(l), ValueData::Integer(r)) => l.cmp(&Decimal::from(*r)),

        // String: lexicographic comparison
        (ValueData::String(l), ValueData::String(r)) => l.cmp(r),

        // Date/Time: chronological comparison
        (ValueData::Date { value: l, .. }, ValueData::Date { value: r, .. }) => l.cmp(r),
        (
            ValueData::DateTime {
                value: l,
                precision: _,
                timezone_offset: _,
            },
            ValueData::DateTime {
                value: r,
                precision: _,
                timezone_offset: _,
            },
        ) => l.cmp(r),
        (
            ValueData::Time {
                value: l,
                precision: _,
            },
            ValueData::Time {
                value: r,
                precision: _,
            },
        ) => l.cmp(r),

        // Different types: order by type name
        _ => {
            let left_type = get_type_name(left);
            let right_type = get_type_name(right);
            left_type.cmp(right_type)
        }
    }
}

fn get_type_name(value: &Value) -> &str {
    match value.data() {
        ValueData::Boolean(_) => "Boolean",
        ValueData::Integer(_) => "Integer",
        ValueData::Decimal(_) => "Decimal",
        ValueData::String(_) => "String",
        ValueData::Date { .. } => "Date",
        ValueData::DateTime {
            value: _,
            precision: _,
            timezone_offset: _,
        } => "DateTime",
        ValueData::Time {
            value: _,
            precision: _,
        } => "Time",
        ValueData::Quantity { .. } => "Quantity",
        ValueData::Object(_) => "Object",
        ValueData::LazyJson { .. } => "Object", // Lazy JSON materializes to Object
        ValueData::Empty => "Empty",
    }
}

fn datetime_precision_from_digits(digits: i32) -> Option<crate::value::DateTimePrecision> {
    use crate::value::DateTimePrecision::*;
    match digits {
        4 => Some(Year),
        6 => Some(Month),
        8 => Some(Day),
        10 => Some(Hour),
        12 => Some(Minute),
        14 => Some(Second),
        17 => Some(Millisecond),
        _ => None,
    }
}

fn time_precision_from_digits(digits: i32) -> Option<crate::value::TimePrecision> {
    use crate::value::TimePrecision::*;
    match digits {
        2 => Some(Hour),
        4 => Some(Minute),
        6 => Some(Second),
        9 => Some(Millisecond),
        _ => None,
    }
}

fn digits_for_date_precision(p: crate::value::DatePrecision) -> i32 {
    match p {
        crate::value::DatePrecision::Year => 4,
        crate::value::DatePrecision::Month => 6,
        crate::value::DatePrecision::Day => 8,
    }
}

fn digits_for_datetime_precision(p: crate::value::DateTimePrecision) -> i32 {
    match p {
        crate::value::DateTimePrecision::Year => 4,
        crate::value::DateTimePrecision::Month => 6,
        crate::value::DateTimePrecision::Day => 8,
        crate::value::DateTimePrecision::Hour => 10,
        crate::value::DateTimePrecision::Minute => 12,
        crate::value::DateTimePrecision::Second => 14,
        crate::value::DateTimePrecision::Millisecond => 17,
    }
}

fn digits_for_time_precision(p: crate::value::TimePrecision) -> i32 {
    match p {
        crate::value::TimePrecision::Hour => 2,
        crate::value::TimePrecision::Minute => 4,
        crate::value::TimePrecision::Second => 6,
        crate::value::TimePrecision::Millisecond => 9,
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    use chrono::{Datelike, NaiveDate};
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year, month, 28).unwrap());
    (first_next - chrono::Duration::days(1)).day()
}

fn boundary_timezone_offset(input: Option<i32>, high: bool) -> Option<i32> {
    if input.is_some() {
        return input;
    }
    Some(if high { -12 * 3600 } else { 14 * 3600 })
}

pub fn low_boundary(
    collection: Collection,
    precision_arg: Option<&Collection>,
) -> Result<Collection> {
    // lowBoundary() returns the least possible value of the input to the specified precision
    // Supports Decimal, Date, DateTime, and Time values
    // Precision parameter: optional integer (0-31 for decimals, or precision level for temporal)

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "lowBoundary() requires singleton collection".into(),
        ));
    }

    let item = collection.iter().next().unwrap();

    // Extract precision if provided
    let precision = if let Some(prec_arg) = precision_arg {
        if prec_arg.is_empty() {
            None
        } else if prec_arg.len() > 1 {
            return Err(Error::TypeError(
                "lowBoundary() precision must be singleton".into(),
            ));
        } else {
            prec_arg.as_integer().ok().map(|i| i as i32)
        }
    } else {
        None
    };

    match item.data() {
        ValueData::String(s) => {
            if let Some(v) = crate::temporal_parse::parse_datetime_value_lenient(s.as_ref())
                .or_else(|| crate::temporal_parse::parse_date_value(s.as_ref()))
                .or_else(|| crate::temporal_parse::parse_time_value(s.as_ref()))
            {
                return low_boundary(Collection::singleton(v), precision_arg);
            }
            Err(Error::TypeError(
                "lowBoundary() requires Decimal, Date, DateTime, or Time value".into(),
            ))
        }
        ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            // For decimals, calculate low boundary based on precision
            // If precision is None, use default precision (scale of the decimal)
            let scale = d.scale() as i32;
            let prec = precision.unwrap_or(scale);

            // Validate precision: must be between 0 and 31
            if !(0..=31).contains(&prec) {
                return Ok(Collection::empty());
            }

            // Calculate low boundary: subtract half of the smallest unit at the given precision
            // For example, if value is 1.587 and precision is 2, low boundary is 1.58
            // We need to subtract 0.005 (half of 0.01)
            let divisor = Decimal::from(10_i64.pow(prec as u32));
            let half_unit = Decimal::from(5) / (divisor * Decimal::from(10));
            let low_bound = *d - half_unit;

            // Round to the specified precision
            let rounded = low_bound.round_dp(prec as u32);
            Ok(Collection::singleton(Value::decimal(rounded)))
        }
        ValueData::Integer(i) => {
            // Convert to decimal and handle precision
            let d = Decimal::from(*i);
            let prec = precision.unwrap_or(0);

            if !(0..=31).contains(&prec) {
                return Ok(Collection::empty());
            }

            if prec == 0 {
                // For integer precision 0, low boundary is value - 0.5, rounded down
                let low_bound = Decimal::from(*i) - Decimal::from_str("0.5").unwrap();
                Ok(Collection::singleton(Value::decimal(low_bound.round_dp(0))))
            } else {
                // For higher precision, convert to decimal and use decimal logic
                let divisor = Decimal::from(10_i64.pow(prec as u32));
                let half_unit = Decimal::from(5) / (divisor * Decimal::from(10));
                let low_bound = d - half_unit;
                let rounded = low_bound.round_dp(prec as u32);
                Ok(Collection::singleton(Value::decimal(rounded)))
            }
        }
        ValueData::Date {
            value: d,
            precision: input_prec,
        } => {
            use chrono::{Datelike, FixedOffset, NaiveDate, NaiveDateTime, Utc};

            let desired_digits = precision.unwrap_or(17);
            let Some(desired_dt_prec) = datetime_precision_from_digits(desired_digits) else {
                return Ok(Collection::empty());
            };

            // Date boundaries always return a dateTime (FHIR dateTime can carry date-only precision).
            let input_digits = digits_for_date_precision(*input_prec);

            let year = d.year();
            let month = if desired_digits >= 6 {
                if input_digits >= 6 {
                    d.month()
                } else {
                    1
                }
            } else {
                1
            };
            let day = if desired_digits >= 8 {
                if input_digits >= 8 {
                    d.day()
                } else {
                    1
                }
            } else {
                1
            };

            let date = NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| Error::InvalidOperation("Invalid date boundary".into()))?;
            let naive = NaiveDateTime::new(date, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
            let tz_out = if desired_digits <= 8 {
                None
            } else {
                boundary_timezone_offset(None, false)
            };
            let dt_utc = if let Some(offset_secs) = tz_out {
                let offset = FixedOffset::east_opt(offset_secs)
                    .ok_or_else(|| Error::InvalidOperation("Invalid timezone offset".into()))?;
                offset
                    .from_local_datetime(&naive)
                    .single()
                    .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?
                    .with_timezone(&Utc)
            } else {
                chrono::DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
            };
            Ok(Collection::singleton(
                Value::datetime_with_precision_and_offset(dt_utc, desired_dt_prec, tz_out),
            ))
        }
        ValueData::DateTime {
            value: dt,
            precision: input_prec,
            timezone_offset: input_tz,
        } => {
            use chrono::{
                Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc,
            };

            let desired_digits = precision.unwrap_or(17);
            let Some(desired_prec) = datetime_precision_from_digits(desired_digits) else {
                return Ok(Collection::empty());
            };

            let input_digits = digits_for_datetime_precision(*input_prec);

            let local_naive = if let Some(offset) = input_tz {
                let offset = FixedOffset::east_opt(*offset)
                    .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
                dt.with_timezone(&offset).naive_local()
            } else {
                dt.naive_utc()
            };

            let year = local_naive.date().year();
            let month = if desired_digits >= 6 {
                if input_digits >= 6 {
                    local_naive.date().month()
                } else {
                    1
                }
            } else {
                1
            };
            let day = if desired_digits >= 8 {
                if input_digits >= 8 {
                    local_naive.date().day()
                } else {
                    1
                }
            } else {
                1
            };

            let hour = if desired_digits >= 10 {
                if input_digits >= 10 {
                    local_naive.time().hour()
                } else {
                    0
                }
            } else {
                0
            };
            let minute = if desired_digits >= 12 {
                if input_digits >= 12 {
                    local_naive.time().minute()
                } else {
                    0
                }
            } else {
                0
            };
            let second = if desired_digits >= 14 {
                if input_digits >= 14 {
                    local_naive.time().second()
                } else {
                    0
                }
            } else {
                0
            };
            let ms = if desired_digits >= 17 {
                if input_digits >= 17 {
                    local_naive.time().nanosecond() / 1_000_000
                } else {
                    0
                }
            } else {
                0
            };

            let date = NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?;
            let time = NaiveTime::from_hms_nano_opt(hour, minute, second, ms * 1_000_000)
                .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?;
            let local = NaiveDateTime::new(date, time);

            let tz_out = if desired_digits <= 8 {
                None
            } else {
                boundary_timezone_offset(*input_tz, false)
            };

            let dt_utc = if let Some(offset_secs) = tz_out {
                let offset = FixedOffset::east_opt(offset_secs)
                    .ok_or_else(|| Error::InvalidOperation("Invalid timezone offset".into()))?;
                offset
                    .from_local_datetime(&local)
                    .single()
                    .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?
                    .with_timezone(&Utc)
            } else {
                chrono::DateTime::<Utc>::from_naive_utc_and_offset(local, Utc)
            };

            Ok(Collection::singleton(
                Value::datetime_with_precision_and_offset(dt_utc, desired_prec, tz_out),
            ))
        }
        ValueData::Time {
            value: t,
            precision: input_prec,
        } => {
            use chrono::{NaiveTime, Timelike};
            let desired_digits = precision.unwrap_or(9);
            let Some(desired_prec) = time_precision_from_digits(desired_digits) else {
                return Ok(Collection::empty());
            };
            let input_digits = digits_for_time_precision(*input_prec);

            let hour = t.hour();
            let minute = if desired_digits >= 4 {
                if input_digits >= 4 {
                    t.minute()
                } else {
                    0
                }
            } else {
                0
            };
            let second = if desired_digits >= 6 {
                if input_digits >= 6 {
                    t.second()
                } else {
                    0
                }
            } else {
                0
            };
            let ms = if desired_digits >= 9 {
                if input_digits >= 9 {
                    t.nanosecond() / 1_000_000
                } else {
                    0
                }
            } else {
                0
            };

            let out = NaiveTime::from_hms_nano_opt(hour, minute, second, ms * 1_000_000)
                .ok_or_else(|| Error::InvalidOperation("Invalid time boundary".into()))?;
            Ok(Collection::singleton(Value::time_with_precision(
                out,
                desired_prec,
            )))
        }
        ValueData::Quantity { value, unit } => {
            use rust_decimal::Decimal;
            // For quantities, apply lowBoundary to the value and keep the unit
            let scale = value.scale() as i32;
            let prec = precision.unwrap_or(scale);

            if !(0..=31).contains(&prec) {
                return Ok(Collection::empty());
            }

            let divisor = Decimal::from(10_i64.pow(prec as u32));
            let half_unit = Decimal::from(5) / (divisor * Decimal::from(10));
            let low_bound = *value - half_unit;
            let rounded = low_bound.round_dp(prec as u32);

            Ok(Collection::singleton(Value::quantity(
                rounded,
                Arc::clone(unit),
            )))
        }
        _ => Err(Error::TypeError(
            "lowBoundary() requires Decimal, Date, DateTime, or Time value".into(),
        )),
    }
}

pub fn high_boundary(
    collection: Collection,
    precision_arg: Option<&Collection>,
) -> Result<Collection> {
    // highBoundary() returns the greatest possible value of the input to the specified precision
    // Supports Decimal, Date, DateTime, and Time values
    // Precision parameter: optional integer (0-31 for decimals, or precision level for temporal)

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "highBoundary() requires singleton collection".into(),
        ));
    }

    let item = collection.iter().next().unwrap();

    // Extract precision if provided
    let precision = if let Some(prec_arg) = precision_arg {
        if prec_arg.is_empty() {
            None
        } else if prec_arg.len() > 1 {
            return Err(Error::TypeError(
                "highBoundary() precision must be singleton".into(),
            ));
        } else {
            prec_arg.as_integer().ok().map(|i| i as i32)
        }
    } else {
        None
    };

    match item.data() {
        ValueData::String(s) => {
            if let Some(v) = crate::temporal_parse::parse_datetime_value_lenient(s.as_ref())
                .or_else(|| crate::temporal_parse::parse_date_value(s.as_ref()))
                .or_else(|| crate::temporal_parse::parse_time_value(s.as_ref()))
            {
                return high_boundary(Collection::singleton(v), precision_arg);
            }
            Err(Error::TypeError(
                "highBoundary() requires Decimal, Date, DateTime, or Time value".into(),
            ))
        }
        ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            // For decimals, calculate high boundary based on precision
            let scale = d.scale() as i32;
            let prec = precision.unwrap_or(scale);

            // Validate precision: must be between 0 and 31
            if !(0..=31).contains(&prec) {
                return Ok(Collection::empty());
            }

            // Calculate high boundary: add half of the smallest unit at the given precision (but not quite reaching the next value)
            // For example, if value is 1.587 and precision is 2, high boundary is 1.59 (but actually 1.584999...)
            let divisor = Decimal::from(10_i64.pow(prec as u32));
            let half_unit = Decimal::from(5) / (divisor * Decimal::from(10));
            // High boundary is just below the next value at the given precision
            let high_bound = *d + half_unit;

            // Round to the specified precision
            let rounded = high_bound.round_dp(prec as u32);
            Ok(Collection::singleton(Value::decimal(rounded)))
        }
        ValueData::Integer(i) => {
            // Convert to decimal and handle precision
            let d = Decimal::from(*i);
            let prec = precision.unwrap_or(0);

            if !(0..=31).contains(&prec) {
                return Ok(Collection::empty());
            }

            if prec == 0 {
                // For integer precision 0, high boundary is value + 0.5, rounded down (so it's just below next integer)
                let high_bound = Decimal::from(*i) + Decimal::from_str("0.5").unwrap();
                Ok(Collection::singleton(Value::decimal(
                    high_bound.round_dp(0),
                )))
            } else {
                // For higher precision, convert to decimal and use decimal logic
                let divisor = Decimal::from(10_i64.pow(prec as u32));
                let half_unit = Decimal::from(5) / (divisor * Decimal::from(10));
                let high_bound = d + half_unit;
                let rounded = high_bound.round_dp(prec as u32);
                Ok(Collection::singleton(Value::decimal(rounded)))
            }
        }
        ValueData::Date {
            value: d,
            precision: input_prec,
        } => {
            use chrono::{
                Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
            };

            let desired_digits = precision.unwrap_or(17);
            let Some(desired_dt_prec) = datetime_precision_from_digits(desired_digits) else {
                return Ok(Collection::empty());
            };
            let input_digits = digits_for_date_precision(*input_prec);

            let year = d.year();
            let month = if desired_digits >= 6 {
                if input_digits >= 6 {
                    d.month()
                } else {
                    12
                }
            } else {
                12
            };
            let day = if desired_digits >= 8 {
                if input_digits >= 8 {
                    d.day()
                } else {
                    last_day_of_month(year, month)
                }
            } else {
                last_day_of_month(year, month)
            };

            let hour = if desired_digits > 8 { 23 } else { 0 };
            let minute = if desired_digits > 8 { 59 } else { 0 };
            let second = if desired_digits > 8 { 59 } else { 0 };
            let ms = if desired_digits > 8 { 999 } else { 0 };

            let date = NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| Error::InvalidOperation("Invalid date boundary".into()))?;
            let time = NaiveTime::from_hms_nano_opt(hour, minute, second, ms * 1_000_000)
                .ok_or_else(|| Error::InvalidOperation("Invalid date boundary time".into()))?;
            let local = NaiveDateTime::new(date, time);

            let tz_out = if desired_digits <= 8 {
                None
            } else {
                boundary_timezone_offset(None, true)
            };

            let dt_utc = if let Some(offset_secs) = tz_out {
                let offset = FixedOffset::east_opt(offset_secs)
                    .ok_or_else(|| Error::InvalidOperation("Invalid timezone offset".into()))?;
                offset
                    .from_local_datetime(&local)
                    .single()
                    .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?
                    .with_timezone(&Utc)
            } else {
                chrono::DateTime::<Utc>::from_naive_utc_and_offset(local, Utc)
            };

            Ok(Collection::singleton(
                Value::datetime_with_precision_and_offset(dt_utc, desired_dt_prec, tz_out),
            ))
        }
        ValueData::DateTime {
            value: dt,
            precision: input_prec,
            timezone_offset: input_tz,
        } => {
            use chrono::{
                Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc,
            };

            let desired_digits = precision.unwrap_or(17);
            let Some(desired_prec) = datetime_precision_from_digits(desired_digits) else {
                return Ok(Collection::empty());
            };
            let input_digits = digits_for_datetime_precision(*input_prec);

            let local_naive = if let Some(offset) = input_tz {
                let offset = FixedOffset::east_opt(*offset)
                    .ok_or_else(|| Error::InvalidOperation("Invalid timezone offset".into()))?;
                dt.with_timezone(&offset).naive_local()
            } else {
                dt.naive_utc()
            };

            let year = local_naive.date().year();
            let month = if desired_digits >= 6 {
                if input_digits >= 6 {
                    local_naive.date().month()
                } else {
                    12
                }
            } else {
                12
            };
            let day = if desired_digits >= 8 {
                if input_digits >= 8 {
                    local_naive.date().day()
                } else {
                    last_day_of_month(year, month)
                }
            } else {
                last_day_of_month(year, month)
            };

            let hour = if desired_digits > 8 {
                if input_digits >= 10 {
                    local_naive.time().hour()
                } else {
                    23
                }
            } else {
                0
            };
            let minute = if desired_digits > 8 {
                if input_digits >= 12 {
                    local_naive.time().minute()
                } else {
                    59
                }
            } else {
                0
            };
            let second = if desired_digits > 8 {
                if input_digits >= 14 {
                    local_naive.time().second()
                } else {
                    59
                }
            } else {
                0
            };
            let ms = if desired_digits > 8 {
                if input_digits >= 17 {
                    local_naive.time().nanosecond() / 1_000_000
                } else {
                    999
                }
            } else {
                0
            };

            let date = NaiveDate::from_ymd_opt(year, month, day)
                .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?;
            let time = NaiveTime::from_hms_nano_opt(hour, minute, second, ms * 1_000_000)
                .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?;
            let local = NaiveDateTime::new(date, time);

            let tz_out = if desired_digits <= 8 {
                None
            } else {
                boundary_timezone_offset(*input_tz, true)
            };
            let dt_utc = if let Some(offset_secs) = tz_out {
                let offset = FixedOffset::east_opt(offset_secs)
                    .ok_or_else(|| Error::InvalidOperation("Invalid timezone offset".into()))?;
                offset
                    .from_local_datetime(&local)
                    .single()
                    .ok_or_else(|| Error::InvalidOperation("Invalid datetime boundary".into()))?
                    .with_timezone(&Utc)
            } else {
                chrono::DateTime::<Utc>::from_naive_utc_and_offset(local, Utc)
            };

            Ok(Collection::singleton(
                Value::datetime_with_precision_and_offset(dt_utc, desired_prec, tz_out),
            ))
        }
        ValueData::Time {
            value: t,
            precision: input_prec,
        } => {
            use chrono::{NaiveTime, Timelike};
            let desired_digits = precision.unwrap_or(9);
            let Some(desired_prec) = time_precision_from_digits(desired_digits) else {
                return Ok(Collection::empty());
            };
            let input_digits = digits_for_time_precision(*input_prec);

            let hour = if input_digits >= 2 { t.hour() } else { 23 };
            let minute = if desired_digits > 2 {
                if input_digits >= 4 {
                    t.minute()
                } else {
                    59
                }
            } else {
                59
            };
            let second = if desired_digits > 2 {
                if input_digits >= 6 {
                    t.second()
                } else {
                    59
                }
            } else {
                59
            };
            let ms = if desired_digits > 2 {
                if input_digits >= 9 {
                    t.nanosecond() / 1_000_000
                } else {
                    999
                }
            } else {
                999
            };

            let out = NaiveTime::from_hms_nano_opt(hour, minute, second, ms * 1_000_000)
                .ok_or_else(|| Error::InvalidOperation("Invalid time boundary".into()))?;
            Ok(Collection::singleton(Value::time_with_precision(
                out,
                desired_prec,
            )))
        }
        ValueData::Quantity { value, unit } => {
            use rust_decimal::Decimal;
            // For quantities, apply highBoundary to the value and keep the unit
            let scale = value.scale() as i32;
            let prec = precision.unwrap_or(scale);

            if !(0..=31).contains(&prec) {
                return Ok(Collection::empty());
            }

            let divisor = Decimal::from(10_i64.pow(prec as u32));
            let half_unit = Decimal::from(5) / (divisor * Decimal::from(10));
            let high_bound = *value + half_unit;
            let rounded = high_bound.round_dp(prec as u32);

            Ok(Collection::singleton(Value::quantity(
                rounded,
                Arc::clone(unit),
            )))
        }
        _ => Err(Error::TypeError(
            "highBoundary() requires Decimal, Date, DateTime, or Time value".into(),
        )),
    }
}

pub fn comparable(collection: Collection, other_arg: Option<&Collection>) -> Result<Collection> {
    if collection.is_empty() || other_arg.is_none() {
        return Ok(Collection::empty());
    }

    let other = other_arg.unwrap();
    if other.is_empty() {
        return Ok(Collection::empty());
    }

    // Both collections must be singletons
    if collection.len() != 1 || other.len() != 1 {
        return Err(Error::TypeError(
            "comparable() requires singleton collections".into(),
        ));
    }

    let left = collection.iter().next().unwrap().clone();
    let right = other.iter().next().unwrap().clone();

    // Use the same semantics as the ordering operators: if `<` yields a boolean, values are comparable.
    let comparable = match execute_binary_op(
        HirBinaryOperator::Lt,
        Collection::singleton(left),
        Collection::singleton(right),
    ) {
        Ok(result) => !result.is_empty(),
        Err(_) => false,
    };

    Ok(Collection::singleton(Value::boolean(comparable)))
}

pub fn precision(collection: Collection) -> Result<Collection> {
    // precision() returns the precision for Decimal, Date, DateTime, and Time values
    // For Decimal: number of decimal places
    // For Date: number of significant components (4=year, 7=year-month, 10=year-month-day)
    // For DateTime: number of significant components including time
    // For Time: number of significant time components

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "precision() requires singleton collection".into(),
        ));
    }

    let item = collection.iter().next().unwrap();

    match item.data() {
        ValueData::Decimal(d) => Ok(Collection::singleton(Value::integer(d.scale() as i64))),
        ValueData::Date { precision, .. } => {
            let p = match precision {
                crate::value::DatePrecision::Year => 4,
                crate::value::DatePrecision::Month => 6,
                crate::value::DatePrecision::Day => 8,
            };
            Ok(Collection::singleton(Value::integer(p)))
        }
        ValueData::DateTime { precision, .. } => {
            let p = match precision {
                crate::value::DateTimePrecision::Year => 4,
                crate::value::DateTimePrecision::Month => 6,
                crate::value::DateTimePrecision::Day => 8,
                crate::value::DateTimePrecision::Hour => 10,
                crate::value::DateTimePrecision::Minute => 12,
                crate::value::DateTimePrecision::Second => 14,
                crate::value::DateTimePrecision::Millisecond => 17,
            };
            Ok(Collection::singleton(Value::integer(p)))
        }
        ValueData::Time { precision, .. } => {
            let p = match precision {
                crate::value::TimePrecision::Hour => 2,
                crate::value::TimePrecision::Minute => 4,
                crate::value::TimePrecision::Second => 6,
                crate::value::TimePrecision::Millisecond => 9,
            };
            Ok(Collection::singleton(Value::integer(p)))
        }
        _ => Ok(Collection::empty()),
    }
}

pub fn type_function(
    collection: Collection,
    path_hint: Option<&str>,
    fhir_context: Option<&dyn FhirContext>,
    ctx: &Context,
) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // If we know the declared type from the FHIR context, return it directly
    if let (Some(fc), Some(path), ValueData::Object(root_obj)) =
        (fhir_context, path_hint, ctx.resource.data())
    {
        if let Some(rt_col) = root_obj.get("resourceType") {
            if let Some(rt_val) = rt_col.iter().next() {
                if let ValueData::String(rt) = rt_val.data() {
                    if let Ok(Some(elem)) = fc.resolve_path_type(rt.as_ref(), path) {
                        let declared: Vec<String> = elem
                            .type_codes
                            .iter()
                            .map(|s| normalize_type_code(s))
                            .collect();
                        if let Some(item) = collection.iter().next() {
                            if let Some(chosen) =
                                choose_declared_type_for_value(item, &declared, Some(path))
                            {
                                let desc = TypeDescriptor {
                                    namespace: "FHIR",
                                    name: chosen,
                                    base_type: None,
                                };
                                return Ok(Collection::singleton(type_info_value(&desc)));
                            }
                        }
                    }
                }
            }
        }
    }

    let mut descriptors = collection
        .iter()
        .map(|item| infer_type_descriptor(item, path_hint));

    let first = descriptors.next().unwrap();
    for desc in descriptors {
        if desc.namespace != first.namespace || !desc.name.eq_ignore_ascii_case(&first.name) {
            return Err(Error::TypeError(
                "type() requires all items to have the same type".into(),
            ));
        }
    }

    Ok(Collection::singleton(type_info_value(&first)))
}

pub fn conforms_to(
    collection: Collection,
    structure_arg: Option<&Collection>,
    ctx: &Context,
) -> Result<Collection> {
    // conformsTo() checks if a value conforms to a given structure definition

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let structure = structure_arg.ok_or_else(|| {
        Error::InvalidOperation("conformsTo() requires a structure definition url".into())
    })?;
    let structure_url = structure
        .as_string()
        .map_err(|_| Error::TypeError("conformsTo() expects a canonical url string".into()))?;

    // Basic validation: canonical should reference a StructureDefinition
    if !structure_url.as_ref().contains("StructureDefinition") {
        return Err(Error::InvalidOperation(
            "Invalid StructureDefinition canonical".into(),
        ));
    }

    // Determine the type name from the URL (last non-empty path segment)
    let type_name = structure_url
        .as_ref()
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(structure_url.as_ref());

    // Inspect the target value's resourceType (fallback to root resource)
    let target = collection.iter().next().unwrap_or(&ctx.resource);
    let target_type = match target.data() {
        ValueData::Object(obj) => obj
            .get("resourceType")
            .and_then(|col| col.iter().next())
            .and_then(|v| match v.data() {
                ValueData::String(s) => Some(s.as_ref().to_string()),
                _ => None,
            }),
        ValueData::LazyJson { .. } => target
            .data()
            .resolved_json()
            .and_then(|v| v.get("resourceType"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    };

    let conforms = target_type
        .map(|rt| rt.eq_ignore_ascii_case(type_name))
        .unwrap_or(false);

    Ok(Collection::singleton(Value::boolean(conforms)))
}

pub fn has_value(collection: Collection) -> Result<Collection> {
    // Returns true if the input collection contains a single value which is not empty
    // Returns false if collection is empty, has more than one item, or contains only empty values

    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(false)));
    }

    // If collection has more than one item, return false
    if collection.len() > 1 {
        return Ok(Collection::singleton(Value::boolean(false)));
    }

    // Get the single item
    let item = collection.iter().next().unwrap();

    // Check if item has a value (is not empty)
    match item.data() {
        ValueData::Empty => Ok(Collection::singleton(Value::boolean(false))),
        ValueData::String(s) => {
            // For strings, check if not empty
            Ok(Collection::singleton(Value::boolean(!s.is_empty())))
        }
        _ => Ok(Collection::singleton(Value::boolean(true))),
    }
}

/// Resolve references to resources
///
/// This function resolves FHIR references in three ways:
/// 1. Contained resources (references starting with '#')
/// 2. External resources (via custom ResourceResolver if provided)
/// 3. Already-resolved resource objects (pass-through)
///
/// If a custom ResourceResolver is provided, it will be used for external references.
/// Otherwise, only contained references can be resolved.
pub fn resolve(
    collection: Collection,
    ctx: &Context,
    resource_resolver: Option<&Arc<dyn ResourceResolver>>,
) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Build index of contained resources by local reference (#id)
    let mut contained_index: HashMap<String, Value> = HashMap::new();
    if let ValueData::Object(obj) = ctx.resource.data() {
        if let Some(contained) = obj.get("contained") {
            for res in contained.iter() {
                if let ValueData::Object(res_obj) = res.data() {
                    if let Some(id_col) = res_obj.get("id") {
                        if let Some(id_val) = id_col.iter().next() {
                            if let ValueData::String(id_str) = id_val.data() {
                                contained_index
                                    .insert(format!("#{}", id_str.as_ref()), res.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut resolved = Collection::empty();

    for item in collection.iter() {
        let reference = match item.data() {
            ValueData::String(s) => Some(s.as_ref().to_string()),
            ValueData::Object(obj) => obj
                .get("reference")
                .and_then(|col| col.iter().next())
                .and_then(|v| match v.data() {
                    ValueData::String(s) => Some(s.as_ref().to_string()),
                    _ => None,
                }),
            _ => None,
        };

        if let Some(ref_str) = reference {
            // Check if it's a contained reference first
            if ref_str.starts_with('#') {
                if let Some(target) = contained_index.get(&ref_str) {
                    resolved.push(target.clone());
                }
            } else if let Some(resolver) = resource_resolver {
                // Use custom resolver for external references
                match resolver.resolve(&ref_str) {
                    Ok(Some(resource)) => {
                        resolved.push(resource);
                    }
                    Ok(None) => {
                        // Reference not found - skip (don't add to result)
                    }
                    Err(e) => {
                        // Resolution error - propagate
                        return Err(e);
                    }
                }
            }
            // else: external reference without resolver - skip
        } else if matches!(item.data(), ValueData::Object(_)) {
            // Already a resource-like object - treat as resolved
            resolved.push(item.clone());
        }
    }

    Ok(resolved)
}
