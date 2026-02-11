//! FHIR ElementDefinition model
//!
//! Version-agnostic model for ElementDefinition (used in StructureDefinition snapshots and differentials)

use super::complex::*;
use super::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// FHIR ElementDefinition - defines an element in a resource or data type structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementDefinition {
    /// Unique id for inter-element referencing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Path of the element in the hierarchy (e.g., "Patient.name")
    pub path: String,

    /// Codes that define how this element is represented
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representation: Option<Vec<PropertyRepresentation>>,

    /// Name for this particular element (in a slice)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice_name: Option<String>,

    /// If this slice definition constrains an inherited slice
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice_is_constraining: Option<bool>,

    /// Short label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,

    /// Full formal definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,

    /// Comments about the use of this element
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Why this resource has been created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,

    /// Other names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<Vec<String>>,

    /// Minimum cardinality
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<u32>,

    /// Maximum cardinality (can be "*")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,

    /// Base definition information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<ElementDefinitionBase>,

    /// Reference to definition of content if present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_reference: Option<String>,

    /// Data type and profile for this element
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<ElementDefinitionType>>,

    /// Specified value if missing from instance
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,

    /// Implicit meaning when this element is missing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meaning_when_missing: Option<String>,

    /// What the order of the elements means
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_meaning: Option<String>,

    /// Value must be exactly this (various types)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Value>,

    /// Value must have at least these property values (various types)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<Value>,

    /// Example value (as defined for type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Vec<ElementDefinitionExample>>,

    /// Minimum allowed value (for some types)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub min_value: Option<Value>,

    /// Maximum allowed value (for some types)
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub max_value: Option<Value>,

    /// Max length for strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i32>,

    /// Reference to invariant about presence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<Vec<String>>,

    /// Condition that must evaluate to true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<Vec<ElementDefinitionConstraint>>,

    /// If this modifies the meaning of other elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_modifier: Option<bool>,

    /// Reason that this element is marked as a modifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_modifier_reason: Option<String>,

    /// Include when in summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_summary: Option<bool>,

    /// ValueSet details if this is coded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<ElementDefinitionBinding>,

    /// Map element to another set of definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<Vec<ElementDefinitionMapping>>,

    /// This element is sliced - slices follow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slicing: Option<ElementDefinitionSlicing>,

    /// If this element must be supported
    #[serde(skip_serializing_if = "Option::is_none")]
    pub must_support: Option<bool>,

    /// Additional content beyond core fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// How a property is represented when serialized
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyRepresentation {
    XmlAttr,
    XmlText,
    TypeAttr,
    CdaText,
    Xhtml,
}

/// Base definition information for an element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementDefinitionBase {
    /// Path that identifies the base element
    pub path: String,

    /// Min cardinality of the base element
    pub min: u32,

    /// Max cardinality of the base element
    pub max: String,
}

/// Data type for an element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementDefinitionType {
    /// Data type code
    pub code: String,

    /// Profile (StructureDefinition canonical URLs) that apply
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Vec<String>>,

    /// Profile (StructureDefinition) for Reference/canonical target types
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_profile: Option<Vec<String>>,

    /// Aggregation modes for references (contained | referenced | bundled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<Vec<AggregationMode>>,

    /// Versioning rule for references (either | independent | specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versioning: Option<ReferenceVersionRules>,
}

/// How aggregated references are handled
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregationMode {
    Contained,
    Referenced,
    Bundled,
}

/// How reference versions are handled
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceVersionRules {
    Either,
    Independent,
    Specific,
}

/// Example value for an element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementDefinitionExample {
    /// Describes the purpose of this example
    pub label: String,

    /// Value of example (one of various types)
    #[serde(flatten)]
    pub value: Value,
}

/// Constraint on an element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementDefinitionConstraint {
    /// Target of 'condition' reference
    pub key: String,

    /// Why this constraint is necessary or appropriate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,

    /// Severity (error | warning)
    pub severity: ConstraintSeverity,

    /// Human description of constraint
    pub human: String,

    /// FHIRPath expression of constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,

    /// XPath expression of constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xpath: Option<String>,

    /// Reference to original source of constraint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Severity of a constraint
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintSeverity {
    Error,
    Warning,
}

/// ValueSet binding for a coded element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ElementDefinitionBinding {
    /// Binding strength (required | extensible | preferred | example)
    pub strength: BindingStrength,

    /// Human explanation of the value set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Source of value set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_set: Option<String>,
}

