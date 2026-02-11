use crate::db::search::escape::unescape_search_value;
use rust_decimal::Decimal;

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam, SearchPrefix};

pub(in crate::db::search::query_builder) fn build_number_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let prefix = v.prefix.unwrap_or(SearchPrefix::Eq);
        let (min, max) = number_precision_range(&v.raw).ok()?;
        let clause = match prefix {
            SearchPrefix::Eq => {
                let min_idx = push_text(bind_params, min.to_string());
                let max_idx = push_text(bind_params, max.to_string());
                format!(
                    "(sp.value >= ${}::numeric AND sp.value < ${}::numeric)",
                    min_idx, max_idx
                )
            }
            SearchPrefix::Ne => {
                let min_idx = push_text(bind_params, min.to_string());
                let max_idx = push_text(bind_params, max.to_string());
                format!(
                    "(sp.value < ${}::numeric OR sp.value >= ${}::numeric)",
                    min_idx, max_idx
                )
            }
            SearchPrefix::Gt | SearchPrefix::Sa => {
                let max_idx = push_text(bind_params, max.to_string());
                format!("sp.value >= ${}::numeric", max_idx)
            }
            SearchPrefix::Ge => {
                let min_idx = push_text(bind_params, min.to_string());
                format!("sp.value >= ${}::numeric", min_idx)
            }
            SearchPrefix::Lt | SearchPrefix::Eb => {
                let min_idx = push_text(bind_params, min.to_string());
                format!("sp.value < ${}::numeric", min_idx)
            }
            SearchPrefix::Le => {
                let max_idx = push_text(bind_params, max.to_string());
                format!("sp.value < ${}::numeric", max_idx)
            }
            SearchPrefix::Ap => {
                let value = Decimal::from_str_exact(v.raw.trim()).ok()?;
                let precision = decimal_precision(v.raw.trim()).ok()?;
                let delta = (value.abs() / Decimal::new(10, 0)).max(precision);
                let min = value - delta;
                let max = value + delta;
                let min_idx = push_text(bind_params, min.to_string());
                let max_idx = push_text(bind_params, max.to_string());
                format!(
                    "(sp.value >= ${}::numeric AND sp.value <= ${}::numeric)",
                    min_idx, max_idx
                )
            }
        };
        parts.push(clause);
    }

    if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}

pub(in crate::db::search::query_builder) fn build_quantity_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let prefix = v.prefix.unwrap_or(SearchPrefix::Eq);
        let q = parse_quantity(&v.raw).ok()?;

        let number = Decimal::from_str_exact(q.number.trim()).ok()?;
        let (min, max) = number_precision_range(q.number).ok()?;
        let clause = match prefix {
            SearchPrefix::Eq => {
                let min_idx = push_text(bind_params, min.to_string());
                let max_idx = push_text(bind_params, max.to_string());
                format!(
                    "(sp.value >= ${}::numeric AND sp.value < ${}::numeric)",
                    min_idx, max_idx
                )
            }
            SearchPrefix::Ne => {
                let min_idx = push_text(bind_params, min.to_string());
                let max_idx = push_text(bind_params, max.to_string());
                format!(
                    "(sp.value < ${}::numeric OR sp.value >= ${}::numeric)",
                    min_idx, max_idx
                )
            }
            SearchPrefix::Gt | SearchPrefix::Sa => {
                let max_idx = push_text(bind_params, max.to_string());
                format!("sp.value >= ${}::numeric", max_idx)
            }
            SearchPrefix::Ge => {
                let min_idx = push_text(bind_params, min.to_string());
                format!("sp.value >= ${}::numeric", min_idx)
            }
            SearchPrefix::Lt | SearchPrefix::Eb => {
                let min_idx = push_text(bind_params, min.to_string());
                format!("sp.value < ${}::numeric", min_idx)
            }
            SearchPrefix::Le => {
                let max_idx = push_text(bind_params, max.to_string());
                format!("sp.value < ${}::numeric", max_idx)
            }
            SearchPrefix::Ap => {
                // +/- 10% around the value (at least the value's implied precision).
                let precision = decimal_precision(q.number).ok()?;
                let delta = (number.abs() / Decimal::new(10, 0)).max(precision);
                let min = number - delta;
                let max = number + delta;
                let min_idx = push_text(bind_params, min.to_string());
                let max_idx = push_text(bind_params, max.to_string());
                format!(
                    "(sp.value >= ${}::numeric AND sp.value <= ${}::numeric)",
                    min_idx, max_idx
                )
            }
        };

        let mut clause = clause;

        // Handle system and code filtering based on FHIR spec:
        // - When system+code specified: search only code field (precise matching)
        // - When ||code specified (no system): search BOTH code and unit fields
        // - When only value specified: no code/unit filtering
        match (&q.system, &q.code) {
            (Some(system), Some(code)) => {
                // Format: value|system|code
                // Per spec: "it is inappropriate to search on the human display for the unit"
                // Only match against the code field (precise)
                let sys_idx = push_text(bind_params, system.clone());
                let code_idx = push_text(bind_params, code.clone());
                clause.push_str(&format!(" AND sp.system = ${}", sys_idx));
                clause.push_str(&format!(" AND sp.code = ${}", code_idx));
            }
            (None, Some(code)) => {
                // Format: value||code
                // Per spec: "matches a quantity by value and code or unit"
                // Search BOTH code and unit fields
                let code_idx = push_text(bind_params, code.clone());
                clause.push_str(&format!(
                    " AND (sp.code = ${0} OR sp.unit = ${0})",
                    code_idx
                ));
            }
            (Some(system), None) => {
                // Format: value|system| (system without code)
                // Just filter by system
                let sys_idx = push_text(bind_params, system.clone());
                clause.push_str(&format!(" AND sp.system = ${}", sys_idx));
            }
            (None, None) => {
                // Format: value
                // No code/unit filtering - search by value only
            }
        }

        parts.push(format!("({})", clause));
    }

    if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}

