#![forbid(unsafe_code)]

mod ast;
mod db;
mod error;
mod parser;
mod quantity;
mod unit;

#[cfg(feature = "ucum-fhir")]
pub mod fhir;

use once_cell::sync::Lazy;

pub use ast::{Atom, Term, UnitExpr};
pub use error::{Error, Result};
pub use parser::{parse, validate};
pub use quantity::{normalize, NormalizedQuantity, Quantity};
pub use unit::{
    compare_decimal_quantities, convert_decimal, convertible, equivalent, DimensionVector, Unit,
    UnitKind,
};

static UCUM_DB: Lazy<db::UcumDb> = Lazy::new(|| {
    db::UcumDb::from_essence_xml(include_str!("../ucum-essence.xml"))
        .expect("failed to load embedded ucum-essence.xml")
});

pub(crate) fn db() -> &'static db::UcumDb {
    &UCUM_DB
}