/// Mapping to another standard
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementDefinitionMapping {
    /// Reference to mapping declaration
    pub identity: String,

    /// Computable language of mapping
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Details of the mapping
    pub map: String,

    /// Comments about the mapping
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Slicing information for an element
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementDefinitionSlicing {
    /// Element values that are used to distinguish slices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Vec<ElementDefinitionDiscriminator>>,

    /// Text description of how slicing works
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// If elements must be in same order as slices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,

    /// Slicing rules (closed | open | openAtEnd)
    pub rules: SlicingRules,
}

/// Discriminator for slicing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementDefinitionDiscriminator {
    /// Type of discriminator (value | exists | pattern | type | profile)
    #[serde(rename = "type")]
    pub discriminator_type: DiscriminatorType,

    /// Path to element value
    pub path: String,
}

/// Type of slicing discriminator
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscriminatorType {
    Value,
    Exists,
    Pattern,
    Type,
    Profile,
}

/// Slicing rules
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SlicingRules {
    Closed,
    Open,
    OpenAtEnd,
}

/// Snapshot - a set of elements that define the structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub element: Vec<ElementDefinition>,
}

/// Differential - a set of elements that define changes from the base
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Differential {
    pub element: Vec<ElementDefinition>,
}

impl Snapshot {
    /// Parse from JSON Value
    pub fn from_value(value: &Value) -> Result<Self> {
        serde_json::from_value(value.clone()).map_err(Error::from)
    }
}

impl Differential {
    /// Parse from JSON Value
    pub fn from_value(value: &Value) -> Result<Self> {
        serde_json::from_value(value.clone()).map_err(Error::from)
    }
}

impl ElementDefinition {
    /// Get the key for this element (path:sliceName for slices, just path otherwise)
    pub fn key(&self) -> String {
        if let Some(ref slice_name) = self.slice_name {
            format!("{}:{}", self.path, slice_name)
        } else {
            self.path.clone()
        }
    }

    /// Check if this element has a slice name
    pub fn is_slice(&self) -> bool {
        self.slice_name.is_some()
    }

    /// Get the parent path (everything before the last '.')
    pub fn parent_path(&self) -> Option<String> {
        self.path.rfind('.').map(|pos| self.path[..pos].to_string())
    }

    /// Check if this element is a descendant of the given path
    pub fn is_descendant_of(&self, parent_path: &str) -> bool {
        self.path.starts_with(parent_path)
            && self.path.len() > parent_path.len()
            && self.path.as_bytes().get(parent_path.len()) == Some(&b'.')
    }

    /// Check if this is a choice type element (ends with [x])
    pub fn is_choice_type(&self) -> bool {
        self.path.ends_with("[x]")
    }

    /// Get type codes for this element
    pub fn type_codes(&self) -> Vec<String> {
        self.types
            .as_ref()
            .map(|types| types.iter().map(|t| t.code.clone()).collect())
            .unwrap_or_default()
    }

    /// Check if element is required (min > 0)
    pub fn is_required(&self) -> bool {
        self.min.unwrap_or(0) > 0
    }

    /// Check if element is array/list (max = "*" or max > 1)
    pub fn is_array(&self) -> bool {
        self.max
            .as_ref()
            .map(|m| m == "*" || m.parse::<u32>().map(|n| n > 1).unwrap_or(false))
            .unwrap_or(false)
    }

    /// Get the cardinality as a string (e.g., "0..1", "1..*")
    pub fn cardinality_string(&self) -> String {
        let min = self.min.unwrap_or(0);
        let max = self.max.as_deref().unwrap_or("*");
        format!("{}..{}", min, max)
    }

    /// Extract type information into an ElementTypeInfo struct
    pub fn to_type_info(&self) -> Option<ElementTypeInfo> {
        let type_codes = self.type_codes();

        if type_codes.is_empty() {
            return None;
        }

        let is_choice = self.is_choice_type();
        let is_array = self.is_array();
        let min = self.min.unwrap_or(0);

        // Parse max: "*" becomes None, otherwise parse as integer
        let max = self.max.as_ref().and_then(|m| {
            if m == "*" {
                None
            } else {
                m.parse::<u32>().ok()
            }
        });

        Some(ElementTypeInfo {
            path: self.path.clone(),
            type_codes,
            is_array,
            min,
            max,
            is_choice,
        })
    }
}

/// Information about an element's type
///
/// A convenience struct that extracts commonly-used type information from an ElementDefinition.
#[derive(Debug, Clone)]
pub struct ElementTypeInfo {
    pub path: String,
    pub type_codes: Vec<String>,
    pub is_array: bool,
    pub min: u32,
    pub max: Option<u32>,
    pub is_choice: bool,
}

impl Snapshot {
    /// Create a new empty snapshot
    pub fn new() -> Self {
        Self {
            element: Vec::new(),
        }
    }

