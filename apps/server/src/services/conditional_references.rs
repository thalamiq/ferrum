//! Conditional reference resolution for FHIR `Reference.reference`.
//!
//! Supports search-URI references like `Patient?identifier=...` by resolving them to `Patient/{id}`.

use crate::db::search::engine::SearchEngine;
use crate::services::conditional::{
    build_conditional_search_params_from_items, extract_match_id, extract_match_resource_type,
    parse_form_urlencoded,
};
use crate::Result;
use serde_json::Value as JsonValue;
use sqlx::PgConnection;
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

pub async fn resolve_conditional_references(
    search_engine: &SearchEngine,
    resource: &mut JsonValue,
    base_url: Option<&str>,
) -> Result<()> {
    let mut occurrences = Vec::new();
    collect_conditional_reference_occurrences(resource, &mut Vec::new(), &mut occurrences);
    if occurrences.is_empty() {
        return Ok(());
    }

    let mut cache: HashMap<String, String> = HashMap::new();
    for occ in occurrences {
        let replacement = if let Some(replacement) = cache.get(&occ.raw) {
            replacement.clone()
        } else {
            let resolved =
                resolve_conditional_reference_search_uri(search_engine, &occ.raw, base_url).await?;
            cache.insert(occ.raw.clone(), resolved.clone());
            resolved
        };

        let Some(slot) = json_value_at_path_mut(resource, &occ.path) else {
            return Err(crate::Error::Internal(
                "Failed to apply resolved conditional reference".to_string(),
            ));
        };
        *slot = JsonValue::String(replacement);
    }

    Ok(())
}

pub async fn resolve_conditional_references_with_connection(
    search_engine: &SearchEngine,
    conn: &mut PgConnection,
    resource: &mut JsonValue,
    base_url: Option<&str>,
) -> Result<()> {
    let mut occurrences = Vec::new();
    collect_conditional_reference_occurrences(resource, &mut Vec::new(), &mut occurrences);
    if occurrences.is_empty() {
        return Ok(());
    }

    let mut cache: HashMap<String, String> = HashMap::new();
    for occ in occurrences {
        let replacement = if let Some(replacement) = cache.get(&occ.raw) {
            replacement.clone()
        } else {
            let resolved = resolve_conditional_reference_search_uri_with_connection(
                search_engine,
                conn,
                &occ.raw,
                base_url,
            )
            .await?;
            cache.insert(occ.raw.clone(), resolved.clone());
            resolved
        };

        let Some(slot) = json_value_at_path_mut(resource, &occ.path) else {
            return Err(crate::Error::Internal(
                "Failed to apply resolved conditional reference".to_string(),
            ));
        };
        *slot = JsonValue::String(replacement);
    }

    Ok(())
}

async fn resolve_conditional_reference_search_uri(
    search_engine: &SearchEngine,
    raw_reference: &str,
    base_url: Option<&str>,
) -> Result<String> {
    let parsed = parse_conditional_reference_search_uri(raw_reference)?;
    validate_conditional_reference_query_items(&parsed.query_items)?;

    let search_params = build_conditional_search_params_from_items(&parsed.query_items)?;
    let search_result = search_engine
        .search(Some(&parsed.resource_type), &search_params, base_url)
        .await?;

    validate_search_result_params(raw_reference, &search_result.unknown_params)?;
    resolved_reference_from_search_result(raw_reference, &parsed, &search_result.resources)
}

async fn resolve_conditional_reference_search_uri_with_connection(
    search_engine: &SearchEngine,
    conn: &mut PgConnection,
    raw_reference: &str,
    base_url: Option<&str>,
) -> Result<String> {
    let parsed = parse_conditional_reference_search_uri(raw_reference)?;
    validate_conditional_reference_query_items(&parsed.query_items)?;

    let search_params = build_conditional_search_params_from_items(&parsed.query_items)?;
    let search_result = search_engine
        .search_with_connection(conn, Some(&parsed.resource_type), &search_params, base_url)
        .await?;

    validate_search_result_params(raw_reference, &search_result.unknown_params)?;
    resolved_reference_from_search_result(raw_reference, &parsed, &search_result.resources)
}

fn validate_search_result_params(raw_reference: &str, unknown_params: &[String]) -> Result<()> {
    if unknown_params.is_empty() {
        return Ok(());
    }
    Err(crate::Error::Validation(format!(
        "Unknown or unsupported search parameters in conditional reference '{}': {}",
        raw_reference,
        unknown_params.join(", ")
    )))
}

fn resolved_reference_from_search_result(
    raw_reference: &str,
    parsed: &ParsedConditionalReference,
    resources: &[serde_json::Value],
) -> Result<String> {
    let matched = match resources.len() {
        0 => {
            return Err(crate::Error::PreconditionFailed(format!(
                "Conditional reference '{}' did not match any resources",
                raw_reference
            )));
        }
        1 => &resources[0],
        _ => {
            return Err(crate::Error::PreconditionFailed(format!(
                "Conditional reference '{}' matched multiple resources",
                raw_reference
            )));
        }
    };

    let id = extract_match_id(matched)?;
    let rt = extract_match_resource_type(matched)?;
    let mut resolved = format!("{}/{}", rt, id);
    if let Some(fragment) = &parsed.fragment {
        resolved.push('#');
        resolved.push_str(fragment);
    }
    Ok(resolved)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonPathSegment {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone)]
