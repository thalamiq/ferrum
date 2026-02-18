//! Terminology repository - database access for terminology operations
//!
//! This repository handles all database queries related to FHIR terminology:
//! - CodeSystem and ValueSet resource lookups
//! - Concept lookups in codesystem_concepts table
//! - ValueSet expansion caching
//! - Terminology closure tables

use crate::{Error, Result};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

/// Row from codesystem_concepts table
#[derive(Debug, Clone)]
pub struct ConceptRow {
    pub system: String,
    pub code: String,
    pub display: String,
    pub version: Option<String>,
}

/// Concept details with properties and designations
#[derive(Debug, Clone)]
pub struct ConceptDetails {
    pub display: Option<String>,
    pub properties: Option<JsonValue>,
    pub designations: Option<JsonValue>,
}

impl ConceptDetails {
    /// Check if this concept is abstract based on its properties
    pub fn is_abstract(&self) -> bool {
        if let Some(ref props) = self.properties {
            if let Some(arr) = props.as_array() {
                for prop in arr {
                    let code = prop.get("code").and_then(|v| v.as_str());
                    if code == Some("notSelectable") || code == Some("abstract") {
                        if let Some(true) = prop
                            .get("valueBoolean")
                            .and_then(|v| v.as_bool())
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// Repository for terminology database operations
#[derive(Clone)]
pub struct TerminologyRepository {
    pool: PgPool,
}

impl TerminologyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a FHIR resource by its ID
    pub async fn find_resource_by_id(
        &self,
        resource_type: &str,
        id: &str,
    ) -> Result<Option<JsonValue>> {
        let row = sqlx::query_scalar::<_, JsonValue>(
            "SELECT resource FROM resources
             WHERE resource_type = $1 AND id = $2 AND is_current = true AND deleted = false
             LIMIT 1",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row)
    }

    /// Find a FHIR resource by its canonical URL and optional version
    pub async fn find_resource_by_canonical_url(
        &self,
        resource_type: &str,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<JsonValue>> {
        let row = if let Some(v) = version {
            sqlx::query_scalar::<_, JsonValue>(
                "SELECT resource
                 FROM (
                     SELECT resource, is_current, last_updated, version_id
                     FROM resources
                     WHERE resource_type = $1
                       AND deleted = FALSE
                       AND (url = $2 OR (url IS NULL AND resource->>'url' = $2))
                       AND resource->>'version' = $3
                 ) candidates
                 ORDER BY is_current DESC, last_updated DESC, version_id DESC
                 LIMIT 1",
            )
            .bind(resource_type)
            .bind(url)
            .bind(v)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?
        } else {
            sqlx::query_scalar::<_, JsonValue>(
                "SELECT resource
                 FROM resources
                 WHERE resource_type = $1
                   AND is_current = TRUE
                   AND deleted = FALSE
                   AND (url = $2 OR (url IS NULL AND resource->>'url' = $2))
                 ORDER BY last_updated DESC, version_id DESC
                 LIMIT 1",
            )
            .bind(resource_type)
            .bind(url)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?
        };

        Ok(row)
    }

    /// Find a concept in the codesystem_concepts table
    pub async fn find_concept_in_table(
        &self,
        system: &str,
        code: &str,
        version: Option<&str>,
    ) -> Result<Option<ConceptDetails>> {
        let row = sqlx::query(
            "SELECT display, properties, designations
             FROM codesystem_concepts
             WHERE system = $1
               AND code = $2
               AND version IS NOT DISTINCT FROM $3
             LIMIT 1",
        )
        .bind(system)
        .bind(code)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| ConceptDetails {
            display: Some(r.get("display")),
            properties: r.get("properties"),
            designations: r.get("designations"),
        }))
    }

    /// Find the content mode of a CodeSystem by its canonical URL
    pub async fn find_codesystem_content_mode(&self, url: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT resource->>'content' as content
             FROM resources
             WHERE resource_type = 'CodeSystem'
               AND is_current = TRUE
               AND deleted = FALSE
               AND (url = $1 OR (url IS NULL AND resource->>'url' = $1))
             ORDER BY last_updated DESC
             LIMIT 1",
        )
        .bind(url)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|(content,)| content))
    }

    /// Fetch concepts by filter from codesystem_concepts table
    pub async fn fetch_concepts_by_filter(
        &self,
        system: &str,
        property: &str,
        op: &str,
        value: &str,
    ) -> Result<Vec<ConceptRow>> {
        let rows = match op {
            "=" => {
                sqlx::query(
                    "SELECT system, code, display, version
                     FROM codesystem_concepts
                     WHERE system = $1
                       AND properties @> $2::jsonb",
                )
                .bind(system)
                .bind(serde_json::to_string(&serde_json::json!([{"code": property, "valueString": value}])).unwrap())
                .fetch_all(&self.pool)
                .await
                .map_err(Error::Database)?
            }
            "in" => {
                let values: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
                // Use ANY to match property values against a list
                let conditions: Vec<String> = values
                    .iter()
                    .map(|v| {
                        serde_json::to_string(&serde_json::json!([{"code": property, "valueString": v}])).unwrap()
                    })
                    .collect();
                // Build a query that ORs multiple jsonb containment checks
                let mut query = String::from(
                    "SELECT system, code, display, version FROM codesystem_concepts WHERE system = $1 AND (",
                );
                for (i, _) in conditions.iter().enumerate() {
                    if i > 0 {
                        query.push_str(" OR ");
                    }
                    query.push_str(&format!("properties @> ${}::jsonb", i + 2));
                }
                query.push(')');

                let mut q = sqlx::query(&query).bind(system);
                for cond in &conditions {
                    q = q.bind(cond);
                }
                q.fetch_all(&self.pool).await.map_err(Error::Database)?
            }
            _ => {
                // For is-a and descendent-of, we don't filter by property in SQL
                // The caller handles hierarchy traversal
                return Ok(Vec::new());
            }
        };

        Ok(rows
            .into_iter()
            .map(|r| ConceptRow {
                system: r.get("system"),
                code: r.get("code"),
                display: r.get("display"),
                version: r.get("version"),
            })
            .collect())
    }

    /// Fetch all concepts for a system from codesystem_concepts table
    pub async fn fetch_system_concepts(&self, system: &str) -> Result<Vec<ConceptRow>> {
        let rows = sqlx::query(
            "SELECT system, code, display, version
             FROM codesystem_concepts
             WHERE system = $1",
        )
        .bind(system)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|r| ConceptRow {
                system: r.get("system"),
                code: r.get("code"),
                display: r.get("display"),
                version: r.get("version"),
            })
            .collect())
    }

