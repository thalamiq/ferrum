use crate::generator::{generate_deep_snapshot, generate_snapshot};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use zunder_context::{Error, FhirContext, Result};
use zunder_models::StructureDefinition;

#[derive(Clone, Debug)]
struct SdCacheKey {
    url: String,
    version: Option<String>,
}

impl PartialEq for SdCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url && self.version == other.version
    }
}

impl Eq for SdCacheKey {}

impl Hash for SdCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url.hash(state);
        self.version.hash(state);
    }
}

/// A [`FhirContext`] wrapper that guarantees `StructureDefinition.snapshot` exists and is deep-expanded.
///
/// This wrapper provides enhanced context functionality by:
/// - Materializing snapshots from differentials (via `baseDefinition`) using `fhir_snapshot::generate_snapshot`
/// - Deep-expanding snapshots for nested type validation using `fhir_snapshot::generate_deep_snapshot`
/// - Caching expanded StructureDefinitions by `(url, version)` for reuse across validation runs
///
/// While this type is defined in `fhir-snapshot`, it semantically belongs to the context layer
/// and implements the [`FhirContext`] trait. It's kept here to avoid circular dependencies
/// (since it needs snapshot generation functions from this crate).
pub struct ExpandedFhirContext<C: FhirContext> {
    inner: C,
    materialized: RwLock<HashMap<SdCacheKey, Arc<StructureDefinition>>>,
    expanded: RwLock<HashMap<SdCacheKey, Arc<StructureDefinition>>>,
}

impl<C: FhirContext> ExpandedFhirContext<C> {
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            materialized: RwLock::new(HashMap::new()),
            expanded: RwLock::new(HashMap::new()),
        }
    }

    pub fn inner(&self) -> &C {
        &self.inner
    }

    pub fn into_inner(self) -> C {
        self.inner
    }

    fn parse_structure_definition(resource: Arc<Value>) -> Result<StructureDefinition> {
        Ok(serde_json::from_value(Arc::unwrap_or_clone(resource))?)
    }

    fn key_for(url: &str, sd: &StructureDefinition) -> SdCacheKey {
        SdCacheKey {
            url: url.to_string(),
            version: sd.version.clone(),
        }
    }

    fn get_raw_structure_definition(
        &self,
        canonical_url: &str,
    ) -> Result<Option<StructureDefinition>> {
        let Some(resource) = self.inner.get_latest_resource_by_url(canonical_url)? else {
            return Ok(None);
        };
        Ok(Some(Self::parse_structure_definition(resource)?))
    }

    fn get_or_build_materialized(
        &self,
        canonical_url: &str,
        stack: &mut HashSet<String>,
    ) -> Result<Option<Arc<StructureDefinition>>> {
        if stack.contains(canonical_url) {
            return Err(Error::InvalidStructureDefinition(format!(
                "Circular baseDefinition reference while materializing {}",
                canonical_url
            )));
        }

        let Some(sd) = self.get_raw_structure_definition(canonical_url)? else {
            return Ok(None);
        };

        let key = Self::key_for(canonical_url, &sd);
        if let Some(hit) = self
            .materialized
            .read()
            .ok()
            .and_then(|m| m.get(&key).cloned())
        {
            return Ok(Some(hit));
        }

        stack.insert(canonical_url.to_string());

        let result = (|| -> Result<StructureDefinition> {
            // Already has snapshot
            if sd.snapshot.is_some() {
                return Ok(sd);
            }

            let differential = sd.differential.as_ref().ok_or_else(|| {
                Error::InvalidStructureDefinition(format!(
                    "StructureDefinition {} missing both snapshot and differential",
                    canonical_url
                ))
            })?;

            let base_url = sd.base_definition.as_ref().ok_or_else(|| {
                Error::InvalidStructureDefinition(format!(
                    "StructureDefinition {} missing baseDefinition required to compute snapshot from differential",
                    canonical_url
                ))
            })?;

            let base_sd = self
                .get_or_build_materialized(base_url, stack)?
                .ok_or_else(|| Error::StructureDefinitionNotFound(base_url.clone()))?;

            let base_snapshot = base_sd.snapshot.as_ref().ok_or_else(|| {
                Error::InvalidStructureDefinition(format!(
                    "Base StructureDefinition {} missing snapshot after materialization",
                    base_url
                ))
            })?;

            let snapshot = generate_snapshot(base_snapshot, differential, self).map_err(|e| {
                Error::InvalidStructureDefinition(format!(
                    "Failed to generate snapshot for {}: {}",
                    canonical_url, e
                ))
            })?;

            let mut sd = sd;
            sd.snapshot = Some(snapshot);
            Ok(sd)
        })();

        stack.remove(canonical_url);

        let materialized = Arc::new(result?);
        if let Ok(mut m) = self.materialized.write() {
            m.insert(key, Arc::clone(&materialized));
        }
        Ok(Some(materialized))
    }

    fn get_or_build_expanded(
        &self,
        canonical_url: &str,
    ) -> Result<Option<Arc<StructureDefinition>>> {
        let Some(raw_sd) = self.get_raw_structure_definition(canonical_url)? else {
            return Ok(None);
        };

        let key = Self::key_for(canonical_url, &raw_sd);
        if let Some(hit) = self.expanded.read().ok().and_then(|m| m.get(&key).cloned()) {
            return Ok(Some(hit));
        }

        let mut stack = HashSet::new();
        let materialized = self
            .get_or_build_materialized(canonical_url, &mut stack)?
            .ok_or_else(|| Error::StructureDefinitionNotFound(canonical_url.to_string()))?;

        let snapshot = materialized.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidStructureDefinition(format!(
                "StructureDefinition {} missing snapshot after materialization",
                canonical_url
            ))
        })?;

        let deep = generate_deep_snapshot(snapshot, self).map_err(|e| {
            Error::InvalidStructureDefinition(format!(
                "Failed to deep-expand snapshot for {}: {}",
                canonical_url, e
            ))
        })?;

        let mut expanded_sd = (*materialized).clone();
        expanded_sd.snapshot = Some(deep);

        let expanded_sd = Arc::new(expanded_sd);
        if let Ok(mut m) = self.expanded.write() {
            m.insert(key, Arc::clone(&expanded_sd));
        }

        Ok(Some(expanded_sd))
    }
}