pub(in crate::db::search::query_builder) fn build_number_json_clause(
    idx: usize,
    raw_value: &str,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let v = unescape_search_value(raw_value).ok()?;
    let (prefix, rest) = SearchPrefix::parse_prefix(v.as_str());
    let prefix = prefix.unwrap_or(SearchPrefix::Eq);
    let value_expr = format!("(sc.components->{}->>'value')::numeric", idx);

    let (min, max) = number_precision_range(rest).ok()?;
    let clause = match prefix {
        SearchPrefix::Eq => {
            let min_idx = push_text(bind_params, min.to_string());
            let max_idx = push_text(bind_params, max.to_string());
            format!(
                "({} >= ${}::numeric AND {} < ${}::numeric)",
                value_expr, min_idx, value_expr, max_idx
            )
        }
        SearchPrefix::Ne => {
            let min_idx = push_text(bind_params, min.to_string());
            let max_idx = push_text(bind_params, max.to_string());
            format!(
                "({} < ${}::numeric OR {} >= ${}::numeric)",
                value_expr, min_idx, value_expr, max_idx
            )
        }
        SearchPrefix::Gt | SearchPrefix::Sa => {
            let max_idx = push_text(bind_params, max.to_string());
            format!("{} >= ${}::numeric", value_expr, max_idx)
        }
        SearchPrefix::Ge => {
            let min_idx = push_text(bind_params, min.to_string());
            format!("{} >= ${}::numeric", value_expr, min_idx)
        }
        SearchPrefix::Lt | SearchPrefix::Eb => {
            let min_idx = push_text(bind_params, min.to_string());
            format!("{} < ${}::numeric", value_expr, min_idx)
        }
        SearchPrefix::Le => {
            let max_idx = push_text(bind_params, max.to_string());
            format!("{} < ${}::numeric", value_expr, max_idx)
        }
        SearchPrefix::Ap => {
            let value = Decimal::from_str_exact(rest.trim()).ok()?;
            let precision = decimal_precision(rest.trim()).ok()?;
            let delta = (value.abs() / Decimal::new(10, 0)).max(precision);
            let min = value - delta;
            let max = value + delta;
            let min_idx = push_text(bind_params, min.to_string());
            let max_idx = push_text(bind_params, max.to_string());
            format!(
                "({} >= ${}::numeric AND {} <= ${}::numeric)",
                value_expr, min_idx, value_expr, max_idx
            )
        }
    };

    Some(clause)
}

