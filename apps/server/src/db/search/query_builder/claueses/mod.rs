//! Search clause builders organized by parameter type.
//!
//! Each module contains clause building logic for a specific FHIR search parameter type.

// Declare submodules
mod composite;
mod date;
mod fulltext_query;
mod number;
mod reference;
mod reverse_chain;
mod special;
mod string;
mod token;
mod uri;

// Re-export public APIs from special (main entry point)
pub(in crate::db::search::query_builder) use special::build_param_clause;
pub(crate) use special::build_param_clause_for_resource;

// Re-export public APIs from composite
pub(crate) use composite::{parse_composite_tuple, validate_composite_component_value};

// Re-export public APIs from reference
pub(crate) use reference::{parse_reference_query_value, ParsedReferenceQuery};

// Re-export date parsing helper used by _filter (po operator).
pub(crate) use date::fhir_date_range;
