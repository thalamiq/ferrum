//! Terminology resource hook
//!
//! Maintains extracted terminology tables used by terminology operations:
//! - `codesystem_concepts` (from CodeSystem.concept)
//! - `conceptmap_*` (from ConceptMap.group.*)
//! - `implicit_valuesets` (from CodeSystem.valueSet)

use crate::{hooks::ResourceHook, models::Resource, Result};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};

pub struct TerminologyHook {
    pool: PgPool,
}

impl TerminologyHook {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn index_codesystem(&self, resource: &Resource) -> Result<()> {
        let Some(url) = resource.resource.get("url").and_then(|v| v.as_str()) else {
            return Ok(());
        };
        let version = resource.resource.get("version").and_then(|v| v.as_str());

        let concepts = resource.resource.get("concept").and_then(|v| v.as_array());
        if concepts.is_none() {
            // No concepts to index (e.g. content = not-present)
            return Ok(());
        }

        let mut tx = self.pool.begin().await.map_err(crate::Error::Database)?;

        sqlx::query(
            "DELETE FROM codesystem_concepts
             WHERE system = $1 AND version IS NOT DISTINCT FROM $2",
        )
        .bind(url)
        .bind(version)
        .execute(&mut *tx)
        .await
        .map_err(crate::Error::Database)?;

        if let Some(concepts) = concepts {
            for concept in flatten_codesystem_concepts(concepts) {
                let Some(code) = concept.get("code").and_then(|v| v.as_str()) else {
                    continue;
                };
                let display = concept
                    .get("display")
                    .and_then(|v| v.as_str())
                    .unwrap_or(code);
                let properties = concept.get("property").cloned();
                let designations = concept.get("designation").cloned();

                sqlx::query(
                    "INSERT INTO codesystem_concepts (system, version, code, display, properties, designations)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (system, version, code) DO UPDATE
                       SET display = EXCLUDED.display,
                           properties = EXCLUDED.properties,
                           designations = EXCLUDED.designations,
                           updated_at = NOW()",
                )
                .bind(url)
                .bind(version)
                .bind(code)
                .bind(display)
                .bind(properties)
                .bind(designations)
                .execute(&mut *tx)
                .await
                .map_err(crate::Error::Database)?;
            }
        }

        if let Some(vs_url) = resource.resource.get("valueSet").and_then(|v| v.as_str()) {
            sqlx::query(
                "INSERT INTO implicit_valuesets (codesystem_url, codesystem_version, valueset_url)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (codesystem_url) DO UPDATE
                   SET codesystem_version = EXCLUDED.codesystem_version,
                       valueset_url = EXCLUDED.valueset_url,
                       updated_at = NOW()",
            )
            .bind(url)
            .bind(version)
            .bind(vs_url)
            .execute(&mut *tx)
            .await
            .map_err(crate::Error::Database)?;
        }

        tx.commit().await.map_err(crate::Error::Database)?;
        Ok(())
    }

    async fn index_conceptmap(&self, resource: &Resource) -> Result<()> {
        let Some(url) = resource.resource.get("url").and_then(|v| v.as_str()) else {
            return Ok(());
        };
        let version = resource.resource.get("version").and_then(|v| v.as_str());
        let Some(groups) = resource.resource.get("group").and_then(|v| v.as_array()) else {
            return Ok(());
        };

        let mut tx = self.pool.begin().await.map_err(crate::Error::Database)?;

        // Cascade deletes elements/targets.
        sqlx::query(
            "DELETE FROM conceptmap_groups
             WHERE conceptmap_url = $1 AND conceptmap_version IS NOT DISTINCT FROM $2",
        )
        .bind(url)
        .bind(version)
        .execute(&mut *tx)
        .await
        .map_err(crate::Error::Database)?;

        for group in groups {
            let source_system = group.get("source").and_then(|v| v.as_str());
            let source_version = group.get("sourceVersion").and_then(|v| v.as_str());
            let target_system = group.get("target").and_then(|v| v.as_str());
            let target_version = group.get("targetVersion").and_then(|v| v.as_str());

            let group_row = sqlx::query(
                "INSERT INTO conceptmap_groups (
                    conceptmap_url, conceptmap_version,
                    source_system, source_version,
                    target_system, target_version
                 )
                 VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id",
            )
            .bind(url)
            .bind(version)
            .bind(source_system)
            .bind(source_version)
            .bind(target_system)
            .bind(target_version)
            .fetch_one(&mut *tx)
            .await
            .map_err(crate::Error::Database)?;

            let group_id: i32 = group_row.get("id");

            let Some(elements) = group.get("element").and_then(|v| v.as_array()) else {
                continue;
            };

            for element in elements {
                let Some(source_code) = element.get("code").and_then(|v| v.as_str()) else {
                    continue;
                };
                let source_display = element.get("display").and_then(|v| v.as_str());
                let no_map = element
                    .get("noMap")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let element_row = sqlx::query(
                    "INSERT INTO conceptmap_elements (group_id, source_code, source_display, no_map)
                     VALUES ($1, $2, $3, $4)
                     RETURNING id",
                )
                .bind(group_id)
                .bind(source_code)
                .bind(source_display)
                .bind(no_map)
                .fetch_one(&mut *tx)
                .await
                .map_err(crate::Error::Database)?;

                let element_id: i32 = element_row.get("id");

                let Some(targets) = element.get("target").and_then(|v| v.as_array()) else {
                    continue;
                };

                for target in targets {
                    let target_code = target.get("code").and_then(|v| v.as_str());
                    let target_display = target.get("display").and_then(|v| v.as_str());
                    let equivalence = target
                        .get("equivalence")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unmatched");
                    let comment = target.get("comment").and_then(|v| v.as_str());
                    let dependencies = target.get("dependsOn").cloned();
                    let products = target.get("product").cloned();

                    sqlx::query(
                        "INSERT INTO conceptmap_targets (
                            element_id, target_code, target_display, equivalence, comment, dependencies, products
                         )
                         VALUES ($1, $2, $3, $4, $5, $6, $7)",
                    )
                    .bind(element_id)
                    .bind(target_code)
                    .bind(target_display)
                    .bind(equivalence)
                    .bind(comment)
                    .bind(dependencies)
                    .bind(products)
                    .execute(&mut *tx)
                    .await
                    .map_err(crate::Error::Database)?;
                }
            }
        }

        tx.commit().await.map_err(crate::Error::Database)?;
        Ok(())
    }
}

#[async_trait]
impl ResourceHook for TerminologyHook {
    async fn on_created(&self, resource: &Resource) -> Result<()> {
        self.on_updated(resource).await
    }

    async fn on_updated(&self, resource: &Resource) -> Result<()> {
        match resource.resource_type.as_str() {
            "CodeSystem" => self.index_codesystem(resource).await?,
            "ConceptMap" => self.index_conceptmap(resource).await?,
            _ => {}
        }
        Ok(())
    }

    async fn on_deleted(&self, _resource_type: &str, _id: &str, _version: i32) -> Result<()> {
        // Best-effort cleanup is deferred; without canonical URL we cannot reliably
        // remove extracted rows without reloading the resource.
        Ok(())
    }
}

fn flatten_codesystem_concepts<'a>(root: &'a [JsonValue]) -> Vec<&'a JsonValue> {
    let mut out = Vec::new();
    let mut stack: Vec<&'a [JsonValue]> = vec![root];
    while let Some(slice) = stack.pop() {
        for concept in slice {
            out.push(concept);
            if let Some(children) = concept.get("concept").and_then(|v| v.as_array()) {
                stack.push(children);
            }
        }
    }
    out
}
