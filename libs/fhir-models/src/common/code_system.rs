//! FHIR CodeSystem model
//!
//! Version-agnostic model for CodeSystems (terminology)

use super::complex::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// FHIR CodeSystem resource
///
/// Declares the existence of and describes a code system or code system supplement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeSystem {
    /// Resource type - always "CodeSystem"
    #[serde(default = "default_resource_type")]
    pub resource_type: String,

    /// Logical id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Canonical identifier
    pub url: String,

    /// Business version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Name (computer friendly)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Name (human friendly)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Publication status
    pub status: PublicationStatus,

    /// For testing purposes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<bool>,

    /// Date last changed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,

    /// Name of the publisher
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,

    /// Contact details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Vec<ContactDetail>>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Content intends to support these contexts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_context: Option<Vec<UsageContext>>,

    /// Intended jurisdiction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<Vec<Value>>,

    /// Why this code system is defined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,

    /// Use and/or publishing restrictions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,

    /// If code comparison is case sensitive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_sensitive: Option<bool>,

    /// Canonical reference to the value set with all codes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_set: Option<String>,

    /// Hierarchy meaning (grouped-by | is-a | part-of | classified-with)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hierarchy_meaning: Option<String>,

    /// If code system defines a compositional grammar
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compositional: Option<bool>,

    /// If definitions are not stable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_needed: Option<bool>,

    /// Content type (not-present | example | fragment | complete | supplement)
    pub content: CodeSystemContentMode,

    /// Canonical URL of the code system this supplements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplements: Option<String>,

    /// Total concepts in the code system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,

    /// Filter definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<CodeSystemFilter>>,

    /// Property definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<Vec<CodeSystemProperty>>,

    /// Concepts in the code system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concept: Option<Vec<CodeSystemConcept>>,

    /// Additional content
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

fn default_resource_type() -> String {
    "CodeSystem".to_string()
}

/// Content mode for a code system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodeSystemContentMode {
    NotPresent,
    Example,
    Fragment,
    Complete,
    Supplement,
}

/// Filter for a code system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSystemFilter {
    /// Code that identifies the filter
    pub code: String,

    /// Description of filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Operators that can be used with filter
    pub operator: Vec<String>,

    /// What to use for the value
    pub value: String,
}

/// Property definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSystemProperty {
    /// Identifies the property
    pub code: String,

    /// Formal identifier for the property
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    /// Description of the property
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Type of property (code | Coding | string | integer | boolean | dateTime | decimal)
    #[serde(rename = "type")]
    pub property_type: String,
}

/// Concept in the code system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSystemConcept {
    /// Code that identifies the concept
    pub code: String,

    /// Text to display to the user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// Formal definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,

    /// Additional representations for the concept
    #[serde(skip_serializing_if = "Option::is_none")]
    pub designation: Option<Vec<Value>>,

    /// Property values for the concept
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<Vec<CodeSystemConceptProperty>>,

    /// Child concepts (nested hierarchy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concept: Option<Vec<CodeSystemConcept>>,
}

/// Property value for a concept
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSystemConceptProperty {
    /// Reference to property definition
    pub code: String,

    /// Value of the property
    #[serde(flatten)]
    pub value: Value,
}

impl CodeSystem {
    /// Create a new CodeSystem with minimal required fields
    pub fn new(
        url: impl Into<String>,
        status: PublicationStatus,
        content: CodeSystemContentMode,
    ) -> Self {
        Self {
            resource_type: "CodeSystem".to_string(),
            id: None,
            url: url.into(),
            version: None,
            name: None,
            title: None,
            status,
            experimental: None,
            date: None,
            publisher: None,
            contact: None,
            description: None,
            use_context: None,
            jurisdiction: None,
            purpose: None,
            copyright: None,
            case_sensitive: None,
            value_set: None,
            hierarchy_meaning: None,
            compositional: None,
            version_needed: None,
            content,
            supplements: None,
            count: None,
            filter: None,
            property: None,
            concept: None,
            extensions: HashMap::new(),
        }
    }
}
