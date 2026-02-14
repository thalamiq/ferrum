//! Data models for FHIR packages

use serde::{Deserialize, Serialize};

// Re-export fhir-package types
pub use ferrum_package::{IndexedFile, PackageIndex, PackageManifest, PackageType};

/// Search result from Simplifier registry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SimplifierSearchResult {
    pub name: String,
    pub description: String,
    #[serde(rename = "FHIRVersion")]
    pub fhir_version: String,
    pub version: String,
}

/// Search parameters for Simplifier registry
#[derive(Debug, Clone, Default)]
pub struct SimplifierSearchParams {
    pub name: Option<String>,
    pub canonical: Option<String>,
    pub fhir_version: Option<String>,
    pub prerelease: Option<bool>,
}