pub(in crate::db::search::query_builder) fn build_quantity_json_clause(
    idx: usize,
    raw_value: &str,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let v = unescape_search_value(raw_value).ok()?;
    let (prefix, rest) = SearchPrefix::parse_prefix(v.as_str());
    let prefix = prefix.unwrap_or(SearchPrefix::Eq);
    let q = parse_quantity(rest).ok()?;
    let value_expr = format!("(sc.components->{}->>'value')::numeric", idx);

    let number = Decimal::from_str_exact(q.number.trim()).ok()?;
    let (min, max) = number_precision_range(q.number).ok()?;
    let mut clause = match prefix {
        SearchPrefix::Eq => {
            let min_idx = push_text(bind_params, min.to_string());
            let max_idx = push_text(bind_params, max.to_string());
            format!(
                "({} >= ${}::numeric AND {} < ${}::numeric)",
                value_expr, min_idx, value_expr, max_idx
            )
        }
        SearchPrefix::Ne => {
            let min_idx = push_text(bind_params, min.to_string());
            let max_idx = push_text(bind_params, max.to_string());
            format!(
                "({} < ${}::numeric OR {} >= ${}::numeric)",
                value_expr, min_idx, value_expr, max_idx
            )
        }
        SearchPrefix::Gt | SearchPrefix::Sa => {
            let max_idx = push_text(bind_params, max.to_string());
            format!("{} >= ${}::numeric", value_expr, max_idx)
        }
        SearchPrefix::Ge => {
            let min_idx = push_text(bind_params, min.to_string());
            format!("{} >= ${}::numeric", value_expr, min_idx)
        }
        SearchPrefix::Lt | SearchPrefix::Eb => {
            let min_idx = push_text(bind_params, min.to_string());
            format!("{} < ${}::numeric", value_expr, min_idx)
        }
        SearchPrefix::Le => {
            let max_idx = push_text(bind_params, max.to_string());
            format!("{} < ${}::numeric", value_expr, max_idx)
        }
        SearchPrefix::Ap => {
            let precision = decimal_precision(q.number).ok()?;
            let delta = (number.abs() / Decimal::new(10, 0)).max(precision);
            let min = number - delta;
            let max = number + delta;
            let min_idx = push_text(bind_params, min.to_string());
            let max_idx = push_text(bind_params, max.to_string());
            format!(
                "({} >= ${}::numeric AND {} <= ${}::numeric)",
                value_expr, min_idx, value_expr, max_idx
            )
        }
    };

    if let Some(code) = q.code {
        let code_idx = push_text(bind_params, code);
        clause.push_str(&format!(
            " AND sc.components->{}->>'code' = ${}",
            idx, code_idx
        ));
    }
    if let Some(system) = q.system {
        let sys_idx = push_text(bind_params, system);
        clause.push_str(&format!(
            " AND sc.components->{}->>'system' = ${}",
            idx, sys_idx
        ));
    }

    Some(format!("({})", clause))
}

struct ParsedQuantity<'a> {
    number: &'a str,
    system: Option<String>,
    code: Option<String>,
}

fn parse_quantity(raw: &str) -> Result<ParsedQuantity<'_>, ()> {
    // Supported formats:
    // - [number]
    // - [number]||[code]
    // - [number]|[system]|[code]
    if let Some((number, rest)) = raw.split_once("||") {
        return Ok(ParsedQuantity {
            number,
            system: None,
            code: Some(rest.to_string()),
        });
    }
    let parts: Vec<&str> = raw.split('|').collect();
    match parts.len() {
        1 => Ok(ParsedQuantity {
            number: parts[0],
            system: None,
            code: None,
        }),
        3 => Ok(ParsedQuantity {
            number: parts[0],
            system: Some(parts[1].to_string()).filter(|s| !s.is_empty()),
            code: Some(parts[2].to_string()).filter(|s| !s.is_empty()),
        }),
        _ => Err(()),
    }
}

fn number_precision_range(raw: &str) -> Result<(Decimal, Decimal), ()> {
    let value_str = raw.trim();
    let number = Decimal::from_str_exact(value_str).map_err(|_| ())?;
    let precision = decimal_precision(value_str)?;
    Ok((number - precision, number + precision))
}

fn decimal_precision(value_str: &str) -> Result<Decimal, ()> {
    let mut s = value_str.trim().to_string();
    if s.starts_with('+') || s.starts_with('-') {
        s.remove(0);
    }

    if let Some((coeff, exp)) = s.split_once(['e', 'E']) {
        // Exponential notation: best-effort precision based on significant digits in coefficient.
        let exp: i32 = exp.parse().map_err(|_| ())?;
        let coeff = coeff.trim_start_matches(['+', '-']);
        let digits = coeff.chars().filter(|c| c.is_ascii_digit()).count().max(1) as i32;
        // Least significant digit position:
        // - coefficient has `digits` significant digits
        // - shifting by `exp` moves the decimal point
        // unit = 10^(exp - (digits - 1))
        let unit = pow10(exp - (digits - 1))?;
        Ok(unit / Decimal::new(2, 0))
    } else if let Some((_int, frac)) = s.split_once('.') {
        let places = frac.len() as u32;
        Ok(Decimal::new(5, places + 1))
    } else {
        // Integer: precision at the units place (FHIR treats all digits as significant).
        Ok(Decimal::new(5, 1))
    }
}

fn pow10(power: i32) -> Result<Decimal, ()> {
    if power >= 0 {
        let mut result = Decimal::new(1, 0);
        for _ in 0..power {
            result *= Decimal::new(10, 0);
        }
        Ok(result)
    } else {
        let scale = (-power) as u32;
        Ok(Decimal::new(1, scale))
    }
}
