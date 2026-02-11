//! Search service - FHIR search implementation
//!
//! Orchestrates search operations by:
//! - Building SQL queries via search engine
//! - Constructing FHIR Bundle responses
//! - Handling pagination, includes, and result totals

use crate::{
    db::search::engine::SearchEngine,
    db::search::params::{CursorDirection, SearchParameters},
    models::is_known_resource_type,
    runtime_config::{ConfigKey, RuntimeConfigCache},
    services::SummaryFilter,
    Result,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Search results from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Resources matching the search
    pub resources: Vec<JsonValue>,
    /// Total count of matching resources (if requested)
    pub total: Option<i64>,
    /// Included resources (_include, _revinclude)
    pub included: Vec<JsonValue>,
    /// Unknown/unsupported parameters that were ignored
    #[serde(skip)]
    pub unknown_params: Vec<String>,
}

/// Search service coordinates FHIR search operations
pub struct SearchService {
    search_engine: Arc<SearchEngine>,
    summary_filter: Option<Arc<SummaryFilter>>,
    runtime_config_cache: Arc<RuntimeConfigCache>,
}

impl SearchService {
    /// Create a new search service
    pub fn new(
        search_engine: Arc<SearchEngine>,
        runtime_config_cache: Arc<RuntimeConfigCache>,
    ) -> Self {
        Self {
            search_engine,
            summary_filter: None,
            runtime_config_cache,
        }
    }

    /// Create a new search service with summary filtering support
    pub fn with_summary_filter(
        search_engine: Arc<SearchEngine>,
        summary_filter: Arc<SummaryFilter>,
        runtime_config_cache: Arc<RuntimeConfigCache>,
    ) -> Self {
        Self {
            search_engine,
            summary_filter: Some(summary_filter),
            runtime_config_cache,
        }
    }

    /// Search for resources of a specific type
    ///
    /// GET/POST [base]/{resource_type}?params
    pub async fn search_type(
        &self,
        resource_type: &str,
        query_items: &[(String, String)],
        query_string: &str,
        base_url: &str,
    ) -> Result<JsonValue> {
        self.validate_resource_type_name(resource_type)?;

        let params = SearchParameters::from_items(query_items)?;
        let default_count: usize = self
            .runtime_config_cache
            .get(ConfigKey::SearchDefaultCount)
            .await;

        // Execute search via database engine
        let result = self
            .search_engine
            .search(Some(resource_type), &params, Some(base_url))
            .await?;

        // Build FHIR searchset Bundle
        self.build_searchset_bundle(
            result,
            resource_type,
            query_string,
            base_url,
            &params,
            default_count,
            query_items,
        )
    }

    /// Search across all resource types
    ///
    /// GET/POST [base]?params
    /// Requires _type parameter to specify which resource types to search
    pub async fn search_system(
        &self,
        query_items: &[(String, String)],
        query_string: &str,
        base_url: &str,
    ) -> Result<JsonValue> {
        let params = SearchParameters::from_items(query_items)?;
        let default_count: usize = self
            .runtime_config_cache
            .get(ConfigKey::SearchDefaultCount)
            .await;

        // Extract _type parameter to determine which resource types to search
        // If not specified, this is an error per FHIR spec
        if params.types.is_empty() {
            return Err(crate::Error::Validation(
                "System-level search requires _type parameter to specify resource types"
                    .to_string(),
            ));
        }

        self.validate_resource_types(&params.types)?;

        // Execute system-level search
        let result = self
            .search_engine
            .search(None, &params, Some(base_url))
            .await?;

        // Build FHIR searchset Bundle
        self.build_searchset_bundle(
            result,
            "",
            query_string,
            base_url,
            &params,
            default_count,
            query_items,
        )
    }

    /// Search within a compartment
    ///
    /// GET/POST [base]/{compartment_type}/{compartment_id}/[{resource_type}]?params
    pub async fn search_compartment(
        &self,
        compartment_type: &str,
        compartment_id: &str,
        resource_type: Option<&str>,
        query_items: &[(String, String)],
        query_string: &str,
        base_url: &str,
    ) -> Result<JsonValue> {
        self.validate_resource_type_name(compartment_type)?;
        if let Some(resource_type) = resource_type {
            self.validate_resource_type_name(resource_type)?;
        }

        let params = SearchParameters::from_items(query_items)?;
        let default_count: usize = self
            .runtime_config_cache
            .get(ConfigKey::SearchDefaultCount)
            .await;

        // Execute compartment search
        let result = self
            .search_engine
            .search_compartment(
                compartment_type,
                compartment_id,
                resource_type,
                &params,
                Some(base_url),
            )
            .await?;

        // Build FHIR searchset Bundle
        let search_path = if let Some(rt) = resource_type {
            format!("{}/{}/{}", compartment_type, compartment_id, rt)
        } else {
            // Per FHIR spec, all-types compartment searches use a literal `*` path segment.
            format!("{}/{}/{}", compartment_type, compartment_id, "*")
        };
        self.build_searchset_bundle(
            result,
            &search_path,
            query_string,
            base_url,
            &params,
            default_count,
            query_items,
        )
    }

