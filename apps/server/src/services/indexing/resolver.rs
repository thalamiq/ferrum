//! FHIRPath `ResourceResolver` used by search indexing.
//!
//! Indexing needs `resolve()` to work for common SearchParameter expressions like:
//! `subject.where(resolve() is Patient)`.
//!
//! The upstream FHIRPath VM only resolves:
//! - contained references (`#id`) from the current resource, and
//! - external references via a provided `ResourceResolver`.
//!
//! For indexing we keep `resolve()` safe and fast:
//! - First, optionally resolve from a small cache pre-warmed from Postgres.
//! - If not cached (or DB resolution is disabled), fall back to a lightweight stub
//!   resource based on the reference string (type-only shortcut).
//! - Support transaction `Bundle.entry.fullUrl` references via a seeded mapping.

use crate::db::search::query_builder::{parse_reference_query_value, ParsedReferenceQuery};
use lru::LruCache;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use ferrum_fhirpath::resolver::ResourceResolver;
use ferrum_fhirpath::Value;

#[derive(Debug)]
pub(crate) struct IndexingResourceResolver {
    pool: Option<PgPool>,
    resolved: Mutex<LruCache<String, Option<JsonValue>>>,
    full_url_mapping: Mutex<LruCache<String, String>>,
}

impl IndexingResourceResolver {
    pub(crate) fn new_with_pool(pool: PgPool, cache_size: usize) -> Self {
        Self {
            pool: Some(pool),
            resolved: Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1024).unwrap()),
            )),
            full_url_mapping: Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1024).unwrap()),
            )),
        }
    }

    pub(crate) fn new_stub(cache_size: usize) -> Self {
        Self {
            pool: None,
            resolved: Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1024).unwrap()),
            )),
            full_url_mapping: Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1024).unwrap()),
            )),
        }
    }

    pub(crate) fn seed_full_url_mapping<I>(&self, mapping: I)
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut cache = self.full_url_mapping.lock().unwrap();
        for (from, to) in mapping {
            cache.put(from, to);
        }
    }

    pub(crate) async fn prewarm_cache_for_resource(
        &self,
        resource: &JsonValue,
    ) -> crate::Result<()> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };

        let references = extract_all_references(resource);
        for reference in references {
            let normalized = self.normalize_reference(&reference);
            let cache_key = normalized.as_deref().unwrap_or(reference.as_str());

            // Avoid re-querying when we've already cached an entry.
            {
                let cache = self.resolved.lock().unwrap();
                if cache.peek(cache_key).is_some() {
                    continue;
                }
            }

            let parsed = parse_reference_query_value(cache_key, None);
            let Some((resource_type, id, version)) = parsed.and_then(parsed_resource_identity)
            else {
                // Unknown reference format (e.g., urn:uuid); rely on stub fallback.
                continue;
            };

            let result = if let Some(version_id) = version.and_then(|v| v.parse::<i32>().ok()) {
                sqlx::query_scalar::<_, JsonValue>(
                    r#"
                    SELECT resource
                    FROM resource_history
                    WHERE resource_type = $1 AND id = $2 AND version_id = $3
                    "#,
                )
                .bind(resource_type)
                .bind(id)
                .bind(version_id)
                .fetch_optional(pool)
                .await
                .map_err(crate::Error::Database)?
            } else {
                sqlx::query_scalar::<_, JsonValue>(
                    r#"
                    SELECT resource
                    FROM resources
                    WHERE resource_type = $1 AND id = $2 AND deleted = false
                    "#,
                )
                .bind(resource_type)
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(crate::Error::Database)?
            };

            let mut cache = self.resolved.lock().unwrap();
            cache.put(cache_key.to_string(), result);
        }

        Ok(())
    }

    fn normalize_reference(&self, reference: &str) -> Option<String> {
        // Exact match.
        {
            let mut map = self.full_url_mapping.lock().unwrap();
            if let Some(to) = map.get(reference) {
                return Some(to.clone());
            }
        }

        // Fragment-aware: "urn:uuid:...#foo" -> "Patient/123#foo"
        if let Some((base, frag)) = reference.split_once('#') {
            let mut map = self.full_url_mapping.lock().unwrap();
            if let Some(to) = map.get(base) {
                return Some(format!("{}#{}", to, frag));
            }
        }

        None
    }
}

