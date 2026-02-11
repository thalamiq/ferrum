//! FHIR ValueSet model
//!
//! Version-agnostic model for ValueSets (terminology)

use super::complex::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// FHIR ValueSet resource
///
/// A set of codes drawn from one or more code systems.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueSet {
    /// Resource type - always "ValueSet"
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

    /// Immutable flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub immutable: Option<bool>,

    /// Why this value set is defined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,

    /// Use and/or publishing restrictions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copyright: Option<String>,

    /// Content logical definition (the "intension")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compose: Option<ValueSetCompose>,

    /// Used when the value set is "expanded"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expansion: Option<ValueSetExpansion>,

    /// Additional content
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

fn default_resource_type() -> String {
    "ValueSet".to_string()
}

/// Content logical definition of the value set (intension)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueSetCompose {
    /// Fixed date for references with no specified version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_date: Option<String>,

    /// Whether inactive codes are in the value set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactive: Option<bool>,

    /// Include one or more codes from a code system or other value set
    pub include: Vec<ValueSetInclude>,

    /// Explicitly exclude codes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<ValueSetInclude>>,
}

/// Include codes from a code system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueSetInclude {
    /// The system the codes come from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,

    /// Specific version of the code system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Specific codes from the system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concept: Option<Vec<ValueSetConcept>>,

    /// Select codes/concepts by their properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<ValueSetFilter>>,

    /// Select only contents included in specified value set(s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_set: Option<Vec<String>>,
}

/// A concept defined in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueSetConcept {
    /// Code from the system
    pub code: String,

    /// Text to display for this code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// Additional representations for this concept
    #[serde(skip_serializing_if = "Option::is_none")]
    pub designation: Option<Vec<Value>>,
}

/// Select codes by property
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueSetFilter {
    /// Property name
    pub property: String,

    /// Filter operator (= | is-a | descendent-of | is-not-a | regex | in | not-in | generalizes | exists)
    pub op: String,

    /// Value of the filter
    pub value: String,
}

/// Expansion of the value set
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueSetExpansion {
    /// Uniquely identifies this expansion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,

    /// Time valueset expansion was generated
    pub timestamp: String,

    /// Total number of codes in the expansion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i32>,

    /// Offset at which this resource starts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i32>,

    /// Parameters used for expansion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<Vec<Value>>,

    /// Codes in the value set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<Vec<ValueSetExpansionContains>>,
}

/// Codes in an expansion
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValueSetExpansionContains {
    /// System value for the code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,

    /// If user cannot select this entry
    #[serde(rename = "abstract", skip_serializing_if = "Option::is_none")]
    pub is_abstract: Option<bool>,

    /// If concept is inactive in the code system
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactive: Option<bool>,

    /// Version in which this code/display is defined
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Code - if blank, this is not a selectable code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// User display for the concept
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// Codes contained under this entry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<Vec<ValueSetExpansionContains>>,
}

impl ValueSet {
    /// Create a new ValueSet with minimal required fields
    pub fn new(url: impl Into<String>, status: PublicationStatus) -> Self {
        Self {
            resource_type: "ValueSet".to_string(),
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
            immutable: None,
            purpose: None,
            copyright: None,
            compose: None,
            expansion: None,
            extensions: HashMap::new(),
        }
    }
}
