//! Canonical models for the FHIR NPM Package specification.
//!
//! Provides serde-friendly representations of `package.json` manifests and
//! `.index.json` files with support for extension fields.

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use tar::Archive;
use thiserror::Error;

pub type PackageName = String;
pub type Version = String;
pub type VersionReference = String;

/// Validate version string format per FHIR Package specification.
///
/// Versions must contain only alphanumeric characters, '.', '_', and '-'.
/// Numeric versions (starting with a digit) must follow SemVer format.
pub fn validate_version_format(version: &str) -> Result<(), PackageError> {
    if version.is_empty() {
        return Err(PackageError::ValidationError(
            "Version cannot be empty".into(),
        ));
    }

    let allowed = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '-');
    if !version.chars().all(allowed) {
        return Err(PackageError::ValidationError(format!(
            "Version '{}' contains invalid characters",
            version
        )));
    }

    if version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        let base = version.split('-').next().unwrap_or(version);
        let parts: Vec<&str> = base.split('.').collect();

        if parts.len() < 2 {
            return Err(PackageError::ValidationError(format!(
                "Numeric version '{}' must follow SemVer format (e.g., '1.2.3')",
                version
            )));
        }

        if !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            return Err(PackageError::ValidationError(format!(
                "Version '{}' has non-numeric parts",
                version
            )));
        }
    }

    Ok(())
}

/// Parse version into base and optional label (e.g., "1.2.3-release" → ("1.2.3", Some("release"))).
pub fn parse_version(version: &str) -> (String, Option<String>) {
    if let Some((base, label)) = version.split_once('-') {
        (base.to_string(), Some(label.to_string()))
    } else {
        (version.to_string(), None)
    }
}

/// Compare versions numerically if both start with digits, otherwise lexicographically. Labels ignored.
pub fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    let (base1, _) = parse_version(v1);
    let (base2, _) = parse_version(v2);

    let is_numeric = |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_digit());

    if is_numeric(&base1) && is_numeric(&base2) {
        compare_numeric_versions(&base1, &base2)
    } else {
        base1.cmp(&base2)
    }
}

fn compare_numeric_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    let parts1: Vec<u32> = v1.split('.').filter_map(|p| p.parse().ok()).collect();
    let parts2: Vec<u32> = v2.split('.').filter_map(|p| p.parse().ok()).collect();

    let max_len = parts1.len().max(parts2.len());
    for i in 0..max_len {
        let p1 = parts1.get(i).copied().unwrap_or(0);
        let p2 = parts2.get(i).copied().unwrap_or(0);
        match p1.cmp(&p2) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    std::cmp::Ordering::Equal
}

/// Check if version matches reference (supports exact match, patch wildcards like "1.2.x", and label variants).
pub fn version_matches(version: &str, reference: &str) -> bool {
    if version == reference {
        return true;
    }

    if let Some(prefix) = reference.strip_suffix(".x") {
        if let Some(suffix) = version.strip_prefix(&format!("{}.", prefix)) {
            let (patch, _) = parse_version(suffix);
            return patch.parse::<u32>().is_ok();
        }
        return false;
    }

    let (base_version, _) = parse_version(version);
    let (base_reference, _) = parse_version(reference);
    base_version == base_reference
}

pub type Url = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maintainer {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageType {
    Conformance,
    Ig,
    Core,
    Examples,
    Group,
    Tool,
    IgTemplate,
    Unknown(String),
}

impl Serialize for PackageType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PackageType::Conformance => serializer.serialize_str("Conformance"),
            PackageType::Ig => serializer.serialize_str("IG"),
            PackageType::Core => serializer.serialize_str("Core"),
            PackageType::Examples => serializer.serialize_str("Examples"),
            PackageType::Group => serializer.serialize_str("Group"),
            PackageType::Tool => serializer.serialize_str("Tool"),
            PackageType::IgTemplate => serializer.serialize_str("IG-Template"),
            PackageType::Unknown(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for PackageType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "Conformance" => PackageType::Conformance,
            "IG" => PackageType::Ig,
            "Core" => PackageType::Core,
            "Examples" => PackageType::Examples,
            "Group" => PackageType::Group,
            "Tool" | "fhir.tool" => PackageType::Tool,
            "IG-Template" => PackageType::IgTemplate,
            _ => PackageType::Unknown(s),
        })
    }
}

