//! Version-agnostic FHIR models
//!
//! Types that work across FHIR R4, R4B, and R5

pub mod bundle;
pub mod code_system;
pub mod complex;
pub mod element_definition;
pub mod error;
pub mod structure_definition;
pub mod value_set;

// Re-export commonly used types
pub use bundle::*;
pub use code_system::*;
pub use complex::*;
pub use element_definition::*;
pub use error::{Error, Result};
pub use structure_definition::*;
pub use value_set::*;