impl<C: FhirContext> FhirContext for ExpandedFhirContext<C> {
    fn get_resource_by_url(
        &self,
        canonical_url: &str,
        version: Option<&str>,
    ) -> Result<Option<Arc<Value>>> {
        self.inner.get_resource_by_url(canonical_url, version)
    }

    fn get_structure_definition(
        &self,
        canonical_url: &str,
    ) -> Result<Option<Arc<StructureDefinition>>> {
        self.get_or_build_expanded(canonical_url)
    }
}

/// Wraps a borrowed `&dyn FhirContext` so it can be used with [`ExpandedFhirContext`].
pub struct BorrowedFhirContext<'a>(pub &'a dyn FhirContext);

impl FhirContext for BorrowedFhirContext<'_> {
    fn get_resource_by_url(
        &self,
        canonical_url: &str,
        version: Option<&str>,
    ) -> Result<Option<Arc<Value>>> {
        self.0.get_resource_by_url(canonical_url, version)
    }
}

impl<'a> ExpandedFhirContext<BorrowedFhirContext<'a>> {
    pub fn borrowed(inner: &'a dyn FhirContext) -> Self {
        Self::new(BorrowedFhirContext(inner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct MockContext {
        by_url: HashMap<String, Arc<Value>>,
    }

    impl FhirContext for MockContext {
        fn get_resource_by_url(
            &self,
            canonical_url: &str,
            _version: Option<&str>,
        ) -> Result<Option<Arc<Value>>> {
            Ok(self.by_url.get(canonical_url).cloned())
        }
    }

    fn sd_human_name() -> Value {
        json!({
            "resourceType": "StructureDefinition",
            "url": "http://hl7.org/fhir/StructureDefinition/HumanName",
            "name": "HumanName",
            "status": "active",
            "kind": "complex-type",
            "abstract": false,
            "type": "HumanName",
            "snapshot": {
                "element": [
                    { "id": "HumanName", "path": "HumanName" },
                    { "id": "HumanName.given", "path": "HumanName.given", "min": 0, "max": "*", "type": [{ "code": "string" }] }
                ]
            }
        })
    }

    fn sd_patient_base() -> Value {
        json!({
            "resourceType": "StructureDefinition",
            "url": "http://hl7.org/fhir/StructureDefinition/Patient",
            "name": "Patient",
            "status": "active",
            "kind": "resource",
            "abstract": false,
            "type": "Patient",
            "snapshot": {
                "element": [
                    { "id": "Patient", "path": "Patient" },
                    { "id": "Patient.name", "path": "Patient.name", "min": 0, "max": "*", "type": [{ "code": "HumanName" }] }
                ]
            }
        })
    }

    fn sd_patient_profile_differential() -> Value {
        json!({
            "resourceType": "StructureDefinition",
            "url": "http://example.org/fhir/StructureDefinition/MyPatient",
            "name": "MyPatient",
            "status": "active",
            "kind": "resource",
            "abstract": false,
            "type": "Patient",
            "baseDefinition": "http://hl7.org/fhir/StructureDefinition/Patient",
            "derivation": "constraint",
            "differential": {
                "element": [
                    { "id": "Patient.birthDate", "path": "Patient.birthDate", "min": 1, "max": "1", "type": [{ "code": "date" }] }
                ]
            }
        })
    }

    #[test]
    fn expands_and_materializes_structure_definitions() {
        let mut by_url = HashMap::new();
        by_url.insert(
            "http://hl7.org/fhir/StructureDefinition/HumanName".to_string(),
            Arc::new(sd_human_name()),
        );
        by_url.insert(
            "http://hl7.org/fhir/StructureDefinition/Patient".to_string(),
            Arc::new(sd_patient_base()),
        );
        by_url.insert(
            "http://example.org/fhir/StructureDefinition/MyPatient".to_string(),
            Arc::new(sd_patient_profile_differential()),
        );

        let expanded = ExpandedFhirContext::new(MockContext { by_url });
        let sd = expanded
            .get_structure_definition("http://example.org/fhir/StructureDefinition/MyPatient")
            .unwrap()
            .unwrap();

        let snapshot = sd.snapshot.as_ref().unwrap();
        assert!(snapshot.get_element("Patient.birthDate").is_some());
        // Deep expansion should pull in HumanName.given under Patient.name.given
        assert!(snapshot.get_element("Patient.name.given").is_some());
    }
}