    /// Get an element by path
    pub fn get_element(&self, path: &str) -> Option<&ElementDefinition> {
        self.element.iter().find(|e| e.path == path)
    }

    /// Get a mutable element by path
    pub fn get_element_mut(&mut self, path: &str) -> Option<&mut ElementDefinition> {
        self.element.iter_mut().find(|e| e.path == path)
    }

    /// Get all direct children of a path
    pub fn get_children(&self, parent_path: &str) -> Vec<&ElementDefinition> {
        let expected_depth = parent_path.matches('.').count() + 1;
        self.element
            .iter()
            .filter(|e| {
                e.is_descendant_of(parent_path) && e.path.matches('.').count() == expected_depth
            })
            .collect()
    }

    /// Sort elements in canonical FHIR order
    pub fn sort_elements(&mut self) {
        self.element.sort_by(|a, b| {
            // First compare by path depth
            let a_depth = a.path.matches('.').count();
            let b_depth = b.path.matches('.').count();

            match a_depth.cmp(&b_depth) {
                std::cmp::Ordering::Equal => {
                    // Same depth - compare paths, but slices come after base
                    match (a.is_slice(), b.is_slice()) {
                        (false, true) if a.path == b.path => std::cmp::Ordering::Less,
                        (true, false) if a.path == b.path => std::cmp::Ordering::Greater,
                        _ => a.path.cmp(&b.path).then_with(|| {
                            // If same path, sort by slice name
                            a.slice_name.cmp(&b.slice_name)
                        }),
                    }
                }
                other => other,
            }
        });
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl Differential {
    /// Create a new empty differential
    pub fn new() -> Self {
        Self {
            element: Vec::new(),
        }
    }

    /// Get an element by path
    pub fn get_element(&self, path: &str) -> Option<&ElementDefinition> {
        self.element.iter().find(|e| e.path == path)
    }
}

impl Default for Differential {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_key() {
        let elem = ElementDefinition {
            id: None,
            path: "Patient.name".to_string(),
            slice_name: Some("official".to_string()),
            representation: None,
            slice_is_constraining: None,
            short: None,
            definition: None,
            comment: None,
            requirements: None,
            alias: None,
            min: None,
            max: None,
            base: None,
            content_reference: None,
            types: None,
            default_value: None,
            meaning_when_missing: None,
            order_meaning: None,
            fixed: None,
            pattern: None,
            example: None,
            min_value: None,
            max_value: None,
            max_length: None,
            condition: None,
            constraint: None,
            is_modifier: None,
            is_modifier_reason: None,
            is_summary: None,
            binding: None,
            mapping: None,
            slicing: None,
            must_support: None,
            extensions: HashMap::new(),
        };

        assert_eq!(elem.key(), "Patient.name:official");
        assert!(elem.is_slice());
    }

    #[test]
    fn test_is_choice_type() {
        let mut elem = ElementDefinition {
            id: None,
            path: "Observation.value[x]".to_string(),
            slice_name: None,
            representation: None,
            slice_is_constraining: None,
            short: None,
            definition: None,
            comment: None,
            requirements: None,
            alias: None,
            min: None,
            max: None,
            base: None,
            content_reference: None,
            types: None,
            default_value: None,
            meaning_when_missing: None,
            order_meaning: None,
            fixed: None,
            pattern: None,
            example: None,
            min_value: None,
            max_value: None,
            max_length: None,
            condition: None,
            constraint: None,
            is_modifier: None,
            is_modifier_reason: None,
            is_summary: None,
            binding: None,
            mapping: None,
            slicing: None,
            must_support: None,
            extensions: HashMap::new(),
        };

        assert!(elem.is_choice_type());

        elem.path = "Observation.value".to_string();
        assert!(!elem.is_choice_type());
    }

    #[test]
    fn test_cardinality_string() {
        let elem = ElementDefinition {
            id: None,
            path: "Patient.name".to_string(),
            slice_name: None,
            representation: None,
            slice_is_constraining: None,
            short: None,
            definition: None,
            comment: None,
            requirements: None,
            alias: None,
            min: Some(1),
            max: Some("*".to_string()),
            base: None,
            content_reference: None,
            types: None,
            default_value: None,
            meaning_when_missing: None,
            order_meaning: None,
            fixed: None,
            pattern: None,
            example: None,
            min_value: None,
            max_value: None,
            max_length: None,
            condition: None,
            constraint: None,
            is_modifier: None,
            is_modifier_reason: None,
            is_summary: None,
            binding: None,
            mapping: None,
            slicing: None,
            must_support: None,
            extensions: HashMap::new(),
        };

        assert_eq!(elem.cardinality_string(), "1..*");
        assert!(elem.is_required());
        assert!(elem.is_array());
    }
}
