//! Metadata repository - database queries for CapabilityStatement generation

use crate::Result;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// Search parameter information for CapabilityStatement
#[derive(Debug, Clone)]
pub struct SearchParameterInfo {
    pub resource_type: String,
    pub code: String,
    pub param_type: String,
    pub description: Option<String>,
    pub targets: Option<Vec<String>>,
}

/// Repository for metadata database operations
#[derive(Clone)]
pub struct MetadataRepository {
    pool: PgPool,
}

impl MetadataRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all active search parameters
    pub async fn get_search_parameters(&self) -> Result<Vec<SearchParameterInfo>> {
        let query = r#"
            SELECT
                resource_type,
                code,
                type,
                description,
                targets
            FROM search_parameters
            WHERE active = true
            ORDER BY resource_type, code
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

        let mut params = Vec::new();
        for row in rows {
            params.push(SearchParameterInfo {
                resource_type: row.get("resource_type"),
                code: row.get("code"),
                param_type: row.get("type"),
                description: row.get("description"),
                targets: row.get("targets"),
            });
        }

        Ok(params)
    }

    /// Get search parameters grouped by resource type
    pub async fn get_search_parameters_by_resource(
        &self,
    ) -> Result<HashMap<String, Vec<SearchParameterInfo>>> {
        let params = self.get_search_parameters().await?;

        let mut params_by_resource: HashMap<String, Vec<SearchParameterInfo>> = HashMap::new();

        for param in params {
            params_by_resource
                .entry(param.resource_type.clone())
                .or_default()
                .push(param);
        }

        Ok(params_by_resource)
    }

    /// Get list of resource types from StructureDefinitions in database
    pub async fn get_resource_types_from_structure_definitions(&self) -> Result<Vec<String>> {
        let query = r#"
            SELECT DISTINCT resource->>'type' as resource_type
            FROM resources
            WHERE resource_type = 'StructureDefinition'
              AND resource->>'kind' = 'resource'
              AND resource->>'derivation' = 'specialization'
              AND deleted = FALSE
            ORDER BY resource_type
        "#;

        let rows = sqlx::query(query)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

        let mut resource_types = Vec::new();
        for row in rows {
            if let Some(rt) = row.get::<Option<String>, _>("resource_type") {
                resource_types.push(rt);
            }
        }

        Ok(resource_types)
    }
}
