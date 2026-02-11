use crate::error::{Error, Result};
use rust_decimal::Decimal;
use std::cmp::Ordering;

pub const UCUM_SYSTEM: &str = "http://unitsofmeasure.org";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FhirQuantity<'a> {
    pub value: Decimal,
    pub system: Option<&'a str>,
    pub code: Option<&'a str>,
    pub unit: Option<&'a str>,
}

impl<'a> FhirQuantity<'a> {
    pub fn semantics_code(&self) -> Option<&'a str> {
        match self.system {
            Some(UCUM_SYSTEM) => self.code,
            _ => None,
        }
    }
}

pub fn compare(lhs: FhirQuantity<'_>, rhs: FhirQuantity<'_>) -> Result<Ordering> {
    let lc = lhs
        .semantics_code()
        .ok_or_else(|| Error::Db("lhs is not UCUM".into()))?;
    let rc = rhs
        .semantics_code()
        .ok_or_else(|| Error::Db("rhs is not UCUM".into()))?;
    crate::unit::compare_decimal_quantities(&lhs.value, lc, &rhs.value, rc)
}