/// FHIR NPM Package manifest (`package/package.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManifest {
    pub name: PackageName,
    pub version: Version,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<Url>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fhir_versions: Vec<String>,
    #[serde(default)]
    pub dependencies: HashMap<PackageName, VersionReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    pub author: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<Maintainer>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub package_type: Option<PackageType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

impl PackageManifest {
    /// Validate manifest (checks required fields, optionally validates version formats in strict mode).
    pub fn validate(&self, strict: bool) -> Result<(), PackageError> {
        if self.name.is_empty() {
            return Err(PackageError::ValidationError(
                "Package name required".into(),
            ));
        }
        if self.version.is_empty() {
            return Err(PackageError::ValidationError(
                "Package version required".into(),
            ));
        }
        if self.author.is_empty() {
            return Err(PackageError::ValidationError(
                "Package author required".into(),
            ));
        }

        if strict {
            validate_version_format(&self.version)?;

            for dep_version in self.dependencies.values() {
                let version_to_validate = dep_version.strip_suffix(".x").unwrap_or(dep_version);
                validate_version_format(version_to_validate)?;
            }
        }

        Ok(())
    }

    /// Check if package has a core FHIR package dependency.
    pub fn has_core_dependency(&self) -> bool {
        self.dependencies.keys().any(|name| {
            name == "hl7.fhir.core" || (name.starts_with("hl7.fhir.r") && name.ends_with(".core"))
        })
    }
}

/// Package index (`.index.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageIndex {
    #[serde(rename = "index-version")]
    pub index_version: u8,
    pub files: Vec<IndexedFile>,
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

/// File entry in package index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedFile {
    pub filename: String,
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplements: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid structure: {0}")]
    InvalidStructure(String),
    #[error("Missing file: {0}")]
    MissingFile(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}

pub type PackageResult<T> = Result<T, PackageError>;

/// Loaded FHIR package with manifest, optional index, and resources.
///
/// Resources are automatically indexed by ID, canonical URL, and type for fast lookups.
#[derive(Debug, Clone)]
pub struct FhirPackage {
    pub manifest: PackageManifest,
    pub index: Option<PackageIndex>,
    pub resources: Vec<Value>,
    pub examples: Vec<Value>,

    // Indexed resources for fast lookups
    resources_by_id: HashMap<String, Value>,
    resources_by_url: HashMap<String, Value>,
    resources_by_type: HashMap<String, Vec<Value>>,
}

impl FhirPackage {
    /// Create a new FHIR package from manifest and resources.
    ///
    /// Indices are built automatically for fast lookups.
    pub fn new(manifest: PackageManifest, resources: Vec<Value>, examples: Vec<Value>) -> Self {
        let mut package = Self {
            manifest,
            index: None,
            resources,
            examples,
            resources_by_id: HashMap::new(),
            resources_by_url: HashMap::new(),
            resources_by_type: HashMap::new(),
        };

        package.build_indices();
        package
    }

    /// Load package from tar.gz reader.
    pub fn from_tar_gz<R: Read>(mut reader: R) -> PackageResult<Self> {
        let mut decoder = GzDecoder::new(&mut reader);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;

        let mut archive = Archive::new(std::io::Cursor::new(decompressed));
        let mut file_map: HashMap<String, Vec<u8>> = HashMap::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
            file_map.insert(path, contents);
        }

        let manifest_path = "package/package.json";
        let manifest = file_map
            .get(manifest_path)
            .ok_or_else(|| PackageError::MissingFile(manifest_path.to_string()))
            .and_then(|bytes| Self::parse_json::<PackageManifest>(bytes))?;

        let index = file_map
            .get("package/.index.json")
            .and_then(|bytes| Self::parse_json::<PackageIndex>(bytes).ok());

        let resources = Self::load_resources_from_map(
            &file_map,
            "package/",
            &[manifest_path, "package/.index.json"],
        )?;
        let examples = Self::load_resources_from_map(&file_map, "package/examples/", &[])?;

        let mut package = Self {
            manifest,
            index,
            resources,
            examples,
            resources_by_id: HashMap::new(),
            resources_by_url: HashMap::new(),
            resources_by_type: HashMap::new(),
        };

