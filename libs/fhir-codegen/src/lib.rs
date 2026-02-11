//! FHIR Code Generator
//!
//! This library provides a multi-language code generator for FHIR data models.
//! It reads FHIR packages and generates strongly-typed code for various programming languages.
//!
//! ## Architecture
//!
//! The generator uses a three-stage pipeline:
//! 1. **Parser**: Extracts type information from FHIR StructureDefinitions
//! 2. **IR (Intermediate Representation)**: Language-agnostic type model
//! 3. **Generators**: Language-specific code generation from IR
//!
//! This architecture allows adding new target languages without re-parsing FHIR packages.

pub mod generators;
pub mod ir;
pub mod parser;
pub mod utils;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use generators::rust::RustGenerator;
use generators::GeneratorConfig;
use ir::TypeRegistry;
use zunder_context::DefaultFhirContext;
use zunder_package::FhirPackage;

/// Main entry point for code generation
pub struct CodeGenerator {
    registry: TypeRegistry,
}

impl CodeGenerator {
    /// Create a new code generator from a FHIR package
    pub fn from_package(package: FhirPackage) -> Result<Self> {
        let registry = parser::parse_package(package)?;
        Ok(Self { registry })
    }

    /// Create a new code generator from a FHIR context
    pub fn from_context(context: &DefaultFhirContext) -> Result<Self> {
        let registry = parser::parse_context(context)?;
        Ok(Self { registry })
    }

    /// Get the type registry
    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    /// Generate code for a specific language
    pub fn generate<G: generators::Generator>(&self, generator: G) -> Result<G::Output> {
        generator.generate(&self.registry)
    }
}

/// Convenience helper to run the Rust code generator from a package archive.
///
/// Returns the number of generated modules.
pub fn generate_rust_from_package(
    package_path: &Path,
    output_dir: &Path,
    config: GeneratorConfig,
) -> Result<usize> {
    let package_bytes = fs::read(package_path)
        .with_context(|| format!("reading package {}", package_path.display()))?;
    let package = FhirPackage::from_tar_gz_bytes(&package_bytes).context("loading FHIR package")?;

    let codegen = CodeGenerator::from_package(package).context("building type registry")?;

    let generator = RustGenerator::new(config);
    let output = codegen
        .generate(generator)
        .context("running Rust generator")?;

    utils::write_modules(output_dir, &output.modules)?;

    Ok(output.modules.len())
}

/// Convenience helper to run the Rust code generator from a loaded FHIR context.
///
/// Returns the number of generated modules.
pub fn generate_rust_from_context(
    context: &DefaultFhirContext,
    output_dir: &Path,
    config: GeneratorConfig,
) -> Result<usize> {
    let codegen = CodeGenerator::from_context(context).context("building type registry")?;

    let generator = RustGenerator::new(config);
    let output = codegen
        .generate(generator)
        .context("running Rust generator")?;

    utils::write_modules(output_dir, &output.modules)?;

    Ok(output.modules.len())
}
