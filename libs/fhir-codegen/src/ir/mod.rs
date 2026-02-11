//! Intermediate Representation (IR)
//!
//! Language-agnostic representation of FHIR types extracted from StructureDefinitions.
//! This IR serves as the bridge between FHIR definitions and language-specific code.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Registry of all types extracted from a FHIR package
#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    /// All types indexed by their canonical URL or ID
    types: HashMap<String, TypeDefinition>,
    /// Mapping from type name to canonical identifier
    name_index: HashMap<String, String>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a type to the registry
    pub fn add_type(&mut self, id: String, type_def: TypeDefinition) {
        self.name_index.insert(type_def.name.clone(), id.clone());
        self.types.insert(id, type_def);
    }

    /// Get a type by its canonical identifier
    pub fn get_type(&self, id: &str) -> Option<&TypeDefinition> {
        self.types.get(id)
    }

    /// Get a type by its name
    pub fn get_type_by_name(&self, name: &str) -> Option<&TypeDefinition> {
        self.name_index.get(name).and_then(|id| self.types.get(id))
    }

    /// Iterate over all types
    pub fn types(&self) -> impl Iterator<Item = (&String, &TypeDefinition)> {
        self.types.iter()
    }

    /// Get all resource types (kinds: resource, complex-type with derivation)
    pub fn resource_types(&self) -> impl Iterator<Item = &TypeDefinition> {
        self.types.values().filter(|t| t.kind == TypeKind::Resource)
    }

    /// Get all complex types (datatypes, backbones, etc.)
    pub fn complex_types(&self) -> impl Iterator<Item = &TypeDefinition> {
        self.types
            .values()
            .filter(|t| matches!(t.kind, TypeKind::ComplexType | TypeKind::BackboneElement))
    }

    /// Get all primitive types
    pub fn primitive_types(&self) -> impl Iterator<Item = &TypeDefinition> {
        self.types
            .values()
            .filter(|t| t.kind == TypeKind::PrimitiveType)
    }

    /// Get dependencies for a given type (other types it references)
    pub fn get_dependencies(&self, type_def: &TypeDefinition) -> Vec<String> {
        let mut deps = Vec::new();

        // Dependencies from direct properties
        for property in &type_def.properties {
            for prop_type in &property.types {
                let type_name = &prop_type.code;

                // Skip primitive types and special types
                if !is_primitive_type(type_name)
                    && type_name != "Resource"
                    && type_name != "Element"
                    && self.get_type_by_name(type_name).is_some()
                    && !deps.contains(type_name)
                {
                    deps.push(type_name.clone());
                }
            }
        }

        // Dependencies from backbone elements
        for backbone in &type_def.backbone_elements {
            for property in &backbone.properties {
                for prop_type in &property.types {
                    let type_name = &prop_type.code;

                    if !is_primitive_type(type_name)
                        && type_name != "Resource"
                        && type_name != "Element"
                        && self.get_type_by_name(type_name).is_some()
                        && !deps.contains(type_name)
                    {
                        deps.push(type_name.clone());
                    }
                }
            }
        }

        deps
    }
}

/// Check if a type is a FHIR primitive
fn is_primitive_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "boolean"
            | "integer"
            | "unsignedInt"
            | "positiveInt"
            | "integer64"
            | "decimal"
            | "string"
            | "code"
            | "id"
            | "markdown"
            | "uri"
            | "url"
            | "canonical"
            | "oid"
            | "uuid"
            | "date"
            | "dateTime"
            | "instant"
            | "time"
            | "base64Binary"
    )
}

/// A single type definition extracted from a StructureDefinition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDefinition {
    /// The type name (e.g., "Patient", "HumanName", "string")
    pub name: String,
    /// Canonical URL if available
    pub url: Option<String>,
    /// Human-readable description
    pub description: Option<String>,
    /// Kind of type (resource, complex-type, primitive)
    pub kind: TypeKind,
    /// Base type this extends (if any)
    pub base_type: Option<String>,
    /// Properties/elements of this type
    pub properties: Vec<Property>,
    /// Whether this is an abstract type
    pub is_abstract: bool,
    /// Backbone elements defined within this type (for resources)
    pub backbone_elements: Vec<BackboneElement>,
    /// Parent type name if this is a backbone element
    pub parent_type: Option<String>,
}

/// Kind of FHIR type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeKind {
    /// FHIR Resource (e.g., Patient, Observation)
    Resource,
    /// Complex datatype (e.g., HumanName, Address, Coding)
    ComplexType,
    /// Primitive type (e.g., string, integer, boolean)
    PrimitiveType,
    /// Backbone element (nested complex element within a resource)
    BackboneElement,
}

/// A property/field within a type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    /// Property name (e.g., "name", "birthDate")
    pub name: String,
    /// Path in the FHIR element tree (e.g., "Patient.name")
    pub path: String,
    /// Human-readable description
    pub description: Option<String>,
    /// The type(s) this property can have
    pub types: Vec<PropertyType>,
    /// Cardinality
    pub cardinality: Cardinality,
    /// Whether this property is required
    pub is_required: bool,
    /// Whether this property is a modifier element
    pub is_modifier: bool,
    /// Whether this property must be supported
    pub must_support: bool,
}

/// Type reference for a property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyType {
    /// Type code (e.g., "string", "CodeableConcept", "Reference")
    pub code: String,
    /// Target profile URL (for References or profiled types)
    pub profile: Option<String>,
    /// Target resource types (for Reference properties)
    pub target_profiles: Vec<String>,
}

/// Cardinality of a property (min..max)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cardinality {
    /// Minimum occurrences
    pub min: u32,
    /// Maximum occurrences (None means unbounded/*)
    pub max: Option<u32>,
}

impl Cardinality {
    pub fn new(min: u32, max: Option<u32>) -> Self {
        Self { min, max }
    }

    /// Check if this property is a list/array
    pub fn is_array(&self) -> bool {
        self.max.map(|m| m > 1).unwrap_or(true)
    }

    /// Check if this property is optional
    pub fn is_optional(&self) -> bool {
        self.min == 0
    }

    /// Check if this property is required
    pub fn is_required(&self) -> bool {
        self.min > 0
    }
}

/// A backbone element (inline complex type) within a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackboneElement {
    /// Name of the backbone element (e.g., "Contact" for Patient.contact)
    pub name: String,
    /// Full path (e.g., "Patient.contact")
    pub path: String,
    /// Description
    pub description: Option<String>,
    /// Properties of this backbone element
    pub properties: Vec<Property>,
}
