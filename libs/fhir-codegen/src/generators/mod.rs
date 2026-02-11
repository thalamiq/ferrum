//! Code generators for different programming languages
//!
//! Each language has its own module that implements the `Generator` trait.

pub mod rust;

use crate::ir::TypeRegistry;
use anyhow::Result;

/// Trait that all language generators must implement
pub trait Generator {
    /// The output type of this generator
    type Output;

    /// Generate code from the type registry
    fn generate(&self, registry: &TypeRegistry) -> Result<Self::Output>;
}

/// Configuration options for code generation
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Whether to generate documentation comments
    pub generate_docs: bool,
    /// Whether to generate serde derive macros (for serialization)
    pub generate_serde: bool,
    /// Custom module path prefix
    pub module_prefix: Option<String>,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            generate_docs: true,
            generate_serde: true,
            module_prefix: None,
        }
    }
}
