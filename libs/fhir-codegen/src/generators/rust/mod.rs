//! Rust code generator for FHIR types

mod types;

use crate::generators::{Generator, GeneratorConfig};
use crate::ir::{TypeDefinition, TypeRegistry};
use anyhow::Result;
use heck::ToSnakeCase;
use std::collections::HashMap;

/// Output of the Rust generator
#[derive(Debug)]
pub struct RustOutput {
    /// Generated modules indexed by module name
    pub modules: HashMap<String, String>,
}

/// Rust code generator
pub struct RustGenerator {
    config: GeneratorConfig,
}

impl RustGenerator {
    pub fn new(config: GeneratorConfig) -> Self {
        Self { config }
    }

    pub fn new_default() -> Self {
        Self::new(GeneratorConfig::default())
    }
}

impl Generator for RustGenerator {
    type Output = RustOutput;

    fn generate(&self, registry: &TypeRegistry) -> Result<Self::Output> {
        let mut modules = HashMap::new();

        // Generate primitives module (keep primitives together)
        let primitives_code = self.generate_primitives_module(registry);
        modules.insert("primitives.rs".to_string(), primitives_code);

        // Generate one file per complex type
        for type_def in registry.complex_types() {
            let file_name = self.get_module_name(&type_def.name);
            let code = self.generate_type_module(type_def, registry);
            modules.insert(file_name, code);
        }

        // Generate one file per resource
        for type_def in registry.resource_types() {
            if !type_def.is_abstract {
                let file_name = self.get_module_name(&type_def.name);
                let code = self.generate_type_module(type_def, registry);
                modules.insert(file_name, code);
            }
        }

        // Generate mod.rs that exports all modules
        let mod_rs = self.generate_mod_rs(registry);
        modules.insert("mod.rs".to_string(), mod_rs);

        Ok(RustOutput { modules })
    }
}

impl RustGenerator {
    /// Convert a type name to a module name (snake_case)
    fn get_module_name(&self, type_name: &str) -> String {
        format!("{}.rs", type_name.to_snake_case())
    }

    /// Generate a complete module for a single type
    fn generate_type_module(&self, type_def: &TypeDefinition, registry: &TypeRegistry) -> String {
        let mut code = String::new();

        // Header comment
        code.push_str(&format!("//! {} type definition\n", type_def.name));
        if let Some(url) = &type_def.url {
            code.push_str(&format!("//! Canonical URL: {}\n", url));
        }
        code.push('\n');

        // Imports
        code.push_str(&self.generate_imports(type_def, registry));
        code.push('\n');

        // Main type definition
        code.push_str(&types::generate_struct(type_def, registry, &self.config));

        // Backbone elements (if any)
        if !type_def.backbone_elements.is_empty() {
            code.push_str("\n\n");
            code.push_str(&self.generate_backbone_elements(type_def, registry));
        }

        code
    }

    /// Generate imports for a type based on its dependencies
    fn generate_imports(&self, type_def: &TypeDefinition, registry: &TypeRegistry) -> String {
        let mut code = String::new();

        // Always import serde if enabled
        if self.config.generate_serde {
            code.push_str("use serde::{Deserialize, Serialize};\n");
        }

        // Get dependencies
        let deps = registry.get_dependencies(type_def);

        // Determine which modules to import from
        let mut needs_primitives = false;
        let mut complex_deps = Vec::new();

        for dep in &deps {
            if let Some(dep_type) = registry.get_type_by_name(dep) {
                match dep_type.kind {
                    crate::ir::TypeKind::PrimitiveType => {
                        needs_primitives = true;
                    }
                    crate::ir::TypeKind::ComplexType | crate::ir::TypeKind::Resource => {
                        complex_deps.push(dep.clone());
                    }
                    crate::ir::TypeKind::BackboneElement => {
                        // Backbone elements are in the parent's module, skip
                    }
                }
            }
        }

        // Import primitives if needed
        if needs_primitives {
            code.push_str("use super::primitives::*;\n");
        }

        // Import each complex dependency from its own module
        for dep in complex_deps {
            let module_name = dep.to_snake_case();
            code.push_str(&format!("use super::{}::{};\n", module_name, dep));
        }

        code
    }

    /// Generate backbone element structs
    fn generate_backbone_elements(
        &self,
        type_def: &TypeDefinition,
        registry: &TypeRegistry,
    ) -> String {
        let mut code = String::new();

        for (i, backbone) in type_def.backbone_elements.iter().enumerate() {
            if i > 0 {
                code.push_str("\n\n");
            }

            // Generate documentation
            if self.config.generate_docs {
                if let Some(desc) = &backbone.description {
                    code.push_str(&format!("/// {}\n", desc));
                } else {
                    code.push_str(&format!("/// {}\n", backbone.name));
                }
                code.push_str(&format!(
                    "///\n/// Backbone element for {}\n",
                    backbone.path
                ));
            }

            // Generate derive macros
            code.push_str("#[derive(Debug, Clone, PartialEq");
            if self.config.generate_serde {
                code.push_str(", Serialize, Deserialize");
            }
            code.push_str(")]\n");

            // Add serde rename_all for camelCase
            if self.config.generate_serde {
                code.push_str("#[serde(rename_all = \"camelCase\")]\n");
            }

            // Struct definition
            code.push_str(&format!("pub struct {} {{\n", backbone.name));

            // Generate fields
            for property in &backbone.properties {
                code.push_str(&types::generate_field_from_property(
                    property,
                    registry,
                    &self.config,
                ));
            }

            code.push('}');
        }

        code
    }

    fn generate_primitives_module(&self, registry: &TypeRegistry) -> String {
        let mut code = String::new();

        code.push_str("//! FHIR Primitive Types\n\n");

        if self.config.generate_serde {
            code.push_str("use serde::{Deserialize, Serialize};\n\n");
        }

        for type_def in registry.primitive_types() {
            code.push_str(&types::generate_struct(type_def, registry, &self.config));
            code.push_str("\n\n");
        }

        code
    }

    fn generate_mod_rs(&self, registry: &TypeRegistry) -> String {
        let mut code = String::new();

        code.push_str("//! Generated FHIR data models\n\n");

        // Declare primitives module
        code.push_str("pub mod primitives;\n");

        // Declare all complex type modules
        let mut complex_types: Vec<_> = registry.complex_types().collect();
        complex_types.sort_by(|a, b| a.name.cmp(&b.name));

        for type_def in complex_types {
            let module_name = type_def.name.to_snake_case();
            code.push_str(&format!("pub mod {};\n", module_name));
        }

        // Declare all resource modules
        let mut resources: Vec<_> = registry
            .resource_types()
            .filter(|t| !t.is_abstract)
            .collect();
        resources.sort_by(|a, b| a.name.cmp(&b.name));

        for type_def in resources {
            let module_name = type_def.name.to_snake_case();
            code.push_str(&format!("pub mod {};\n", module_name));
        }

        code.push_str("\n// Re-export all types\n");
        code.push_str("pub use primitives::*;\n");

        for type_def in registry.complex_types() {
            let module_name = type_def.name.to_snake_case();
            code.push_str(&format!("pub use {}::*;\n", module_name));
        }

        for type_def in registry.resource_types() {
            if !type_def.is_abstract {
                let module_name = type_def.name.to_snake_case();
                code.push_str(&format!("pub use {}::*;\n", module_name));
            }
        }

        code
    }
}
