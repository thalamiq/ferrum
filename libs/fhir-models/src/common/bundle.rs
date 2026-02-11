//! FHIR Bundle model
//!
//! Version-agnostic model for Bundles that works across R4, R4B, and R5.

use super::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// FHIR Bundle resource
///
/// A container for a collection of resources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    /// Resource type - always "Bundle"
    #[serde(default = "default_resource_type")]
    pub resource_type: String,

    /// Logical id of this artifact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Indicates the purpose of this bundle - how it was intended to be used
    #[serde(rename = "type")]
    pub bundle_type: BundleType,

    /// When the bundle was assembled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,

    /// If search, the total number of matches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,

    /// Links related to this Bundle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Vec<BundleLink>>,

    /// Entry in the bundle - will have a resource or information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<Vec<BundleEntry>>,

    /// Digital Signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Value>,

    /// Additional content beyond core fields (extensions, version-specific fields)
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

fn default_resource_type() -> String {
    "Bundle".to_string()
}

/// Type of Bundle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BundleType {
    /// Document Bundle - A set of resources composing a single coherent document
    Document,
    /// Message Bundle - A message (application/response or application/request)
    Message,
    /// Transaction Bundle - A transaction - intended to be processed atomically
    Transaction,
    /// Transaction Response Bundle - Response to a transaction
    #[serde(rename = "transaction-response")]
    TransactionResponse,
    /// Batch Bundle - A set of resources collected for a specific purpose
    Batch,
    /// Batch Response Bundle - Response to a batch
    #[serde(rename = "batch-response")]
    BatchResponse,
    /// History Bundle - A list of resources with history
    History,
    /// Search Results Bundle - Results of a search operation
    Searchset,
    /// Collection Bundle - A set of resources collected for a specific purpose
    Collection,
}

/// Links related to this Bundle
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleLink {
    /// See http://www.iana.org/assignments/link-relations/link-relations.xhtml#link-relations-1
    pub relation: String,

    /// Reference details for the link
    pub url: String,
}

