//! FHIR data models
//!
//! This crate provides strongly-typed Rust structures for FHIR resources.
//!
//! # Module Organization
//!
//! - `common`: Version-agnostic models that work across FHIR R4, R4B, and R5
//! - Future: `r4`, `r5` modules for version-specific models
//!
//! # Design Philosophy
//!
//! - **Version-agnostic core**: Common fields present across all FHIR versions
//! - **Extensible**: `extensions` field captures version-specific or custom properties
//! - **Strongly-typed**: Type safety for common operations
//! - **Flexible**: Can serialize/deserialize to/from JSON
//! - **Compatible**: Works with existing `serde_json::Value`-based code
//!
//! # Example
//!
//! ```rust
//! use ferrum_models::common::{StructureDefinition, StructureDefinitionKind};
//! use serde_json::json;
//!
//! let sd_json = json!({
//!     "resourceType": "StructureDefinition",
//!     "id": "Patient",
//!     "url": "http://hl7.org/fhir/StructureDefinition/Patient",
//!     "version": "4.0.1",
//!     "name": "Patient",
//!     "status": "active",
//!     "kind": "resource",
//!     "abstract": false,
//!     "type": "Patient"
//! });
//!
//! let sd: StructureDefinition = serde_json::from_value(sd_json).unwrap();
//! assert_eq!(sd.name, "Patient");
//! assert_eq!(sd.kind, StructureDefinitionKind::Resource);
//! ```

pub mod common;

// Re-export commonly used types
pub use common::*;
