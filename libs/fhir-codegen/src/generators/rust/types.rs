//! Type generation for Rust structs

use crate::generators::GeneratorConfig;
use crate::ir::{Property, PropertyType, TypeDefinition, TypeKind, TypeRegistry};
use heck::ToSnakeCase;
use ferrum_models::common::structure_definition::StructureDefinitionKind;

/// Generate a Rust struct for a type definition
pub fn generate_struct(
    type_def: &TypeDefinition,
    registry: &TypeRegistry,
    config: &GeneratorConfig,
) -> String {
    let mut code = String::new();

    // Generate documentation
    if config.generate_docs {
        if let Some(desc) = &type_def.description {
            code.push_str(&format!("/// {}\n", desc));
        } else {
            code.push_str(&format!("/// {}\n", type_def.name));
        }

        if let Some(url) = &type_def.url {
            code.push_str(&format!("///\n/// Canonical URL: {}\n", url));
        }

        let sd_kind = structure_definition_kind(type_def.kind);
        code.push_str(&format!("/// Kind: {:?}\n", sd_kind));
    }

    // Generate derive macros
    code.push_str("#[derive(Debug, Clone, PartialEq");
    if config.generate_serde {
        code.push_str(", Serialize, Deserialize");
    }
    code.push_str(")]\n");

    // Add serde rename_all for camelCase
    if config.generate_serde {
        code.push_str("#[serde(rename_all = \"camelCase\")]\n");
    }

    // Struct definition
    code.push_str(&format!("pub struct {} {{\n", type_def.name));

    // Generate fields
    for property in &type_def.properties {
        code.push_str(&generate_field(property, registry, config));
    }

    code.push('}');

    code
}

fn structure_definition_kind(kind: TypeKind) -> StructureDefinitionKind {
    match kind {
        TypeKind::Resource => StructureDefinitionKind::Resource,
        TypeKind::ComplexType | TypeKind::BackboneElement => StructureDefinitionKind::ComplexType,
        TypeKind::PrimitiveType => StructureDefinitionKind::PrimitiveType,
    }
}

/// Generate a field for a property (public version)
pub fn generate_field_from_property(
    property: &Property,
    registry: &TypeRegistry,
    config: &GeneratorConfig,
) -> String {
    generate_field(property, registry, config)
}

/// Generate a field for a property
fn generate_field(
    property: &Property,
    registry: &TypeRegistry,
    config: &GeneratorConfig,
) -> String {
    let mut code = String::new();

    // Documentation
    if config.generate_docs {
        if let Some(desc) = &property.description {
            code.push_str(&format!("    /// {}\n", desc));
        }

        if property.is_modifier {
            code.push_str("    /// **Modifier element**\n");
        }

        if property.must_support {
            code.push_str("    /// **Must support**\n");
        }
    }

    // Serde attributes
    if config.generate_serde {
        // Handle optional fields
        if property.cardinality.is_optional() {
            code.push_str("    #[serde(skip_serializing_if = \"Option::is_none\")]\n");
        }

        // Handle special renames (e.g., 'type' is a Rust keyword)
        if is_rust_keyword(&property.name) {
            code.push_str(&format!("    #[serde(rename = \"{}\")]\n", property.name));
        }
    }

    // Field name (convert to snake_case and handle keywords)
    let field_name = sanitize_field_name(&property.name);

    // Field type
    let field_type = generate_field_type(property, registry);

    code.push_str(&format!("    pub {}: {},\n", field_name, field_type));

    code
}

/// Generate the Rust type for a property
fn generate_field_type(property: &Property, registry: &TypeRegistry) -> String {
    // Handle multiple types (use an enum or Box<dyn> in practice, simplified here)
    let base_type = if property.types.is_empty() {
        "serde_json::Value".to_string()
    } else if property.types.len() == 1 {
        map_fhir_type_to_rust(&property.types[0], registry)
    } else {
        // Multiple types - could generate an enum, but for now use Value
        "serde_json::Value".to_string()
    };

    // Wrap in Vec if array
    let base_type = if property.cardinality.is_array() {
        format!("Vec<{}>", base_type)
    } else {
        base_type
    };

    // Wrap in Option if optional
    if property.cardinality.is_optional() {
        format!("Option<{}>", base_type)
    } else {
        base_type
    }
}

/// Map a FHIR type to a Rust type
fn map_fhir_type_to_rust(property_type: &PropertyType, registry: &TypeRegistry) -> String {
    match property_type.code.as_str() {
        // FHIR primitives to Rust primitives
        "boolean" => "bool".to_string(),
        "integer" | "unsignedInt" | "positiveInt" => "i32".to_string(),
        "integer64" => "i64".to_string(),
        "decimal" => "f64".to_string(),
        "string" | "code" | "id" | "markdown" | "uri" | "url" | "canonical" | "oid" | "uuid" => {
            "String".to_string()
        }
        "date" | "dateTime" | "instant" | "time" => "String".to_string(), // Or use chrono types
        "base64Binary" => "String".to_string(),

        // Complex types - reference by name
        "Reference" => "Reference".to_string(),
        "Quantity" => "Quantity".to_string(),
        "CodeableConcept" => "CodeableConcept".to_string(),
        "Coding" => "Coding".to_string(),
        "Period" => "Period".to_string(),
        "Range" => "Range".to_string(),
        "Ratio" => "Ratio".to_string(),
        "Identifier" => "Identifier".to_string(),
        "HumanName" => "HumanName".to_string(),
        "Address" => "Address".to_string(),
        "ContactPoint" => "ContactPoint".to_string(),
        "Attachment" => "Attachment".to_string(),
        "Annotation" => "Annotation".to_string(),
        "Timing" => "Timing".to_string(),
        "Signature" => "Signature".to_string(),
        "SampledData" => "SampledData".to_string(),

        // BackboneElement or nested types -> treat as complex structs
        "BackboneElement" => "BackboneElement".to_string(),
        "Element" => "Element".to_string(),

        // Extension
        "Extension" => "Extension".to_string(),

        // Resource reference
        "Resource" => "serde_json::Value".to_string(),

        // Check if it's a known type in the registry
        other => {
            if registry.get_type_by_name(other).is_some() {
                other.to_string()
            } else {
                // Unknown type, use Value as fallback
                "serde_json::Value".to_string()
            }
        }
    }
}

/// Sanitize a field name to be a valid Rust identifier
fn sanitize_field_name(name: &str) -> String {
    let snake = name.to_snake_case();

    if is_rust_keyword(&snake) {
        format!("r#{}", snake)
    } else {
        snake
    }
}

/// Check if a string is a Rust keyword
fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}
