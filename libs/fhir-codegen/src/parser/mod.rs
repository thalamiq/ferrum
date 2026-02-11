//! Parser for FHIR StructureDefinitions
//!
//! Extracts type information from FHIR StructureDefinitions and builds
//! an intermediate representation (IR) suitable for code generation.

use crate::ir::{
    BackboneElement, Cardinality, Property, PropertyType, TypeDefinition, TypeKind, TypeRegistry,
};
use anyhow::{anyhow, Result};
use serde_json::Value;
use zunder_context::DefaultFhirContext;
use zunder_package::FhirPackage;

/// Parse a FHIR package and extract all type definitions
pub fn parse_package(package: FhirPackage) -> Result<TypeRegistry> {
    let mut registry = TypeRegistry::new();

    // Get all StructureDefinition resources
    let (conformance_resources, _examples) = package.all_resources();

    for resource in conformance_resources {
        if let Some("StructureDefinition") = resource.get("resourceType").and_then(|v| v.as_str()) {
            if let Ok(type_def) = parse_structure_definition(resource) {
                let id = type_def
                    .url
                    .clone()
                    .unwrap_or_else(|| type_def.name.clone());
                registry.add_type(id, type_def);
            }
        }
    }

    Ok(registry)
}

/// Parse a FHIR context and extract all type definitions
pub fn parse_context(context: &DefaultFhirContext) -> Result<TypeRegistry> {
    let mut registry = TypeRegistry::new();

    for sd in context.all_structure_definitions() {
        if let Ok(type_def) = parse_structure_definition(&sd) {
            let id = type_def
                .url
                .clone()
                .unwrap_or_else(|| type_def.name.clone());
            registry.add_type(id, type_def);
        }
    }

    Ok(registry)
}

/// Parse a single StructureDefinition into a TypeDefinition
fn parse_structure_definition(sd: &Value) -> Result<TypeDefinition> {
    let name = sd
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("StructureDefinition missing 'name'"))?
        .to_string();

    let url = sd.get("url").and_then(|v| v.as_str()).map(String::from);

    let description = sd
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Determine the kind
    let kind_str = sd.get("kind").and_then(|v| v.as_str());
    let derivation = sd.get("derivation").and_then(|v| v.as_str());

    let kind = match (kind_str, derivation) {
        (Some("resource"), _) => TypeKind::Resource,
        (Some("complex-type"), _) => TypeKind::ComplexType,
        (Some("primitive-type"), _) => TypeKind::PrimitiveType,
        _ => {
            // Check if it's a backbone element by looking at the type field
            let type_field = sd.get("type").and_then(|v| v.as_str());
            if type_field == Some("BackboneElement") || type_field == Some("Element") {
                TypeKind::BackboneElement
            } else {
                TypeKind::ComplexType
            }
        }
    };

    let is_abstract = sd
        .get("abstract")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let base_type = sd
        .get("baseDefinition")
        .and_then(|v| v.as_str())
        .map(extract_type_name_from_url);

    // Parse elements from the snapshot
    let (properties, backbone_elements) = if let Some(snapshot) = sd.get("snapshot") {
        parse_elements(snapshot, &name)?
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(TypeDefinition {
        name,
        url,
        description,
        kind,
        base_type,
        properties,
        is_abstract,
        backbone_elements,
        parent_type: None,
    })
}

/// Parse elements from a snapshot into properties and backbone elements
fn parse_elements(
    snapshot: &Value,
    type_name: &str,
) -> Result<(Vec<Property>, Vec<BackboneElement>)> {
    let elements = snapshot
        .get("element")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Snapshot missing 'element' array"))?;

    let mut properties = Vec::new();
    let mut backbone_elements = Vec::new();

    // Group elements by their depth to identify backbone elements
    let expected_prefix = format!("{}.", type_name);

    // First pass: identify direct properties and backbone element roots
    let mut backbone_roots = std::collections::HashMap::new();

    for element in elements.iter().skip(1) {
        let path = element.get("path").and_then(|v| v.as_str()).unwrap_or("");

        if !path.starts_with(&expected_prefix) {
            continue;
        }

        let remainder = &path[expected_prefix.len()..];
        let parts: Vec<&str> = remainder.split('.').collect();

        if parts.len() == 1 {
            // Direct property of this type
            if let Ok(property) = parse_element(element, type_name) {
                properties.push(property);
            }
        } else if parts.len() > 1 {
            // Part of a backbone element
            let backbone_name = parts[0];
            backbone_roots
                .entry(backbone_name.to_string())
                .or_insert_with(Vec::new)
                .push(element);
        }
    }

    // Second pass: parse backbone elements
    for (backbone_name, elements) in backbone_roots {
        if let Ok(backbone) = parse_backbone_element(&backbone_name, &elements, type_name) {
            backbone_elements.push(backbone);
        }
    }

    Ok((properties, backbone_elements))
}