struct ConditionalReferenceOccurrence {
    raw: String,
    path: Vec<JsonPathSegment>,
}

#[derive(Debug, Clone)]
struct ParsedConditionalReference {
    resource_type: String,
    query_items: Vec<(String, String)>,
    fragment: Option<String>,
}

fn collect_conditional_reference_occurrences(
    value: &JsonValue,
    path: &mut Vec<JsonPathSegment>,
    out: &mut Vec<ConditionalReferenceOccurrence>,
) {
    match value {
        JsonValue::Object(map) => {
            if let Some(JsonValue::String(reference)) = map.get("reference") {
                if reference.contains('?') {
                    let mut p = path.clone();
                    p.push(JsonPathSegment::Key("reference".to_string()));
                    out.push(ConditionalReferenceOccurrence {
                        raw: reference.clone(),
                        path: p,
                    });
                }
            }

            for (k, v) in map {
                path.push(JsonPathSegment::Key(k.clone()));
                collect_conditional_reference_occurrences(v, path, out);
                path.pop();
            }
        }
        JsonValue::Array(arr) => {
            for (idx, v) in arr.iter().enumerate() {
                path.push(JsonPathSegment::Index(idx));
                collect_conditional_reference_occurrences(v, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn json_value_at_path_mut<'a>(
    value: &'a mut JsonValue,
    path: &[JsonPathSegment],
) -> Option<&'a mut JsonValue> {
    let mut current = value;
    for seg in path {
        match seg {
            JsonPathSegment::Key(k) => {
                let JsonValue::Object(map) = current else {
                    return None;
                };
                current = map.get_mut(k)?;
            }
            JsonPathSegment::Index(i) => {
                let JsonValue::Array(arr) = current else {
                    return None;
                };
                current = arr.get_mut(*i)?;
            }
        }
    }
    Some(current)
}

fn parse_conditional_reference_search_uri(raw: &str) -> Result<ParsedConditionalReference> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(crate::Error::InvalidReference(
            "Empty conditional reference".to_string(),
        ));
    }

    let (raw, fragment) = raw
        .split_once('#')
        .map(|(b, f)| (b, Some(f.to_string())))
        .unwrap_or((raw, None));

    let raw_lower = raw.to_ascii_lowercase();
    let (resource_type, query) = if raw_lower.starts_with("http://")
        || raw_lower.starts_with("https://")
    {
        let url = Url::parse(raw).map_err(|e| {
            crate::Error::InvalidReference(format!(
                "Invalid absolute conditional reference URL: {e}"
            ))
        })?;
        let rt = url
            .path()
            .trim_matches('/')
            .split('/')
            .rfind(|s| !s.is_empty())
            .ok_or_else(|| {
                crate::Error::InvalidReference(
                    "Conditional reference must include a resource type path segment".to_string(),
                )
            })?
            .to_string();
        let query = url.query().ok_or_else(|| {
            crate::Error::InvalidReference(
                "Conditional reference must include a query string".to_string(),
            )
        })?;
        (rt, query.to_string())
    } else {
        let (path, query) = raw.split_once('?').ok_or_else(|| {
            crate::Error::InvalidReference(
                "Conditional reference must be of the form '{type}?{criteria}'".to_string(),
            )
        })?;
        let rt = path
            .trim_matches('/')
            .split('/')
            .rfind(|s| !s.is_empty())
            .ok_or_else(|| {
                crate::Error::InvalidReference(
                    "Conditional reference must include a resource type".to_string(),
                )
            })?
            .to_string();
        (rt, query.to_string())
    };

    if !is_valid_resource_type_name(&resource_type) {
        return Err(crate::Error::InvalidReference(format!(
            "Invalid resource type in conditional reference: {}",
            resource_type
        )));
    }

    let query_items = parse_form_urlencoded(&query)?;
    if query_items.is_empty() {
        return Err(crate::Error::InvalidReference(
            "Conditional reference query must not be empty".to_string(),
        ));
    }

    Ok(ParsedConditionalReference {
        resource_type,
        query_items,
        fragment,
    })
}

fn validate_conditional_reference_query_items(items: &[(String, String)]) -> Result<()> {
    const DISALLOWED: &[&str] = &[
        "_count",
        "_offset",
        "_sort",
        "_include",
        "_revinclude",
        "_summary",
        "_elements",
        "_format",
        "_pretty",
        "_total",
        "_cursor",
        "_cursor_direction",
        "_maxresults",
        "_type",
    ];

    for (k, _) in items {
        if DISALLOWED.contains(&k.as_str())
            || k.starts_with("_include:")
            || k.starts_with("_revinclude:")
        {
            return Err(crate::Error::Validation(format!(
                "Conditional reference does not allow result/control parameter '{}'",
                k
            )));
        }
    }
    Ok(())
}

fn is_valid_resource_type_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.chars().next().unwrap();
    first.is_ascii_uppercase() && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Service wrapper for resolving conditional `Reference.reference` search URIs.
#[derive(Clone)]
pub struct ConditionalReferenceResolver {
    search_engine: Arc<SearchEngine>,
}

impl ConditionalReferenceResolver {
    pub fn new(search_engine: Arc<SearchEngine>) -> Self {
        Self { search_engine }
    }

    pub async fn resolve(&self, resource: &mut JsonValue, base_url: Option<&str>) -> Result<()> {
        resolve_conditional_references(self.search_engine.as_ref(), resource, base_url).await
    }
}
