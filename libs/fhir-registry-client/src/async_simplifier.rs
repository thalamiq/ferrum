//! Simplifier registry API client

use crate::error::{Error, Result};
use crate::models::{SimplifierSearchParams, SimplifierSearchResult};
use reqwest::Client;
use std::time::Duration;
use ferrum_package::FhirPackage;

const SIMPLIFIER_BASE_URL: &str = "https://packages.simplifier.net";

/// Client for the Simplifier package registry.
pub struct SimplifierClient {
    client: Client,
    base_url: String,
}

impl SimplifierClient {
    /// Create a new Simplifier client with default settings.
    pub fn new() -> Result<Self> {
        Self::with_base_url(SIMPLIFIER_BASE_URL.to_string())
    }

    /// Create a Simplifier client with a custom base URL.
    pub fn with_base_url(base_url: String) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self { client, base_url })
    }

    /// Search for packages in the Simplifier registry.
    pub async fn search(
        &self,
        params: &SimplifierSearchParams,
    ) -> Result<Vec<SimplifierSearchResult>> {
        let mut url = format!("{}/catalog", self.base_url);
        let mut query_params = Vec::new();

        if let Some(name) = &params.name {
            query_params.push(format!("name={}", urlencoding::encode(name)));
        }
        if let Some(canonical) = &params.canonical {
            query_params.push(format!("canonical={}", urlencoding::encode(canonical)));
        }
        if let Some(fhir_version) = &params.fhir_version {
            query_params.push(format!("fhirversion={}", urlencoding::encode(fhir_version)));
        }
        if let Some(prerelease) = params.prerelease {
            query_params.push(format!("prerelease={}", prerelease));
        }

        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Search failed with status: {}",
                response.status()
            )));
        }

        let results: Vec<SimplifierSearchResult> = response.json().await?;
        Ok(results)
    }

    /// Get all versions for a package.
    pub async fn get_versions(&self, package_name: &str) -> Result<Vec<String>> {
        let url = format!("{}/{}", self.base_url, package_name);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(Error::Registry(format!(
                "Failed to get versions for {}: status {}",
                package_name,
                response.status()
            )));
        }

        // Parse package metadata to extract version keys
        let package_metadata: serde_json::Value = response.json().await?;

        let versions = package_metadata
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect::<Vec<String>>())
            .ok_or_else(|| {
                Error::Registry(format!(
                    "Invalid package metadata for {}: missing or invalid 'versions' field",
                    package_name
                ))
            })?;

        Ok(versions)
    }

    /// Download a package from the Simplifier registry.
    pub async fn download_package(&self, package_name: &str, version: &str) -> Result<FhirPackage> {
        let url = format!("{}/{}/{}", self.base_url, package_name, version);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(Error::PackageNotFound {
                name: package_name.to_string(),
                version: version.to_string(),
            });
        }

        let bytes = response.bytes().await?;
        let package = FhirPackage::from_tar_gz_bytes(&bytes)?;
        Ok(package)
    }
}

impl Default for SimplifierClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default SimplifierClient")
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_parse_package_metadata_versions() {
        // Simulate the response from Simplifier API
        let metadata_json = r#"{
            "_id": "de.basisprofil.r4",
            "name": "de.basisprofil.r4",
            "dist-tags": {"latest": "1.5.4"},
            "versions": {
                "0.9.0": {
                    "name": "de.basisprofil.r4",
                    "version": "0.9.0"
                },
                "1.5.3": {
                    "name": "de.basisprofil.r4",
                    "version": "1.5.3"
                },
                "1.5.4": {
                    "name": "de.basisprofil.r4",
                    "version": "1.5.4"
                }
            }
        }"#;

        let package_metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap();

        let versions = package_metadata
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| {
                let mut vers: Vec<String> = obj.keys().cloned().collect();
                vers.sort();
                vers
            })
            .unwrap();

        assert_eq!(versions.len(), 3);
        assert!(versions.contains(&"0.9.0".to_string()));
        assert!(versions.contains(&"1.5.3".to_string()));
        assert!(versions.contains(&"1.5.4".to_string()));
    }

    #[test]
    fn test_parse_empty_versions() {
        let metadata_json = r#"{
            "_id": "test.package",
            "name": "test.package",
            "versions": {}
        }"#;

        let package_metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap();

        let versions = package_metadata
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect::<Vec<String>>())
            .unwrap();

        assert_eq!(versions.len(), 0);
    }

    #[test]
    fn test_parse_missing_versions_field() {
        let metadata_json = r#"{
            "_id": "test.package",
            "name": "test.package"
        }"#;

        let package_metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap();

        let result = package_metadata
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect::<Vec<String>>());

        assert!(result.is_none());
    }

    #[test]
    fn test_parse_invalid_versions_field() {
        // versions field is an array instead of object
        let metadata_json = r#"{
            "_id": "test.package",
            "name": "test.package",
            "versions": []
        }"#;

        let package_metadata: serde_json::Value = serde_json::from_str(metadata_json).unwrap();

        let result = package_metadata
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect::<Vec<String>>());

        assert!(result.is_none());
    }
}
