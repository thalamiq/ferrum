use crate::error::{Error, Result};
use crate::unit::{DimensionVector, Unit, UnitKind};
use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quantity {
    pub value: Decimal,
    pub unit: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedQuantity {
    pub value: Decimal,
    pub unit: String,
}

pub fn normalize(value: Decimal, unit: &str) -> Result<NormalizedQuantity> {
    let u = Unit::parse(unit)?;
    match &u.kind {
        UnitKind::NonLinear => Err(Error::NonLinear(unit.into())),
        UnitKind::Affine { .. } => normalize_to("K", &u, value),
        UnitKind::Multiplicative { .. } => normalize_to_best(&u, value),
    }
}

fn normalize_to(target_unit: &str, from: &Unit, value: Decimal) -> Result<NormalizedQuantity> {
    let from_value = crate::unit::decimal_to_rational(value)?;
    let base = from.to_base(&from_value)?;
    let to = Unit::parse(target_unit)?;
    let out = to.from_base(&base)?;
    let out_decimal = crate::unit::rational_to_decimal(out)?;
    Ok(NormalizedQuantity {
        value: out_decimal,
        unit: target_unit.into(),
    })
}

fn normalize_to_best(from: &Unit, value: Decimal) -> Result<NormalizedQuantity> {
    let from_value = crate::unit::decimal_to_rational(value)?;
    let base = from.to_base(&from_value)?;

    if let Some(target) = best_named_unit_for_dimension(from.dimensions) {
        let to = Unit::parse(&target)?;
        let out = to.from_base(&base)?;
        let out_decimal = crate::unit::rational_to_decimal(out)?;
        return Ok(NormalizedQuantity {
            value: out_decimal,
            unit: target,
        });
    }

    let out_decimal = crate::unit::rational_to_decimal(base)?;
    Ok(NormalizedQuantity {
        value: out_decimal,
        unit: render_base_expr(from.dimensions),
    })
}

fn best_named_unit_for_dimension(dim: DimensionVector) -> Option<String> {
    static CANON: Lazy<HashMap<DimensionVector, String>> = Lazy::new(build_canon_map);
    CANON.get(&dim).cloned()
}

fn build_canon_map() -> HashMap<DimensionVector, String> {
    let mut map: HashMap<DimensionVector, (u32, String)> = HashMap::new();
    let db = crate::db();

    for (code, def) in &db.units {
        // Don't canonicalize bracketed / special units.
        if def.is_special || def.is_arbitrary || code.starts_with('[') {
            continue;
        }
        let Ok(u) = Unit::parse(code) else { continue };
        let UnitKind::Multiplicative { factor: _ } = u.kind else {
            continue;
        };
        if u.dimensions == DimensionVector::ZERO {
            continue;
        }

        let rank = rank(def, code);
        let key = u.dimensions;
        match map.get(&key) {
            Some((cur_rank, _)) if *cur_rank <= rank => {}
            _ => {
                map.insert(key, (rank, code.clone()));
            }
        }
    }

    map.into_iter().map(|(k, (_, v))| (k, v)).collect()
}

fn rank(def: &crate::db::UnitDef, code: &str) -> u32 {
    // Lower is better.
    let mut score = 0u32;
    if def.class.as_deref() == Some("si") {
        score += 0;
    } else {
        score += 1000;
    }
    if def.is_metric {
        score += 0;
    } else {
        score += 100;
    }
    score += code.len() as u32;
    score
}

fn render_base_expr(dim: DimensionVector) -> String {
    let mut out = String::new();
    let parts = [
        ("g", dim.0[1]),
        ("mol", dim.0[7]),
        ("m", dim.0[0]),
        ("s", dim.0[2]),
        ("K", dim.0[4]),
        ("C", dim.0[5]),
        ("rad", dim.0[3]),
        ("cd", dim.0[6]),
    ];
    for (sym, exp) in parts {
        if exp == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(sym);
        if exp != 1 {
            out.push_str(&exp.to_string());
        }
    }
    if out.is_empty() {
        out.push('1');
    }
    out
}