    /// Build a FHIR searchset Bundle from search results
    ///
    /// Per FHIR spec (3.2.1.3), a searchset Bundle contains:
    /// - type: "searchset" (SHALL)
    /// - total: number of matching resources (if requested)
    /// - link: self (SHALL), first, next, prev, last (for pagination)
    /// - entry: array of matching resources with search metadata
    fn build_searchset_bundle(
        &self,
        result: SearchResult,
        resource_path: &str,
        query_string: &str,
        base_url: &str,
        params: &SearchParameters,
        default_count: usize,
        query_items: &[(String, String)],
    ) -> Result<JsonValue> {
        // Handle _summary=count mode - only return count, no entries.
        if matches!(
            params.summary,
            Some(crate::db::search::params::SummaryMode::Count)
        ) {
            let mut bundle = serde_json::json!({
                "resourceType": "Bundle",
                "type": "searchset",
                "link": [{
                    "relation": "self",
                    "url": if query_string.is_empty() {
                        format!("{}/{}", base_url, resource_path)
                    } else {
                        format!("{}/{}?{}", base_url, resource_path, query_string)
                    }
                }]
            });

            if let Some(total) = result.total {
                bundle["total"] = serde_json::json!(total);
            }

            if !result.unknown_params.is_empty() {
                bundle["_unknown_params"] = serde_json::json!(result.unknown_params);
            }

            return Ok(bundle);
        }

        // Apply _summary and _elements filtering (Search Result Parameters)
        // Per FHIR spec 3.2.1.7: _summary takes precedence over _elements
        let filtered_resources = if let Some(ref filter) = self.summary_filter {
            if let Some(summary_mode) = params.summary {
                // Apply _summary filtering
                result
                    .resources
                    .iter()
                    .map(|r| filter.filter_resource(r.clone(), summary_mode))
                    .collect::<Result<Vec<_>>>()?
            } else if !params.elements.is_empty() {
                // Apply _elements filtering
                result
                    .resources
                    .iter()
                    .map(|r| filter.filter_elements(r.clone(), &params.elements))
                    .collect::<Result<Vec<_>>>()?
            } else {
                result.resources.clone()
            }
        } else {
            result.resources.clone()
        };

        // Build entry array from resources
        let mut entries = Vec::new();

        // Add matching resources
        for resource in &filtered_resources {
            let resource_type = resource
                .get("resourceType")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let id = resource.get("id").and_then(|v| v.as_str()).unwrap_or("");

            entries.push(serde_json::json!({
                "fullUrl": format!("{}/{}/{}", base_url, resource_type, id),
                "resource": resource,
                "search": {
                    "mode": "match"
                }
            }));
        }

        // Add included resources
        // Note: many servers do not return includes for `_summary=text`.
        if !matches!(
            params.summary,
            Some(crate::db::search::params::SummaryMode::Text)
        ) {
            for resource in &result.included {
                let resource_type = resource
                    .get("resourceType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let id = resource.get("id").and_then(|v| v.as_str()).unwrap_or("");

                entries.push(serde_json::json!({
                    "fullUrl": format!("{}/{}/{}", base_url, resource_type, id),
                    "resource": resource,
                    "search": {
                        "mode": "include"
                    }
                }));
            }
        }

        // Build links (SHALL include self link as HTTP GET per spec 3.2.1.3.2)
        let mut links = Vec::new();

        // Self link (SHALL be present, expressed as HTTP GET)
        let self_url = if query_string.is_empty() {
            format!("{}/{}", base_url, resource_path)
        } else {
            format!("{}/{}?{}", base_url, resource_path, query_string)
        };
        links.push(serde_json::json!({
            "relation": "self",
            "url": self_url
        }));

        let count = params.effective_count_with_default(default_count);
        let has_paging_context =
            params.cursor.is_some() || params.cursor_direction == CursorDirection::Last;

        // First link (only when not on the initial page)
        if has_paging_context && !result.resources.is_empty() {
            let first_url = self.build_paging_url(base_url, resource_path, query_items, None, None);
            links.push(serde_json::json!({
                "relation": "first",
                "url": first_url
            }));
        }

        // Prev link (cursor-based pagination)
        // Best practice: omit on the initial page
        if has_paging_context && !result.resources.is_empty() {
            if let Some(first_resource) = result.resources.first() {
                if let (Some(last_updated), Some(id)) = (
                    first_resource
                        .get("meta")
                        .and_then(|m| m.get("lastUpdated"))
                        .and_then(|v| v.as_str()),
                    first_resource.get("id").and_then(|v| v.as_str()),
                ) {
                    let cursor = crate::db::search::query_builder::encode_cursor(last_updated, id);
                    let prev_url = self.build_paging_url(
                        base_url,
                        resource_path,
                        query_items,
                        Some(&cursor),
                        Some(CursorDirection::Prev),
                    );
                    links.push(serde_json::json!({
                        "relation": "prev",
                        "url": prev_url
                    }));
                }
            }
        }

        // Next link (cursor-based pagination)
        // Only add if we got a full page of results (indicates more may exist)
        if params.cursor_direction != CursorDirection::Last && result.resources.len() == count {
            if let Some(last_resource) = result.resources.last() {
                // Extract last_updated and id from the last resource
                if let (Some(last_updated), Some(id)) = (
                    last_resource
                        .get("meta")
                        .and_then(|m| m.get("lastUpdated"))
                        .and_then(|v| v.as_str()),
                    last_resource.get("id").and_then(|v| v.as_str()),
                ) {
                    let cursor = crate::db::search::query_builder::encode_cursor(last_updated, id);
                    let next_url = self.build_paging_url(
                        base_url,
                        resource_path,
                        query_items,
                        Some(&cursor),
                        None,
                    );
                    links.push(serde_json::json!({
                        "relation": "next",
                        "url": next_url
                    }));
                }
            }
        }

        // Last link (cursor-based pagination)
        if !result.resources.is_empty() && params.cursor_direction != CursorDirection::Last {
            let last_url = self.build_paging_url(
                base_url,
                resource_path,
                query_items,
                None,
                Some(CursorDirection::Last),
            );
            links.push(serde_json::json!({
                "relation": "last",
                "url": last_url
            }));
        }

        // Build Bundle (type SHALL be "searchset" per spec 3.2.1.3.1)
        // Order: resourceType, type, total, link, entry
        let mut bundle = serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset"
        });