impl ResourceResolver for IndexingResourceResolver {
    fn resolve(&self, reference: &str) -> ferrum_fhirpath::Result<Option<Value>> {
        let normalized = self.normalize_reference(reference);
        let cache_key = normalized.as_deref().unwrap_or(reference);

        // 1) Try cached full resolution.
        {
            let mut cache = self.resolved.lock().unwrap();
            if let Some(result) = cache.get(cache_key) {
                return Ok(result.as_ref().map(|json| Value::from_json(json.clone())));
            }
        }

        // 2) Fallback: lightweight "type-only" stub so expressions like
        //    `resolve() is Patient` can be evaluated without a DB lookup.
        let Some(parsed) = parse_reference_query_value(cache_key, None) else {
            return Ok(None);
        };

        let (resource_type, id_opt) = match parsed {
            ParsedReferenceQuery::Relative {
                typ: Some(typ), id, ..
            } => (typ, Some(id)),
            ParsedReferenceQuery::Absolute {
                typ: Some(typ), id, ..
            } => (typ, id),
            _ => return Ok(None),
        };

        let mut obj = serde_json::Map::new();
        obj.insert("resourceType".to_string(), JsonValue::String(resource_type));
        if let Some(id) = id_opt {
            obj.insert("id".to_string(), JsonValue::String(id));
        }

        Ok(Some(Value::from_json(JsonValue::Object(obj))))
    }
}

fn parsed_resource_identity(
    parsed: ParsedReferenceQuery,
) -> Option<(String, String, Option<String>)> {
    match parsed {
        ParsedReferenceQuery::Relative { typ, id, version } => Some((typ?, id, version)),
        ParsedReferenceQuery::Absolute {
            is_local: _,
            typ,
            id,
            version,
            ..
        } => Some((typ?, id?, version)),
        _ => None,
    }
}

fn extract_all_references(value: &JsonValue) -> Vec<String> {
    let mut out = Vec::new();
    extract_references_recursive(value, &mut out);
    out
}

fn extract_references_recursive(value: &JsonValue, out: &mut Vec<String>) {
    match value {
        JsonValue::Object(map) => {
            if let Some(JsonValue::String(reference)) = map.get("reference") {
                if !reference.starts_with('#') {
                    out.push(reference.clone());
                }
            }

            for v in map.values() {
                extract_references_recursive(v, out);
            }
        }
        JsonValue::Array(arr) => {
            for v in arr {
                extract_references_recursive(v, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_fhirpath::conversion::ToJson;

    #[test]
    fn resolves_type_stub_for_relative_reference() {
        let resolver = IndexingResourceResolver::new_stub(16);
        let value = resolver.resolve("Patient/123").unwrap().unwrap();
        let json = value.to_json().unwrap();
        assert_eq!(
            json.get("resourceType").and_then(|v| v.as_str()),
            Some("Patient")
        );
        assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("123"));
    }

    #[test]
    fn resolves_type_stub_for_absolute_reference() {
        let resolver = IndexingResourceResolver::new_stub(16);
        let value = resolver
            .resolve("https://example.org/fhir/Observation/obs1")
            .unwrap()
            .unwrap();
        let json = value.to_json().unwrap();
        assert_eq!(
            json.get("resourceType").and_then(|v| v.as_str()),
            Some("Observation")
        );
        assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("obs1"));
    }

    #[test]
    fn resolves_via_seeded_full_url_mapping() {
        let resolver = IndexingResourceResolver::new_stub(16);
        resolver.seed_full_url_mapping([("urn:uuid:abc".to_string(), "Patient/xyz".to_string())]);

        let value = resolver.resolve("urn:uuid:abc").unwrap().unwrap();
        let json = value.to_json().unwrap();
        assert_eq!(
            json.get("resourceType").and_then(|v| v.as_str()),
            Some("Patient")
        );
        assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("xyz"));
    }
}