/// Entry in the bundle
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleEntry {
    /// Full URL for the entry (relative to the base URL, or absolute)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,

    /// URI for the entry (e.g., urn:uuid:...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<BundleEntryRequest>,

    /// Results of execution (transaction/batch/history)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<BundleEntryResponse>,

    /// A resource in this bundle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<Value>,

    /// Search-related information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<BundleEntrySearch>,

    /// Additional content beyond core fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Request details for a Bundle entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleEntryRequest {
    /// HTTP verb for the entry (GET | POST | PUT | PATCH | DELETE)
    pub method: String,

    /// URL for HTTP equivalent of this entry
    pub url: String,

    /// For managing cache validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_none_match: Option<String>,

    /// For managing cache validation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_modified_since: Option<String>,

    /// For managing update contention
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_match: Option<String>,

    /// For conditional creates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_none_exist: Option<String>,

    /// Additional content beyond core fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Response details for a Bundle entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleEntryResponse {
    /// Status response code (text)
    pub status: String,

    /// The location (if the operation returns a location)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,

    /// The Etag for the resource (if relevant)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,

    /// Server's date time modified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,

    /// OperationOutcome with hints and warnings (for batch/transaction)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Value>,

    /// Additional content beyond core fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Search-related information for a Bundle entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleEntrySearch {
    /// Why this entry is in the result set - whether it's included as a match or because of an _include requirement
    #[serde(rename = "mode", skip_serializing_if = "Option::is_none")]
    pub search_mode: Option<BundleEntrySearchMode>,

    /// Search ranking (between 0 and 1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,

    /// Additional content beyond core fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Why an entry is in the result set
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleEntrySearchMode {
    /// This resource matched the search specification
    Match,
    /// This resource is returned because it is referred to from another resource in the search set
    Include,
    /// An OperationOutcome providing additional information about the processing of a search entry
    Outcome,
}

impl Bundle {
    /// Create a new Bundle with minimal required fields
    pub fn new(bundle_type: BundleType) -> Self {
        Self {
            resource_type: "Bundle".to_string(),
            id: None,
            bundle_type,
            timestamp: None,
            total: None,
            link: None,
            entry: None,
            signature: None,
            extensions: HashMap::new(),
        }
    }

    /// Parse from JSON Value
    pub fn from_value(value: &Value) -> Result<Self> {
        serde_json::from_value(value.clone()).map_err(Error::from)
    }

    /// Convert to JSON Value
    pub fn to_value(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(Error::from)
    }

    /// Check if this is a transaction bundle
    pub fn is_transaction(&self) -> bool {
        matches!(self.bundle_type, BundleType::Transaction)
    }

    /// Check if this is a batch bundle
    pub fn is_batch(&self) -> bool {
        matches!(self.bundle_type, BundleType::Batch)
    }

    /// Check if this is a search result bundle
    pub fn is_searchset(&self) -> bool {
        matches!(self.bundle_type, BundleType::Searchset)
    }

    /// Get the number of entries in the bundle
    pub fn entry_count(&self) -> usize {
        self.entry.as_ref().map(|e| e.len()).unwrap_or(0)
    }

    /// Get entries as a slice
    pub fn entries(&self) -> &[BundleEntry] {
        self.entry.as_deref().unwrap_or(&[])
    }

    /// Get entries as a mutable slice
    pub fn entries_mut(&mut self) -> &mut [BundleEntry] {
        self.entry.as_deref_mut().unwrap_or(&mut [])
    }

    /// Add an entry to the bundle
    pub fn add_entry(&mut self, entry: BundleEntry) {
        if self.entry.is_none() {
            self.entry = Some(Vec::new());
        }
        if let Some(ref mut entries) = self.entry {
            entries.push(entry);
        }
    }

    /// Add a link to the bundle
    pub fn add_link(&mut self, relation: impl Into<String>, url: impl Into<String>) {
        if self.link.is_none() {
            self.link = Some(Vec::new());
        }
        if let Some(ref mut links) = self.link {
            links.push(BundleLink {
                relation: relation.into(),
                url: url.into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_bundle() {
        let json = json!({
            "resourceType": "Bundle",
            "id": "example-bundle",
            "type": "searchset",
            "total": 1,
            "entry": [
                {
                    "fullUrl": "http://example.org/fhir/Patient/123",
                    "resource": {
                        "resourceType": "Patient",
                        "id": "123"
                    },
                    "search": {
                        "mode": "match",
                        "score": 1.0
                    }
                }
            ]
        });

        let bundle: Bundle = serde_json::from_value(json).unwrap();
        assert_eq!(bundle.id, Some("example-bundle".to_string()));
        assert_eq!(bundle.bundle_type, BundleType::Searchset);
        assert_eq!(bundle.total, Some(1));
        assert_eq!(bundle.entry_count(), 1);
    }

    #[test]
    fn test_serialize_bundle() {
        let bundle = Bundle::new(BundleType::Transaction);
        let json = serde_json::to_value(&bundle).unwrap();
        assert_eq!(json["resourceType"], "Bundle");
        assert_eq!(json["type"], "transaction");
    }

    #[test]
    fn test_is_transaction() {
        let bundle = Bundle::new(BundleType::Transaction);
        assert!(bundle.is_transaction());
        assert!(!bundle.is_batch());
    }

    #[test]
    fn test_is_batch() {
        let bundle = Bundle::new(BundleType::Batch);
        assert!(bundle.is_batch());
        assert!(!bundle.is_transaction());
    }

    #[test]
    fn test_is_searchset() {
        let bundle = Bundle::new(BundleType::Searchset);
        assert!(bundle.is_searchset());
    }

    #[test]
    fn test_add_entry() {
        let mut bundle = Bundle::new(BundleType::Collection);
        let entry = BundleEntry {
            full_url: Some("http://example.org/fhir/Patient/123".to_string()),
            request: None,
            response: None,
            resource: Some(json!({"resourceType": "Patient", "id": "123"})),
            search: None,
            extensions: HashMap::new(),
        };

        bundle.add_entry(entry);
        assert_eq!(bundle.entry_count(), 1);
    }

    #[test]
    fn test_add_link() {
        let mut bundle = Bundle::new(BundleType::Searchset);
        bundle.add_link("self", "http://example.org/fhir/Patient?_id=123");
        assert_eq!(bundle.link.as_ref().unwrap().len(), 1);
        assert_eq!(bundle.link.as_ref().unwrap()[0].relation, "self");
    }

    #[test]
    fn test_bundle_entry_request() {
        let request = BundleEntryRequest {
            method: "POST".to_string(),
            url: "Patient".to_string(),
            if_none_match: None,
            if_modified_since: None,
            if_match: None,
            if_none_exist: None,
            extensions: HashMap::new(),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "POST");
        assert_eq!(json["url"], "Patient");
    }

    #[test]
    fn test_bundle_entry_response() {
        let response = BundleEntryResponse {
            status: "201 Created".to_string(),
            location: Some("Patient/123/_history/1".to_string()),
            etag: Some("W/\"1\"".to_string()),
            last_modified: Some("2023-01-01T00:00:00Z".to_string()),
            outcome: None,
            extensions: HashMap::new(),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["status"], "201 Created");
        assert_eq!(json["location"], "Patient/123/_history/1");
    }
}