/// Parse a backbone element from its elements
fn parse_backbone_element(
    name: &str,
    elements: &[&Value],
    parent_type: &str,
) -> Result<BackboneElement> {
    let full_path = format!("{}.{}", parent_type, name);
    let mut properties = Vec::new();
    let mut description = None;

    let expected_prefix = format!("{}.", full_path);

    // Find the root element to get description
    for element in elements.iter() {
        let path = element.get("path").and_then(|v| v.as_str()).unwrap_or("");

        if path == full_path {
            description = element
                .get("short")
                .and_then(|v| v.as_str())
                .or_else(|| element.get("definition").and_then(|v| v.as_str()))
                .map(String::from);
        }
    }

    // Parse properties that belong directly to this backbone element
    for element in elements.iter() {
        let path = element.get("path").and_then(|v| v.as_str()).unwrap_or("");

        if !path.starts_with(&expected_prefix) {
            continue;
        }

        let remainder = &path[expected_prefix.len()..];

        // Only take direct properties (no further nesting)
        if !remainder.contains('.') && !remainder.is_empty() {
            if let Ok(property) = parse_element(element, &full_path) {
                properties.push(property);
            }
        }
    }

    // Capitalize first letter for struct name
    let struct_name = capitalize_first(name);

    Ok(BackboneElement {
        name: struct_name,
        path: full_path,
        description,
        properties,
    })
}

/// Capitalize the first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

/// Parse a single element into a Property
fn parse_element(element: &Value, type_name: &str) -> Result<Property> {
    let path = element
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Element missing 'path'"))?;

    // Extract property name from path (e.g., "Patient.name" -> "name")
    let name = path
        .rsplit('.')
        .next()
        .ok_or_else(|| anyhow!("Invalid path: {}", path))?
        .to_string();

    // Validate path matches the expected type/parent
    let expected_prefix = format!("{}.", type_name);
    if !path.starts_with(&expected_prefix) {
        return Err(anyhow!("Path doesn't match type: {}", path));
    }

    let description = element
        .get("short")
        .and_then(|v| v.as_str())
        .or_else(|| element.get("definition").and_then(|v| v.as_str()))
        .map(String::from);

    // Parse cardinality
    let min = element.get("min").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

    let max_str = element.get("max").and_then(|v| v.as_str());
    let max = match max_str {
        Some("*") => None,
        Some(n) => n.parse().ok(),
        None => Some(1),
    };

    let cardinality = Cardinality::new(min, max);
    let is_required = cardinality.is_required();

    // Parse types
    let types = if let Some(type_array) = element.get("type").and_then(|v| v.as_array()) {
        type_array
            .iter()
            .filter_map(|t| parse_element_type(t).ok())
            .collect()
    } else {
        Vec::new()
    };

    let is_modifier = element
        .get("isModifier")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let must_support = element
        .get("mustSupport")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(Property {
        name,
        path: path.to_string(),
        description,
        types,
        cardinality,
        is_required,
        is_modifier,
        must_support,
    })
}

/// Parse a type specification from an element
fn parse_element_type(type_spec: &Value) -> Result<PropertyType> {
    let code = type_spec
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Type missing 'code'"))?
        .to_string();

    let profile = type_spec
        .get("profile")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(String::from);

    let target_profiles = type_spec
        .get("targetProfile")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(PropertyType {
        code,
        profile,
        target_profiles,
    })
}

/// Extract the type name from a canonical URL
/// E.g., "http://hl7.org/fhir/StructureDefinition/Patient" -> "Patient"
fn extract_type_name_from_url(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_name_from_url() {
        assert_eq!(
            extract_type_name_from_url("http://hl7.org/fhir/StructureDefinition/Patient"),
            "Patient"
        );
        assert_eq!(extract_type_name_from_url("Patient"), "Patient");
    }
}
