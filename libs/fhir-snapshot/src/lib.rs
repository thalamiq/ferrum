//! FHIR StructureDefinition Snapshot Expansion
//!
//! This crate provides functionality to expand StructureDefinition snapshots
//! by resolving complex types, choice types, and contentReferences.
//!
//! # Example
//!
//! ```rust,no_run
//! use ferrum_snapshot::{SnapshotExpander, generate_snapshot, generate_differential, generate_deep_snapshot};
//! use ferrum_context::{DefaultFhirContext, FhirContext};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load a package (from registry-client)
//! // let package = FhirPackage::from_directory(...)?;
//! // let ctx = DefaultFhirContext::new(package);
//!
//! // Generate snapshot from differential
//! // let snapshot = generate_snapshot(&base_snapshot, &differential)?;
//!
//! // Generate differential from snapshot
//! // let differential = generate_differential(&base_snapshot, &snapshot)?;
//!
//! // Expand snapshot (deep expansion)
//! // let expander = SnapshotExpander::new();
//! // let expanded = expander.expand_snapshot(snapshot, &ctx)?;
//! // Or use the convenience function:
//! // let deep_snapshot = generate_deep_snapshot(&snapshot, &ctx)?;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod expanded_context;
pub mod expander;
pub mod generator;
pub mod inheritance;
pub mod merge;
pub mod normalization;
pub mod slicing;
pub mod snapshot_generation;
pub mod validation;

pub use error::{Error, Result};
pub use expanded_context::{BorrowedFhirContext, ExpandedFhirContext};
pub use expander::SnapshotExpander;
pub use generator::{generate_deep_snapshot, generate_differential, generate_snapshot};
pub use snapshot_generation::{
    generate_structure_definition_differential, generate_structure_definition_snapshot,
};
pub use ferrum_models::{Differential, ElementDefinition, ElementDefinitionType, Snapshot};

// Re-export validation for CLI and external use
pub use validation::{validate_differential, validate_snapshot};