        // Add total if available
        if let Some(total) = result.total {
            bundle["total"] = serde_json::json!(total);
        }

        // Add links
        bundle["link"] = serde_json::json!(links);

        // Add entries
        bundle["entry"] = serde_json::json!(entries);

        // Add unknown parameters as temporary metadata (will be checked and removed by handler)
        if !result.unknown_params.is_empty() {
            bundle["_unknown_params"] = serde_json::json!(result.unknown_params);
        }

        Ok(bundle)
    }

    /// Build a pagination URL with cursor
    /// Per FHIR spec: preserves _count and _maxresults in pagination links
    fn build_paging_url(
        &self,
        base_url: &str,
        resource_path: &str,
        query_items: &[(String, String)],
        cursor: Option<&str>,
        cursor_direction: Option<CursorDirection>,
    ) -> String {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());

        // Add all items except paging controls
        // Per spec: _count and _maxresults SHOULD be preserved across pages
        for (key, value) in query_items {
            if key != "_cursor" && key != "_offset" && key != "_cursor_direction" {
                serializer.append_pair(key, value);
            }
        }

        if let Some(cursor) = cursor {
            serializer.append_pair("_cursor", cursor);
        }

        if let Some(direction) = cursor_direction {
            if direction != CursorDirection::Next {
                serializer.append_pair("_cursor_direction", direction.as_str());
            }
        }

        let query = serializer.finish();
        if query.is_empty() {
            format!("{}/{}", base_url, resource_path)
        } else {
            format!("{}/{}?{}", base_url, resource_path, query)
        }
    }

    fn validate_resource_type_name(&self, resource_type: &str) -> Result<()> {
        if !is_known_resource_type(resource_type) {
            return Err(crate::Error::Validation(format!(
                "Invalid resource type: {}",
                resource_type
            )));
        }

        Ok(())
    }

    fn validate_resource_types(&self, resource_types: &[String]) -> Result<()> {
        for resource_type in resource_types {
            self.validate_resource_type_name(resource_type)?;
        }

        Ok(())
    }
}
