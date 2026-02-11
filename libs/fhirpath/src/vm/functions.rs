//! Function implementations for FHIRPath VM
//!
//! This module re-exports the function dispatcher from the organized submodules.
//! All function implementations are organized by category in submodules.

mod aggregate;
mod boolean;
mod combining;
mod conversion;
mod existence;
mod filtering;
mod helpers;
mod math;
mod navigation;
mod string;
mod subsetting;
mod temporal;
mod type_helpers;
mod type_op;
mod utility;

// Re-export public API
pub use aggregate::{aggregate, aggregate_with_subplans};
pub use boolean::{as_type, not};
pub use combining::{combine, union_func};
pub use conversion::{
    converts_to_boolean, converts_to_date, converts_to_datetime, converts_to_decimal,
    converts_to_integer, converts_to_quantity, converts_to_string, converts_to_time, iif,
    to_boolean, to_date, to_datetime, to_decimal, to_integer, to_quantity, to_time,
};
pub use existence::{
    all, all_false, all_true, any_false, any_true, count, distinct, empty, exists, is_distinct,
    subset_of, superset_of,
};
pub use filtering::{extension, of_type, repeat, select_func, where_func};
pub use math::{abs, ceiling, exp, floor, ln, log, power, round, sqrt, truncate};
pub use navigation::{children, descendants};
pub use string::{
    contains_str, decode, encode, ends_with, escape, index_of, join, last_index_of, length, lower,
    matches, matches_full, replace, replace_matches, split, starts_with, substring, to_chars,
    to_string, trim, unescape, upper,
};
pub use subsetting::{exclude, first, intersect, last, single, skip, tail, take};
pub(crate) use type_helpers::validate_type_specifier;
pub use type_helpers::{matches_type_specifier, matches_type_specifier_exact};
pub use type_op::is_type;
pub use utility::{
    comparable, conforms_to, has_value, high_boundary, low_boundary, now, precision, resolve, sort,
    time_of_day, today, trace, type_function,
};

// Main dispatcher
use crate::context::Context;
use crate::error::{Error, Result};
use crate::hir::FunctionId;
use crate::resolver::ResourceResolver;
use crate::value::Collection;
use std::sync::Arc;
use zunder_context::FhirContext;

/// Execute a function call by dispatching to the appropriate implementation.
///
/// This is the main entry point for all FHIRPath function execution. Functions are
/// identified by their numeric ID and routed to the appropriate implementation module.
pub fn execute_function(
    func_id: FunctionId,
    collection: Collection,
    args: Vec<Collection>,
    ctx: &Context,
    path_hint: Option<&str>,
    fhir_context: Option<&dyn FhirContext>,
    resource_resolver: Option<&Arc<dyn ResourceResolver>>,
) -> Result<Collection> {
    match func_id {
        // Boolean logic functions
        0 => not(collection),
        1 => as_type(collection, args.first(), path_hint, fhir_context, ctx),

        // Existence functions
        10 => empty(collection),
        11 => exists(collection, args.first()),
        12 => all(collection, args.first()),
        13 => all_true(collection),
        14 => any_true(collection),
        15 => all_false(collection),
        16 => any_false(collection),
        17 => subset_of(collection, args.first()),
        18 => superset_of(collection, args.first()),
        19 => count(collection),
        20 => distinct(collection),
        21 => is_distinct(collection),

        // Filtering functions
        30 => where_func(collection, args.first()),
        31 => select_func(collection, args.first()),
        32 => repeat(collection, args.first()),
        33 => of_type(collection, args.first(), path_hint, fhir_context, ctx),
        34 => extension(collection, args.first(), path_hint, ctx),

        // Subsetting functions
        40 => single(collection),
        41 => first(collection),
        42 => last(collection),
        43 => tail(collection),
        44 => skip(collection, args.first()),
        45 => take(collection, args.first()),
        46 => intersect(collection, args.first()),
        47 => exclude(collection, args.first()),

        // Combining functions
        50 => union_func(collection, args.first()),
        51 => combine(collection, args.first()),

        // String functions
        100 => to_string(collection),
        101 => index_of(collection, args.first()),
        102 => last_index_of(collection, args.first()),
        103 => substring(collection, args.first(), args.get(1)),
        104 => starts_with(collection, args.first()),
        105 => ends_with(collection, args.first()),
        106 => contains_str(collection, args.first()),
        107 => upper(collection),
        108 => lower(collection),
        109 => replace(collection, args.first(), args.get(1)),
        110 => matches(collection, args.first()),
        111 => matches_full(collection, args.first()),
        112 => replace_matches(collection, args.first(), args.get(1)),
        113 => length(collection),
        114 => to_chars(collection),
        115 => trim(collection),
        116 => encode(collection, args.first()),
        117 => decode(collection, args.first()),
        118 => escape(collection, args.first()),
        119 => unescape(collection, args.first()),
        120 => split(collection, args.first()),
        121 => join(collection, args.first()),

        // Math functions
        200 => abs(collection),
        201 => ceiling(collection),
        202 => exp(collection),
        203 => floor(collection),
        204 => ln(collection),
        205 => log(collection, args.first()),
        206 => power(collection, args.first()),
        207 => round(collection, args.first()),
        208 => sqrt(collection),
        209 => truncate(collection),

        // Conversion functions
        300 => iif(collection, args.first(), args.get(1), args.get(2)),
        301 => to_boolean(collection),
        302 => converts_to_boolean(collection),
        303 => to_integer(collection),
        304 => converts_to_integer(collection),
        305 => to_decimal(collection),
        306 => converts_to_decimal(collection),
        307 => converts_to_string(collection),
        308 => to_date(collection),
        309 => converts_to_date(collection),
        310 => to_datetime(collection),
        311 => converts_to_datetime(collection),
        312 => to_time(collection),
        313 => converts_to_time(collection),
        314 => to_quantity(collection),
        315 => converts_to_quantity(collection),

        // Navigation functions
        400 => {
            if ctx.strict && args.is_empty() {
                return Err(Error::InvalidOperation(
                    "children() ordering is undefined in strict mode".into(),
                ));
            }
            children(collection, args.first())
        }
        401 => descendants(collection, args.first()),

        // Type functions
        410 => is_type(collection, args.first(), path_hint, fhir_context, ctx),

        // Utility functions
        500 => trace(collection, args.first(), args.get(1)),
        501 => now(),
        502 => today(),
        503 => time_of_day(),
        504 => sort(collection, args.first()),
        505 => low_boundary(collection, args.first()),
        506 => high_boundary(collection, args.first()),
        507 => comparable(collection, args.first()),
        508 => precision(collection),
        509 => type_function(collection, path_hint, fhir_context, ctx),
        510 => conforms_to(collection, args.first(), ctx),
        511 => has_value(collection),
        512 => resolve(collection, ctx, resource_resolver),

        // Aggregate functions
        600 => aggregate(collection, args.first(), args.get(1)),

        _ => Err(Error::FunctionNotFound(format!(
            "Function ID {} not found",
            func_id
        ))),
    }
}
