//! Mathematical functions for FHIRPath.
//!
//! This module implements mathematical operations like `abs()`, `ceiling()`, `floor()`,
//! `round()`, `sqrt()`, `power()`, `log()`, etc.

use std::str::FromStr;

use rust_decimal::Decimal;

use crate::error::{Error, Result};
use crate::value::{Collection, Value, ValueData};

pub fn abs(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let item = collection.iter().next().unwrap();
    match item.data() {
        ValueData::Integer(i) => Ok(Collection::singleton(Value::integer(i.abs()))),
        ValueData::Decimal(d) => Ok(Collection::singleton(Value::decimal(d.abs()))),
        ValueData::Quantity { value, unit } => Ok(Collection::singleton(Value::quantity(
            value.abs(),
            unit.clone(),
        ))),
        _ => Err(Error::TypeError("abs() requires numeric type".into())),
    }
}

pub fn ceiling(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let item = collection.iter().next().unwrap();
    match item.data() {
        ValueData::Decimal(d) => {
            // Use proper ceiling: round up to nearest integer
            // For positive: if fractional part exists, add 1 to rounded value
            // For negative: just round (ceiling of -3.7 is -3, not -4)
            let rounded = d.round_dp(0);
            let result = if *d > rounded {
                rounded + Decimal::ONE
            } else {
                rounded
            };
            Ok(Collection::singleton(Value::decimal(result)))
        }
        ValueData::Integer(i) => Ok(Collection::singleton(Value::integer(*i))),
        _ => Err(Error::TypeError("ceiling() requires numeric type".into())),
    }
}

pub fn exp(collection: Collection) -> Result<Collection> {
    // exp() returns e raised to the power of the input
    // If the input collection contains an Integer, it will be implicitly converted to a Decimal

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "exp() requires singleton collection".into(),
        ));
    }

    let item = collection.iter().next().unwrap();

    // Convert to decimal for calculation
    let num = match item.data() {
        ValueData::Integer(i) => Decimal::from(*i),
        ValueData::Decimal(d) => *d,
        _ => return Err(Error::TypeError("exp() requires numeric value".into())),
    };

    // Convert to f64 for calculation
    let num_f64: f64 = num
        .to_string()
        .parse::<f64>()
        .map_err(|_| Error::InvalidOperation("exp() input value too large".into()))?;

    // Calculate exp
    let result_f64 = num_f64.exp();

    // Check for overflow
    if result_f64.is_infinite() || result_f64.is_nan() {
        return Err(Error::InvalidOperation(
            "exp() result too large to represent".into(),
        ));
    }

    // Convert back to Decimal
    let result = Decimal::from_str(&result_f64.to_string())
        .map_err(|_| Error::InvalidOperation("exp() result cannot be represented".into()))?;

    Ok(Collection::singleton(Value::decimal(result)))
}

pub fn floor(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let item = collection.iter().next().unwrap();
    match item.data() {
        ValueData::Decimal(d) => {
            // Floor: round down to nearest integer
            // For positive: just round (floor of 3.7 is 3)
            // For negative: if fractional part exists, subtract 1 from rounded value
            let rounded = d.round_dp(0);
            let result = if *d < rounded {
                rounded - Decimal::ONE
            } else {
                rounded
            };
            Ok(Collection::singleton(Value::decimal(result)))
        }
        ValueData::Integer(i) => Ok(Collection::singleton(Value::integer(*i))),
        _ => Err(Error::TypeError("floor() requires numeric type".into())),
    }
}

pub fn ln(collection: Collection) -> Result<Collection> {
    // ln() returns the natural logarithm of the input (i.e. the logarithm base e)
    // When used with an Integer, it will be implicitly converted to a Decimal

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "ln() requires singleton collection".into(),
        ));
    }

    let item = collection.iter().next().unwrap();

    // Convert to decimal for calculation
    let num = match item.data() {
        ValueData::Integer(i) => Decimal::from(*i),
        ValueData::Decimal(d) => *d,
        _ => return Err(Error::TypeError("ln() requires numeric value".into())),
    };

    // Check for non-positive numbers
    if num <= Decimal::ZERO {
        return Err(Error::InvalidOperation(
            "ln() called with non-positive number".into(),
        ));
    }

    // Convert to f64 for calculation
    let num_f64: f64 = num
        .to_string()
        .parse()
        .map_err(|_| Error::InvalidOperation("ln() input value too large".into()))?;

    // Calculate natural logarithm
    let result_f64 = num_f64.ln();

    // Check for invalid result
    if result_f64.is_infinite() || result_f64.is_nan() {
        return Err(Error::InvalidOperation(
            "ln() called with invalid input".into(),
        ));
    }

    // Convert back to Decimal
    let result = Decimal::from_str(&result_f64.to_string())
        .map_err(|_| Error::InvalidOperation("ln() result cannot be represented".into()))?;

    Ok(Collection::singleton(Value::decimal(result)))
}

