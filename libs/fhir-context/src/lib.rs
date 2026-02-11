//! FHIR Context for runtime StructureDefinition access
//!
//! Provides a trait-based interface for accessing FHIR conformance resources
//! during FHIRPath HIR generation, similar to the Python implementation.

pub mod context;
pub mod error;
pub mod loader;
pub mod version;

pub use context::{
    ConformanceResourceProvider, DefaultFhirContext, FallbackConformanceProvider, FhirContext,
    FlexibleFhirContext, LockedPackage, PackageIntrospection, PackageLock,
};
pub use error::{Error, Result};
pub use loader::PackageLoader;
