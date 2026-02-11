//! System-level FHIR operations that combine search + CRUD.

use crate::{
    db::search::engine::SearchEngine,
    services::{conditional::build_conditional_search_params_from_items, CrudService},
    Result,
};
use serde_json::Value as JsonValue;
use std::sync::Arc;

pub struct SystemService {
    search_engine: Arc<SearchEngine>,
    crud: Arc<CrudService>,
    supported_resource_types: Vec<String>,
}

impl SystemService {
    pub fn new(
        search_engine: Arc<SearchEngine>,
        crud: Arc<CrudService>,
        supported_resource_types: Vec<String>,
    ) -> Self {
        Self {
            search_engine,
            crud,
            supported_resource_types,
        }
    }

    pub async fn system_delete(
        &self,
        query_items: &[(String, String)],
        base_url: &str,
        strict_handling: bool,
        expected_version: Option<i32>,
    ) -> Result<Option<i32>> {
        if query_items.is_empty() {
            return Err(crate::Error::Validation(
                "System delete requires search parameters in the query string".to_string(),
            ));
        }

        let search_params = build_conditional_search_params_from_items(query_items)?;
        let search_result = self
            .search_engine
            .search(None, &search_params, Some(base_url))
            .await?;

        if strict_handling && !search_result.unknown_params.is_empty() {
            return Err(crate::Error::Validation(format!(
                "Unknown or unsupported search parameters for system delete: {}",
                search_result.unknown_params.join(", ")
            )));
        }

        let matched = match search_result.resources.len() {
            0 => {
                return Err(crate::Error::NotFound(
                    "No resources match system delete criteria".to_string(),
                ));
            }
            1 => &search_result.resources[0],
            _ => {
                return Err(crate::Error::PreconditionFailed(
                    "Multiple resources match system delete criteria".to_string(),
                ));
            }
        };

        let (resource_type, id) = extract_match_target(matched)?;
        self.ensure_resource_type_supported(&resource_type)?;

        if let Some(expected) = expected_version {
            let current = self.crud.read_resource(&resource_type, &id).await?;
            if current.version_id != expected {
                return Err(crate::Error::VersionConflict {
                    expected,
                    actual: current.version_id,
                });
            }
        }

        self.crud.delete_resource(&resource_type, &id).await
    }

    fn ensure_resource_type_supported(&self, resource_type: &str) -> Result<()> {
        if self.supported_resource_types.is_empty() {
            return Ok(());
        }
        if self
            .supported_resource_types
            .iter()
            .any(|rt| rt == resource_type)
        {
            Ok(())
        } else {
            Err(crate::Error::MethodNotAllowed(format!(
                "Resource type '{}' is not supported by this server",
                resource_type
            )))
        }
    }
}

fn extract_match_target(matched: &JsonValue) -> Result<(String, String)> {
    let resource_type = matched
        .get("resourceType")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::Error::Internal("Search match did not include resourceType".to_string())
        })?
        .to_string();
    let id = matched
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::Error::Internal("Search match did not include resource.id".to_string())
        })?
        .to_string();
    Ok((resource_type, id))
}