pub fn log(collection: Collection, base_arg: Option<&Collection>) -> Result<Collection> {
    // log() returns the logarithm base base of the input number
    // When used with Integers, the arguments will be implicitly converted to Decimal

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let base = base_arg
        .ok_or_else(|| Error::InvalidOperation("log() requires 1 argument (base)".into()))?;

    if base.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 || base.len() > 1 {
        return Err(Error::TypeError(
            "log() requires singleton collections".into(),
        ));
    }

    let number_item = collection.iter().next().unwrap();
    let base_item = base.iter().next().unwrap();

    // Convert to decimals for calculation
    let num = match number_item.data() {
        ValueData::Integer(i) => Decimal::from(*i),
        ValueData::Decimal(d) => *d,
        _ => return Err(Error::TypeError("log() requires numeric value".into())),
    };

    let base_num = match base_item.data() {
        ValueData::Integer(i) => Decimal::from(*i),
        ValueData::Decimal(d) => *d,
        _ => return Err(Error::TypeError("log() base must be numeric".into())),
    };

    // Check for non-positive numbers
    if num <= Decimal::ZERO || base_num <= Decimal::ZERO {
        return Err(Error::InvalidOperation(
            "log() called with non-positive number".into(),
        ));
    }

    // Check for base 1
    if base_num == Decimal::ONE {
        return Err(Error::InvalidOperation("log() called with base 1".into()));
    }

    // Convert to f64 for calculation
    let num_f64: f64 = num
        .to_string()
        .parse()
        .map_err(|_| Error::InvalidOperation("log() input value too large".into()))?;
    let base_f64: f64 = base_num
        .to_string()
        .parse()
        .map_err(|_| Error::InvalidOperation("log() base value too large".into()))?;

    // Calculate logarithm: log_base(num) = ln(num) / ln(base)
    let result_f64 = num_f64.ln() / base_f64.ln();

    // Check for invalid result
    if result_f64.is_infinite() || result_f64.is_nan() {
        return Err(Error::InvalidOperation(
            "log() called with invalid input".into(),
        ));
    }

    // Convert back to Decimal
    let result = Decimal::from_str(&result_f64.to_string())
        .map_err(|_| Error::InvalidOperation("log() result cannot be represented".into()))?;

    Ok(Collection::singleton(Value::decimal(result)))
}