    /// Fetch cached ValueSet expansion
    pub async fn fetch_cached_expansion(
        &self,
        valueset_url: &str,
        valueset_version: &str,
        params_hash: &str,
    ) -> Result<Option<JsonValue>> {
        let row: Option<(JsonValue,)> = sqlx::query_as(
            r#"
            SELECT contains
            FROM valueset_expansions
            WHERE valueset_url = $1
              AND valueset_version IS NOT DISTINCT FROM $2
              AND parameters_hash = $3
              AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(valueset_url)
        .bind(if valueset_version.is_empty() {
            None
        } else {
            Some(valueset_version)
        })
        .bind(params_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|(contains,)| contains))
    }

    /// Store ValueSet expansion in cache
    pub async fn store_expansion_cache(
        &self,
        valueset_url: &str,
        valueset_version: &str,
        params_hash: &str,
        expansion_id: Uuid,
        total: usize,
        offset: usize,
        count: usize,
        contains: &JsonValue,
        concepts: &[(
            String,
            String,
            Option<String>,
            Option<bool>,
            Option<JsonValue>,
        )], // (system, code, display, inactive, designations)
    ) -> Result<()> {
        use serde_json::json;

        let parameters = json!({
            "offset": offset,
            "count": count,
        });

        // Store expansion header
        sqlx::query(
            r#"
            INSERT INTO valueset_expansions (
                id, valueset_url, valueset_version, parameters, parameters_hash,
                total, "offset", count, contains, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW() + INTERVAL '24 hours')
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(expansion_id)
        .bind(valueset_url)
        .bind(if valueset_version.is_empty() {
            None
        } else {
            Some(valueset_version)
        })
        .bind(parameters)
        .bind(params_hash)
        .bind(total as i32)
        .bind(offset as i32)
        .bind(count as i32)
        .bind(contains)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Store individual concepts for querying
        for (ordinal, (system, code, display, inactive, designations)) in
            concepts.iter().enumerate()
        {
            sqlx::query(
                r#"
                INSERT INTO valueset_expansion_concepts (
                    expansion_id, system, code, display, inactive, designations, ordinal
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(expansion_id)
            .bind(system)
            .bind(code)
            .bind(display)
            .bind(inactive.unwrap_or(false))
            .bind(designations)
            .bind(ordinal as i32)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        Ok(())
    }

    // ========== Closure Table Operations ==========

    /// Get current version of a closure table
    pub async fn get_closure_version(&self, name: &str) -> Result<Option<i32>> {
        let version: Option<i32> = sqlx::query_scalar(
            "SELECT current_version FROM terminology_closure_tables WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(version)
    }

    /// Check if closure table requires reinitialization
    pub async fn closure_requires_reinit(&self, name: &str) -> Result<bool> {
        let requires: bool = sqlx::query_scalar(
            "SELECT requires_reinit FROM terminology_closure_tables WHERE name = $1",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(requires)
    }

    /// Create a new closure table
    pub async fn create_closure_table(&self, name: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO terminology_closure_tables (name, current_version) VALUES ($1, 1)",
        )
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Begin a transaction for closure operations
    pub async fn begin_transaction(&self) -> Result<Transaction<'_, Postgres>> {
        self.pool.begin().await.map_err(Error::Database)
    }

    /// Insert a concept into closure table
    pub async fn insert_closure_concept(
        tx: &mut Transaction<'_, Postgres>,
        closure_name: &str,
        system: &str,
        code: &str,
        display: Option<&str>,
    ) -> Result<bool> {
        let rows = sqlx::query(
            "INSERT INTO terminology_closure_concepts (closure_name, system, code, display)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(closure_name)
        .bind(system)
        .bind(code)
        .bind(display)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?
        .rows_affected();

        Ok(rows > 0)
    }

    /// Update closure table version
    pub async fn update_closure_version(
        tx: &mut Transaction<'_, Postgres>,
        name: &str,
        version: i32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE terminology_closure_tables SET current_version = $2, updated_at = NOW() WHERE name = $1",
        )
        .bind(name)
        .bind(version)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Fetch all concepts from a closure table
    pub async fn fetch_closure_concepts(
        tx: &mut Transaction<'_, Postgres>,
        closure_name: &str,
    ) -> Result<Vec<(String, String)>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT system, code FROM terminology_closure_concepts WHERE closure_name = $1",
        )
        .bind(closure_name)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows)
    }

    /// Insert a closure relation
    pub async fn insert_closure_relation(
        tx: &mut Transaction<'_, Postgres>,
        closure_name: &str,
        source_system: &str,
        source_code: &str,
        target_system: &str,
        target_code: &str,
        equivalence: &str,
        version: i32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO terminology_closure_relations (
                closure_name, source_system, source_code, target_system, target_code, equivalence, introduced_in_version
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT DO NOTHING",
        )
        .bind(closure_name)
        .bind(source_system)
        .bind(source_code)
        .bind(target_system)
        .bind(target_code)
        .bind(equivalence)
        .bind(version)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Fetch closure relations, optionally filtering by version
    pub async fn fetch_closure_relations(
        &self,
        closure_name: &str,
        since_version: Option<i32>,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let relations = if let Some(since) = since_version {
            sqlx::query_as(
                "SELECT source_system, source_code, target_system, target_code, equivalence
                 FROM terminology_closure_relations
                 WHERE closure_name = $1
                   AND introduced_in_version > $2
                 ORDER BY source_system, source_code, target_system, target_code",
            )
            .bind(closure_name)
            .bind(since)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
        } else {
            sqlx::query_as(
                "SELECT source_system, source_code, target_system, target_code, equivalence
                 FROM terminology_closure_relations
                 WHERE closure_name = $1
                 ORDER BY source_system, source_code, target_system, target_code",
            )
            .bind(closure_name)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?
        };

        Ok(relations)
    }
}
