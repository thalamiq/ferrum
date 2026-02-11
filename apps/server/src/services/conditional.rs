//! Helpers for implementing conditional interactions using SearchEngine.

use crate::Result;
use crate::{
    db::search::engine::SearchEngine, db::PostgresTransactionContext, db::TransactionContext,
    services::CrudService,
};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Parsed If-None-Match condition for conditional updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfNoneMatchCondition {
    /// Wildcard (*) - operation succeeds only if no version exists.
    Any,
    /// Specific version - operation succeeds only if this specific version doesn't exist.
    Version(i32),
}

/// Parse If-None-Match header for conditional update operations.
pub fn parse_if_none_match_for_conditional_update(
    raw: Option<&str>,
) -> crate::Result<Option<IfNoneMatchCondition>> {
    let Some(raw) = raw.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    if raw.contains(',') {
        return Err(crate::Error::Validation(
            "Multiple If-None-Match values are not supported for conditional update".to_string(),
        ));
    }

    if raw == "*" {
        return Ok(Some(IfNoneMatchCondition::Any));
    }

    let v = raw
        .trim()
        .strip_prefix("W/")
        .unwrap_or(raw.trim())
        .trim_matches('"')
        .parse::<i32>()
        .map_err(|_| {
            crate::Error::Validation(format!(
                "Invalid If-None-Match value for conditional update: {}",
                raw
            ))
        })?;
    Ok(Some(IfNoneMatchCondition::Version(v)))
}

pub fn parse_form_urlencoded(s: &str) -> Result<Vec<(String, String)>> {
    Ok(url::form_urlencoded::parse(s.as_bytes())
        .into_owned()
        .collect())
}

pub fn query_from_url(url: &str) -> Option<&str> {
    url.split_once('?').map(|(_, q)| q)
}

pub fn build_conditional_search_params_from_items(
    items: &[(String, String)],
) -> Result<crate::db::search::params::SearchParameters> {
    // Remove `_format` which is not a search parameter (it only affects response formatting).
    let search_items: Vec<(String, String)> = items
        .iter()
        .filter(|(k, _)| k != "_format")
        .cloned()
        .collect();

    let mut search_params = crate::db::search::params::SearchParameters::from_items(&search_items)?;
    // Ensure conditional resolution is not affected by pagination/result params.
    search_params.count = Some(2);
    search_params.offset = None;
    search_params.cursor = None;
    search_params.max_results = None;
    search_params.sort.clear();
    search_params.include.clear();
    search_params.revinclude.clear();
    search_params.summary = None;
    search_params.elements.clear();
    search_params.total = crate::db::search::params::TotalMode::None;

    Ok(search_params)
}

pub fn extract_match_id(matched: &serde_json::Value) -> Result<String> {
    matched
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            crate::Error::Internal("Search match did not include resource.id".to_string())
        })
}

pub fn extract_match_resource_type(matched: &serde_json::Value) -> Result<String> {
    matched
        .get("resourceType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            crate::Error::Internal("Search match did not include resource.resourceType".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_if_none_match_for_conditional_update() {
        assert_eq!(
            parse_if_none_match_for_conditional_update(None).unwrap(),
            None
        );
        assert_eq!(
            parse_if_none_match_for_conditional_update(Some("")).unwrap(),
            None
        );
        assert_eq!(
            parse_if_none_match_for_conditional_update(Some("   ")).unwrap(),
            None
        );

        assert_eq!(
            parse_if_none_match_for_conditional_update(Some("*")).unwrap(),
            Some(IfNoneMatchCondition::Any)
        );
        assert_eq!(
            parse_if_none_match_for_conditional_update(Some("W/\"5\"")).unwrap(),
            Some(IfNoneMatchCondition::Version(5))
        );
        assert_eq!(
            parse_if_none_match_for_conditional_update(Some("\"5\"")).unwrap(),
            Some(IfNoneMatchCondition::Version(5))
        );
        assert_eq!(
            parse_if_none_match_for_conditional_update(Some("5")).unwrap(),
            Some(IfNoneMatchCondition::Version(5))
        );

        assert!(parse_if_none_match_for_conditional_update(Some("W/\"1\", W/\"2\"")).is_err());
        assert!(parse_if_none_match_for_conditional_update(Some("W/\"abc\"")).is_err());
        assert!(parse_if_none_match_for_conditional_update(Some("invalid")).is_err());
    }
}