pub fn power(collection: Collection, exponent_arg: Option<&Collection>) -> Result<Collection> {
    // power() raises a number to the exponent power
    // If used with Integers, result is Integer. If used with Decimals, result is Decimal.
    // If mixed types, Integer is converted to Decimal and result is Decimal.

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let exponent = exponent_arg
        .ok_or_else(|| Error::InvalidOperation("power() requires 1 argument (exponent)".into()))?;

    if exponent.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 || exponent.len() > 1 {
        return Err(Error::TypeError(
            "power() requires singleton collections".into(),
        ));
    }

    let base_item = collection.iter().next().unwrap();
    let exp_item = exponent.iter().next().unwrap();

    // Extract numbers
    let (base_num, base_is_int) = match base_item.data() {
        ValueData::Integer(i) => (Decimal::from(*i), true),
        ValueData::Decimal(d) => (*d, false),
        _ => return Err(Error::TypeError("power() requires numeric base".into())),
    };

    let (exp_num, exp_is_int) = match exp_item.data() {
        ValueData::Integer(i) => (Decimal::from(*i), true),
        ValueData::Decimal(d) => (*d, false),
        _ => return Err(Error::TypeError("power() requires numeric exponent".into())),
    };

    // Check for cases that cannot be represented
    // Negative base with non-integer exponent
    if base_num < Decimal::ZERO {
        let exp_f64: f64 = exp_num
            .to_string()
            .parse()
            .map_err(|_| Error::InvalidOperation("power() exponent value too large".into()))?;
        if exp_f64.fract() != 0.0 {
            // Non-integer exponent with negative base - return empty
            return Ok(Collection::empty());
        }
    }

    // Convert to f64 for calculation
    let base_f64: f64 = base_num
        .to_string()
        .parse()
        .map_err(|_| Error::InvalidOperation("power() base value too large".into()))?;
    let exp_f64: f64 = exp_num
        .to_string()
        .parse()
        .map_err(|_| Error::InvalidOperation("power() exponent value too large".into()))?;

    // Calculate power
    let result_f64 = base_f64.powf(exp_f64);

    // Check for overflow or invalid result
    if result_f64.is_infinite() || result_f64.is_nan() {
        return Ok(Collection::empty());
    }

    // Determine result type based on input types
    if base_is_int && exp_is_int && exp_num >= Decimal::ZERO {
        // Both integers and non-negative exponent - try integer result
        // Check if result is an exact integer
        if result_f64.fract() == 0.0 {
            let result_int = result_f64 as i64;
            // Check if conversion back to f64 is exact (and within i64 range)
            if (result_int as f64) == result_f64 {
                return Ok(Collection::singleton(Value::integer(result_int)));
            }
        }
    }

    // At least one decimal or negative exponent - result is decimal
    let result = Decimal::from_str(&result_f64.to_string())
        .map_err(|_| Error::InvalidOperation("power() result cannot be represented".into()))?;

    Ok(Collection::singleton(Value::decimal(result)))
}

pub fn round(collection: Collection, precision_arg: Option<&Collection>) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let item = collection.iter().next().unwrap();
    match item.data() {
        ValueData::Decimal(d) => {
            let precision = if let Some(precision_arg) = precision_arg {
                precision_arg.as_integer()? as u32
            } else {
                0
            };
            Ok(Collection::singleton(Value::decimal(d.round_dp(precision))))
        }
        ValueData::Integer(i) => Ok(Collection::singleton(Value::integer(*i))),
        _ => Err(Error::TypeError("round() requires numeric type".into())),
    }
}

pub fn sqrt(collection: Collection) -> Result<Collection> {
    // sqrt() returns the square root of the input number as a Decimal
    // If the square root cannot be represented (such as sqrt(-1)), the result is empty

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.len() > 1 {
        return Err(Error::TypeError(
            "sqrt() requires singleton collection".into(),
        ));
    }

    let item = collection.iter().next().unwrap();

    // Convert to decimal for calculation
    let num = match item.data() {
        ValueData::Integer(i) => Decimal::from(*i),
        ValueData::Decimal(d) => *d,
        _ => return Err(Error::TypeError("sqrt() requires numeric value".into())),
    };

    // Check for negative numbers
    if num < Decimal::ZERO {
        return Ok(Collection::empty());
    }

    // Convert to f64 for calculation
    let num_f64: f64 = num
        .to_string()
        .parse()
        .map_err(|_| Error::InvalidOperation("sqrt() input value too large".into()))?;

    // Calculate square root
    let result_f64 = num_f64.sqrt();

    // Check for invalid result
    if result_f64.is_infinite() || result_f64.is_nan() {
        return Ok(Collection::empty());
    }

    // Convert back to Decimal
    let result = Decimal::from_str(&result_f64.to_string())
        .map_err(|_| Error::InvalidOperation("sqrt() result cannot be represented".into()))?;

    Ok(Collection::singleton(Value::decimal(result)))
}

pub fn truncate(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let item = collection.iter().next().unwrap();
    match item.data() {
        ValueData::Decimal(d) => {
            // Truncate: round towards zero (remove fractional part)
            // For positive: floor (3.7 -> 3)
            // For negative: ceiling (-3.7 -> -3)
            let rounded = d.round_dp(0);
            let result = if *d >= Decimal::ZERO {
                // Positive: use floor behavior
                if *d < rounded {
                    rounded - Decimal::ONE
                } else {
                    rounded
                }
            } else {
                // Negative: use ceiling behavior
                if *d > rounded {
                    rounded + Decimal::ONE
                } else {
                    rounded
                }
            };
            Ok(Collection::singleton(Value::decimal(result)))
        }
        ValueData::Integer(i) => Ok(Collection::singleton(Value::integer(*i))),
        _ => Err(Error::TypeError("truncate() requires numeric type".into())),
    }
}
