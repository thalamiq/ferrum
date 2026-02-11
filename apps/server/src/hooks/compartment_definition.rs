//! CompartmentDefinition hook
//!
//! Populates the compartment_memberships table when CompartmentDefinition resources are created or updated.
//!
//! Per FHIR spec, CompartmentDefinitions define which resource types belong to a compartment
//! and which search parameters establish membership.

use crate::{hooks::ResourceHook, models::Resource, Result};
use async_trait::async_trait;
use sqlx::PgPool;

/// Hook that processes CompartmentDefinition resources
///
/// When a CompartmentDefinition is created/updated, this hook:
/// 1. Extracts the compartment type (e.g., "Patient", "Encounter")
/// 2. Parses the resource[] array to get membership rules
/// 3. Rebuilds compartment_memberships table for that compartment type
pub struct CompartmentDefinitionHook {
    db_pool: PgPool,
}

impl CompartmentDefinitionHook {
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Rebuild compartment memberships for a CompartmentDefinition
    async fn rebuild_compartment_memberships(&self, resource: &Resource) -> Result<()> {
        let Some(obj) = resource.resource.as_object() else {
            return Ok(());
        };

        // Extract compartment type (e.g., "Patient")
        let Some(compartment_type) = obj.get("code").and_then(|v| v.as_str()) else {
            tracing::warn!(
                "CompartmentDefinition {} missing 'code' field, skipping",
                resource.id
            );
            return Ok(());
        };

        tracing::info!(
            "Rebuilding compartment memberships for {} compartment from CompartmentDefinition/{}",
            compartment_type,
            resource.id
        );

        // Extract resource definitions
        let Some(resources) = obj.get("resource").and_then(|v| v.as_array()) else {
            tracing::warn!(
                "CompartmentDefinition {} missing 'resource' array, clearing compartment",
                resource.id
            );
            // Clear existing memberships for this compartment
            self.clear_compartment_memberships(compartment_type).await?;
            return Ok(());
        };

        // Parse resource definitions
        let mut memberships: Vec<CompartmentMembership> = Vec::new();
        for res_def in resources {
            let Some(res_obj) = res_def.as_object() else {
                continue;
            };

            let Some(resource_type) = res_obj.get("code").and_then(|v| v.as_str()) else {
                continue;
            };

            // Extract parameter names (if present)
            let parameter_names =
                if let Some(params) = res_obj.get("param").and_then(|v| v.as_array()) {
                    params
                        .iter()
                        .filter_map(|p| p.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    // No params = resource is NOT in this compartment
                    continue;
                };

            // Skip if no params (shouldn't happen after above check, but defensive)
            if parameter_names.is_empty() {
                continue;
            }

            // Extract optional temporal boundary params
            let start_param = res_obj
                .get("startParam")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let end_param = res_obj
                .get("endParam")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            memberships.push(CompartmentMembership {
                resource_type: resource_type.to_string(),
                parameter_names,
                start_param,
                end_param,
            });
        }

        // Rebuild database table
        self.store_compartment_memberships(compartment_type, &memberships)
            .await?;

        tracing::info!(
            "Rebuilt {} compartment with {} resource types",
            compartment_type,
            memberships.len()
        );

        Ok(())
    }

    /// Clear all memberships for a compartment type
    async fn clear_compartment_memberships(&self, compartment_type: &str) -> Result<()> {
        sqlx::query("DELETE FROM compartment_memberships WHERE compartment_type = $1")
            .bind(compartment_type)
            .execute(&self.db_pool)
            .await
            .map_err(crate::Error::Database)?;
        Ok(())
    }

    /// Store compartment memberships in database
    async fn store_compartment_memberships(
        &self,
        compartment_type: &str,
        memberships: &[CompartmentMembership],
    ) -> Result<()> {
        let mut tx = self.db_pool.begin().await.map_err(crate::Error::Database)?;

        // Clear existing memberships for this compartment
        sqlx::query("DELETE FROM compartment_memberships WHERE compartment_type = $1")
            .bind(compartment_type)
            .execute(&mut *tx)
            .await
            .map_err(crate::Error::Database)?;

        // Insert new memberships one by one
        for membership in memberships {
            sqlx::query(
                r#"
                INSERT INTO compartment_memberships (
                    compartment_type,
                    resource_type,
                    parameter_names,
                    start_param,
                    end_param
                )
                VALUES ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(compartment_type)
            .bind(&membership.resource_type)
            .bind(&membership.parameter_names)
            .bind(membership.start_param.as_deref())
            .bind(membership.end_param.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(crate::Error::Database)?;
        }

        tx.commit().await.map_err(crate::Error::Database)?;

        Ok(())
    }
}

#[async_trait]
impl ResourceHook for CompartmentDefinitionHook {
    async fn on_created(&self, resource: &Resource) -> Result<()> {
        if resource.resource_type == "CompartmentDefinition" {
            self.rebuild_compartment_memberships(resource).await?;
        }
        Ok(())
    }

    async fn on_updated(&self, resource: &Resource) -> Result<()> {
        if resource.resource_type == "CompartmentDefinition" {
            self.rebuild_compartment_memberships(resource).await?;
        }
        Ok(())
    }

    async fn on_deleted(&self, resource_type: &str, id: &str, _version: i32) -> Result<()> {
        if resource_type == "CompartmentDefinition" {
            tracing::info!(
                "CompartmentDefinition/{} deleted, but keeping compartment memberships (spec allows this)",
                id
            );
            // Per spec, servers may keep using compartment definitions even after deletion
            // We keep the memberships unless explicitly cleared by creating a new CompartmentDefinition
        }
        Ok(())
    }
}

/// Parsed compartment membership rule from CompartmentDefinition
struct CompartmentMembership {
    resource_type: String,
    parameter_names: Vec<String>,
    start_param: Option<String>,
    end_param: Option<String>,
}