        package.build_indices();
        Ok(package)
    }

    /// Load package from tar.gz bytes.
    pub fn from_tar_gz_bytes(bytes: &[u8]) -> PackageResult<Self> {
        Self::from_tar_gz(std::io::Cursor::new(bytes))
    }

    /// Load package from directory.
    pub fn from_directory(package_dir: &Path) -> PackageResult<Self> {
        let manifest_path = package_dir.join("package.json");
        if !manifest_path.exists() {
            return Err(PackageError::MissingFile(
                manifest_path.to_string_lossy().into(),
            ));
        }

        let manifest = Self::parse_json::<PackageManifest>(&fs::read(manifest_path)?)?;

        let index = package_dir
            .join(".index.json")
            .exists()
            .then(|| package_dir.join(".index.json"))
            .and_then(|p| fs::read(p).ok())
            .and_then(|bytes| Self::parse_json::<PackageIndex>(&bytes).ok());

        let resources =
            Self::load_resources_from_dir(package_dir, &["package.json", ".index.json"])?;
        let examples = package_dir
            .join("examples")
            .exists()
            .then(|| Self::load_resources_from_dir(&package_dir.join("examples"), &[]))
            .transpose()?
            .unwrap_or_default();

        let mut package = Self {
            manifest,
            index,
            resources,
            examples,
            resources_by_id: HashMap::new(),
            resources_by_url: HashMap::new(),
            resources_by_type: HashMap::new(),
        };

        package.build_indices();
        Ok(package)
    }

    pub fn all_resources(&self) -> (&[Value], &[Value]) {
        (&self.resources, &self.examples)
    }

    pub fn conformance_resources(&self) -> &[Value] {
        &self.resources
    }

    pub fn example_resources(&self) -> &[Value] {
        &self.examples
    }

    pub fn all_resources_combined(&self) -> Vec<&Value> {
        self.resources.iter().chain(self.examples.iter()).collect()
    }

    pub fn resources_by_type(&self, resource_type: &str) -> (Vec<&Value>, Vec<&Value>) {
        let filter =
            |r: &&Value| r.get("resourceType").and_then(Value::as_str) == Some(resource_type);
        (
            self.resources.iter().filter(filter).collect(),
            self.examples.iter().filter(filter).collect(),
        )
    }

    pub fn resource_by_id(&self, id: &str) -> Option<&Value> {
        self.resources_by_id.get(id)
    }

    pub fn resource_by_url(&self, url: &str) -> Option<&Value> {
        self.resources_by_url.get(url)
    }

    pub fn resources_of_type(&self, resource_type: &str) -> Option<&[Value]> {
        self.resources_by_type
            .get(resource_type)
            .map(|v| v.as_slice())
    }

    /// Build indices from resources for fast lookups
    fn build_indices(&mut self) {
        let resources: Vec<Value> = self.resources.clone();
        let examples: Vec<Value> = self.examples.clone();

        for resource in resources {
            self.index_resource(resource);
        }
        for resource in examples {
            self.index_resource(resource);
        }
    }

    /// Index a single resource by ID, URL, and type
    fn index_resource(&mut self, resource: Value) {
        if let Some(resource_type) = resource.get("resourceType").and_then(Value::as_str) {
            // Index by type
            self.resources_by_type
                .entry(resource_type.to_string())
                .or_default()
                .push(resource.clone());

            // Index by ID
            if let Some(id) = resource.get("id").and_then(Value::as_str) {
                self.resources_by_id
                    .insert(id.to_string(), resource.clone());
            }

            // Index by canonical URL
            if let Some(url) = resource.get("url").and_then(Value::as_str) {
                self.resources_by_url.insert(url.to_string(), resource);
            }
        }
    }

    fn parse_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> PackageResult<T> {
        let cleaned = Self::clean_bytes(bytes)?;
        Ok(serde_json::from_str(&cleaned)?)
    }

    fn load_resources_from_map(
        file_map: &HashMap<String, Vec<u8>>,
        prefix: &str,
        exclude: &[&str],
    ) -> PackageResult<Vec<Value>> {
        file_map
            .iter()
            .filter(|(path, _)| {
                path.starts_with(prefix)
                    && path.ends_with(".json")
                    && !exclude.contains(&path.as_str())
            })
            .map(|(_, contents)| Self::parse_json(contents))
            .collect()
    }

    fn load_resources_from_dir(dir: &Path, exclude: &[&str]) -> PackageResult<Vec<Value>> {
        let mut resources = Vec::new();
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension() == Some("json".as_ref()) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !exclude.contains(&name) {
                        resources.push(Self::parse_json(&fs::read(&path)?)?);
                    }
                }
            }
        }
        Ok(resources)
    }

    fn clean_bytes(bytes: &[u8]) -> PackageResult<String> {
        let bytes = if bytes.len() >= 3 && &bytes[..3] == b"\xEF\xBB\xBF" {
            &bytes[3..]
        } else {
            bytes
        };

        let content = String::from_utf8(bytes.to_vec())
            .map_err(|e| PackageError::InvalidStructure(format!("Invalid UTF-8: {}", e)))?;

        Ok(content
            .chars()
            .filter(|&c| matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\x7F'))
            .collect::<String>()
            .trim()
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn manifest_matches_spec_example() {
        let manifest_json = json!({
            "name": "hl7.fhir.us.acme",
            "version": "0.1.0",
            "canonical": "http://hl7.org/fhir/us/acme",
            "url": "http://hl7.org/fhir/us/acme/Draft1",
            "title": "ACME project IG",
            "description": "Describes how the ACME project uses FHIR for it's primary API",
            "fhirVersions": ["3.0.0"],
            "dependencies": {
                "hl7.fhir.core": "3.0.0",
                "hl7.fhir.us.core": "1.1.0"
            },
            "keywords": ["us", "United States", "ACME"],
            "author": "hl7",
            "maintainers": [
                { "name": "US Steering Committee", "email": "ussc@lists.hl7.com" }
            ],
            "jurisdiction": "http://unstats.un.org/unsd/methods/m49/m49.htm#001",
            "license": "CC0-1.0"
        });

        let manifest: PackageManifest =
            serde_json::from_value(manifest_json.clone()).expect("deserializes");

        assert_eq!(manifest.name, "hl7.fhir.us.acme");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(
            manifest.description,
            manifest_json["description"].as_str().unwrap()
        );
        assert_eq!(
            manifest.dependencies.get("hl7.fhir.core"),
            Some(&"3.0.0".to_string())
        );

        let round_trip = serde_json::to_value(&manifest).expect("serializes");
        assert_eq!(round_trip["name"], manifest_json["name"]);
        assert_eq!(round_trip["version"], manifest_json["version"]);
        assert_eq!(round_trip["dependencies"], manifest_json["dependencies"]);
    }

    #[test]
    fn index_round_trips() {
        let index_json = json!({
            "index-version": 1,
            "files": [
                {
                    "filename": "StructureDefinition-patient.json",
                    "resourceType": "StructureDefinition",
                    "id": "patient",
                    "url": "http://hl7.org/fhir/StructureDefinition/Patient",
                    "version": "5.0.0",
                    "kind": "resource",
                    "type": "Patient"
                }
            ]
        });

        let index: PackageIndex = serde_json::from_value(index_json.clone()).expect("deserializes");

        assert_eq!(index.index_version, 1);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].resource_type, "StructureDefinition");

        let round_trip = serde_json::to_value(&index).expect("serializes");
        assert_eq!(round_trip, index_json);
    }

    #[test]
    fn manifest_from_submodule_case_new_format() {
        let raw = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fhir-test-cases/npm/test.format.new/package/package.json"
        ));
        let raw = raw.trim_start_matches('\u{feff}');
        let manifest: PackageManifest =
            serde_json::from_str(raw).expect("deserializes case manifest");

        assert_eq!(manifest.name, "hl7.fhir.pubpack");
        assert_eq!(manifest.version, "0.0.2");
        assert_eq!(manifest.package_type, Some(PackageType::Tool));
        assert_eq!(manifest.fhir_versions, vec!["4.1".to_string()]);
        assert_eq!(manifest.author, "FHIR Project");
        assert_eq!(manifest.license.as_deref(), Some("CC0-1.0"));

        // Unknown fields from the manifest should be preserved
        assert_eq!(manifest.extra.get("tools-version"), Some(&Value::from(3)));
    }

    #[test]
    fn load_package_from_tar_gz() {
        let tar_gz_bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fhir-test-cases/npm/test.format.new.tgz"
        ));

        let package =
            FhirPackage::from_tar_gz_bytes(tar_gz_bytes).expect("should load package from tar.gz");

        // Verify manifest
        assert_eq!(package.manifest.name, "hl7.fhir.pubpack");
        assert_eq!(package.manifest.version, "0.0.2");
        assert_eq!(package.manifest.package_type, Some(PackageType::Tool));

        // Verify index is loaded
        assert!(package.index.is_some());
        let index = package.index.as_ref().unwrap();
        assert_eq!(index.index_version, 1);
        // Note: index.files may be empty if the package doesn't populate it

        // Verify resources are loaded (should have StructureDefinition-Definition.json)
        assert!(!package.resources.is_empty());
        let has_structure_def = package
            .resources
            .iter()
            .any(|r| r.get("resourceType").and_then(|v| v.as_str()) == Some("StructureDefinition"));
        assert!(has_structure_def);

        // Verify examples are separate (should be empty for this test package)
        // Examples would be in package/examples/ folder if present
        assert_eq!(package.examples.len(), 0);

        // Verify we can get resources by type
        let (conformance, examples) = package.resources_by_type("StructureDefinition");
        assert!(!conformance.is_empty());
        assert_eq!(examples.len(), 0);

        // Verify separation of examples from non-examples
        let (resources, examples) = package.all_resources();
        assert!(!resources.is_empty());
        assert_eq!(examples.len(), 0);
    }

    #[test]
    fn test_validate_version_format() {
        // Valid versions
        assert!(validate_version_format("1.2.3").is_ok());
        assert!(validate_version_format("1.2.3-release").is_ok());
        assert!(validate_version_format("1.2").is_ok());
        assert!(validate_version_format("0.1.0").is_ok());
        assert!(validate_version_format("5.0.0-ballot").is_ok());
        assert!(validate_version_format("abc.def").is_ok());
        assert!(validate_version_format("abc_def").is_ok()); // Non-numeric can use underscores
                                                             // Versions starting with digits must use dots for SemVer format
        assert!(validate_version_format("1_2_3").is_err()); // Should use dots, not underscores

        // Invalid versions
        assert!(validate_version_format("").is_err());
        assert!(validate_version_format("1.2.3@beta").is_err()); // @ not allowed
        assert!(validate_version_format("1.2.3+metadata").is_err()); // + not allowed
        assert!(validate_version_format("1.2.3 ").is_err()); // space not allowed
    }

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("1.2.3"), ("1.2.3".to_string(), None));
        assert_eq!(
            parse_version("1.2.3-release"),
            ("1.2.3".to_string(), Some("release".to_string()))
        );
        assert_eq!(
            parse_version("5.0.0-ballot"),
            ("5.0.0".to_string(), Some("ballot".to_string()))
        );
    }

    #[test]
    fn test_compare_versions() {
        use std::cmp::Ordering;

        // Numeric comparison
        assert_eq!(compare_versions("1.2.3", "1.2.4"), Ordering::Less);
        assert_eq!(compare_versions("1.2.4", "1.2.3"), Ordering::Greater);
        assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.3", "1.3.0"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);

        // Labels are ignored
        assert_eq!(compare_versions("1.2.3", "1.2.3-release"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.3-ballot", "1.2.4"), Ordering::Less);
    }

    #[test]
    fn test_version_matches() {
        // Exact matches
        assert!(version_matches("1.2.3", "1.2.3"));
        assert!(version_matches("1.2.3-release", "1.2.3-release"));

        // Patch wildcard
        assert!(version_matches("1.2.0", "1.2.x"));
        assert!(version_matches("1.2.1", "1.2.x"));
        assert!(version_matches("1.2.99", "1.2.x"));
        assert!(!version_matches("1.3.0", "1.2.x"));
        assert!(!version_matches("2.0.0", "1.2.x"));

        // Label preference (version without label matches reference without label)
        assert!(version_matches("1.2.3", "1.2.3"));
        assert!(version_matches("1.2.3-release", "1.2.3")); // Labeled version matches unlabeled reference
    }
}