#[async_trait]
pub trait ConditionalStore: Send + Sync {
    async fn logical_id_exists(&mut self, resource_type: &str, id: &str) -> Result<bool>;
    async fn version_exists(
        &mut self,
        resource_type: &str,
        id: &str,
        version_id: i32,
    ) -> Result<bool>;
}

#[async_trait]
impl ConditionalStore for CrudService {
    async fn logical_id_exists(&mut self, resource_type: &str, id: &str) -> Result<bool> {
        match self.read_resource(resource_type, id).await {
            Ok(_) => Ok(true),
            Err(crate::Error::ResourceDeleted { .. }) => Ok(true),
            Err(crate::Error::ResourceNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn version_exists(
        &mut self,
        resource_type: &str,
        id: &str,
        version_id: i32,
    ) -> Result<bool> {
        match self.vread_resource(resource_type, id, version_id).await {
            Ok(_) => Ok(true),
            Err(crate::Error::ResourceDeleted { .. }) => Ok(true),
            Err(crate::Error::VersionNotFound { .. }) => Ok(false),
            Err(crate::Error::ResourceNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl ConditionalStore for Arc<CrudService> {
    async fn logical_id_exists(&mut self, resource_type: &str, id: &str) -> Result<bool> {
        match self.as_ref().read_resource(resource_type, id).await {
            Ok(_) => Ok(true),
            Err(crate::Error::ResourceDeleted { .. }) => Ok(true),
            Err(crate::Error::ResourceNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    async fn version_exists(
        &mut self,
        resource_type: &str,
        id: &str,
        version_id: i32,
    ) -> Result<bool> {
        match self
            .as_ref()
            .vread_resource(resource_type, id, version_id)
            .await
        {
            Ok(_) => Ok(true),
            Err(crate::Error::ResourceDeleted { .. }) => Ok(true),
            Err(crate::Error::VersionNotFound { .. }) => Ok(false),
            Err(crate::Error::ResourceNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl ConditionalStore for PostgresTransactionContext {
    async fn logical_id_exists(&mut self, resource_type: &str, id: &str) -> Result<bool> {
        Ok(self.read(resource_type, id).await?.is_some())
    }

    async fn version_exists(
        &mut self,
        resource_type: &str,
        id: &str,
        version_id: i32,
    ) -> Result<bool> {
        PostgresTransactionContext::version_exists(self, resource_type, id, version_id).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalCreateResult {
    NoMatch,
    MatchFound { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalTargetResolution {
    pub target_id: Option<String>,
    pub target_existed: bool,
}

#[derive(Clone)]
pub struct ConditionalService {
    search_engine: Arc<SearchEngine>,
}

impl ConditionalService {
    pub fn new(search_engine: Arc<SearchEngine>) -> Self {
        Self { search_engine }
    }

    pub async fn conditional_create(
        &self,
        resource_type: &str,
        search_items: &[(String, String)],
        base_url: Option<&str>,
        strict_handling: bool,
    ) -> Result<ConditionalCreateResult> {
        let search_items: Vec<(String, String)> = search_items
            .iter()
            .filter(|(k, _)| k != "_format")
            .cloned()
            .collect();

        if search_items.is_empty() {
            return Err(crate::Error::Validation(
                "If-None-Exist header must contain search parameters".to_string(),
            ));
        }

        let search_params = build_conditional_search_params_from_items(&search_items)?;
        let search_result = self
            .search_engine
            .search(Some(resource_type), &search_params, base_url)
            .await?;

        if strict_handling && !search_result.unknown_params.is_empty() {
            return Err(crate::Error::Validation(format!(
                "Unknown or unsupported search parameters for {}: {}",
                resource_type,
                search_result.unknown_params.join(", ")
            )));
        }

        self.conditional_create_from_matches(&search_result.resources)
    }

    pub fn conditional_create_from_matches(
        &self,
        matched_resources: &[JsonValue],
    ) -> Result<ConditionalCreateResult> {
        match matched_resources.len() {
            0 => Ok(ConditionalCreateResult::NoMatch),
            1 => Ok(ConditionalCreateResult::MatchFound {
                id: extract_match_id(&matched_resources[0])?,
            }),
            _ => Err(crate::Error::PreconditionFailed(
                "Multiple resources match If-None-Exist criteria".to_string(),
            )),
        }
    }

    pub async fn resolve_conditional_target<S: ConditionalStore>(
        &self,
        store: &mut S,
        resource_type: &str,
        query_items: &[(String, String)],
        base_url: Option<&str>,
        strict_handling: bool,
        id_in_body: Option<&str>,
    ) -> Result<ConditionalTargetResolution> {
        let query_items: Vec<(String, String)> = query_items
            .iter()
            .filter(|(k, _)| k != "_format")
            .cloned()
            .collect();

        if query_items.is_empty() {
            return Err(crate::Error::Validation(
                "Conditional operation requires search parameters in the query string".to_string(),
            ));
        }

        let search_params = build_conditional_search_params_from_items(&query_items)?;
        let search_result = self
            .search_engine
            .search(Some(resource_type), &search_params, base_url)
            .await?;

        if strict_handling && !search_result.unknown_params.is_empty() {
            return Err(crate::Error::Validation(format!(
                "Unknown or unsupported search parameters for {}: {}",
                resource_type,
                search_result.unknown_params.join(", ")
            )));
        }

        self.resolve_conditional_target_from_matches(
            store,
            resource_type,
            id_in_body,
            &search_result.resources,
        )
        .await
    }

    pub async fn resolve_conditional_target_from_matches<S: ConditionalStore>(
        &self,
        store: &mut S,
        resource_type: &str,
        id_in_body: Option<&str>,
        matched_resources: &[JsonValue],
    ) -> Result<ConditionalTargetResolution> {
        let (target_id, target_existed) = match matched_resources.len() {
            0 => {
                if let Some(id) = id_in_body {
                    let exists = store.logical_id_exists(resource_type, id).await?;
                    if exists {
                        return Err(crate::Error::BusinessRule(format!(
                            "Conditional update: no matches for criteria, but resource id '{}' already exists",
                            id
                        )));
                    }
                    (Some(id.to_string()), false)
                } else {
                    (None, false)
                }
            }
            1 => {
                let matched_id = extract_match_id(&matched_resources[0])?;
                if let Some(id) = id_in_body {
                    if id != matched_id {
                        return Err(crate::Error::Validation(format!(
                            "Conditional update id '{}' does not match the resolved resource id '{}'",
                            id, matched_id
                        )));
                    }
                }
                (Some(matched_id), true)
            }
            _ => {
                return Err(crate::Error::PreconditionFailed(
                    "Multiple resources match conditional criteria".to_string(),
                ));
            }
        };

        Ok(ConditionalTargetResolution {
            target_id,
            target_existed,
        })
    }

    pub async fn check_if_none_match<S: ConditionalStore>(
        &self,
        store: &mut S,
        resource_type: &str,
        target_id: Option<&str>,
        target_existed: bool,
        if_none_match: Option<IfNoneMatchCondition>,
    ) -> Result<()> {
        match (if_none_match, target_id, target_existed) {
            (Some(IfNoneMatchCondition::Any), Some(_), true) => Err(crate::Error::PreconditionFailed(
                "If-None-Match precondition failed: resource already exists".to_string(),
            )),
            (Some(IfNoneMatchCondition::Version(_)), None, _) => Err(crate::Error::Validation(
                "If-None-Match with a version requires a resolvable logical id (match or client-provided id)"
                    .to_string(),
            )),
            (Some(IfNoneMatchCondition::Version(v)), Some(id), _) => {
                let exists = store.version_exists(resource_type, id, v).await?;
                if exists {
                    return Err(crate::Error::PreconditionFailed(format!(
                        "If-None-Match precondition failed: version {} already exists",
                        v
                    )));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
