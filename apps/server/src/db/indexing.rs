//! Indexing repository - database operations for search index management
//!
//! This repository handles all SQL operations for the search index tables.
//! The IndexingService handles business logic (FHIRPath extraction, value normalization)
//! and calls these repository methods to persist the extracted values.

use crate::{models::Resource, Result};
use sqlx::{PgPool, Postgres, Transaction};
use std::hash::{Hash, Hasher};

/// Repository for search index database operations
#[derive(Clone)]
pub struct IndexingRepository {
    pool: PgPool,
}

impl IndexingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get the database pool (for transaction management)
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ==================== Advisory Locking ====================

    /// Compute advisory lock key from resource identifier
    pub fn compute_lock_key(resource_type: &str, resource_id: &str) -> i64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        resource_type.hash(&mut hasher);
        resource_id.hash(&mut hasher);
        hasher.finish() as i64
    }

    /// Acquire advisory lock for resource to prevent concurrent indexing
    pub async fn acquire_indexing_lock(
        tx: &mut Transaction<'_, Postgres>,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<()> {
        let lock_key = Self::compute_lock_key(resource_type, resource_id);

        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(lock_key)
            .execute(&mut **tx)
            .await
            .map_err(crate::Error::Database)?;

        Ok(())
    }

    // ==================== Index Status ====================

    /// Update resource_search_index_status to track indexing coverage
    pub async fn update_index_status(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource: &Resource,
        param_count: usize,
    ) -> Result<()> {
        // Get current hash for this resource type
        let current_hash: Option<String> = sqlx::query_scalar(
            "SELECT current_hash FROM search_parameter_versions WHERE resource_type = $1",
        )
        .bind(&resource.resource_type)
        .fetch_optional(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        let hash = current_hash.unwrap_or_else(|| "unknown".to_string());

        // Upsert status record
        sqlx::query(
            r#"
            INSERT INTO resource_search_index_status (
                resource_type, resource_id, version_id, search_params_hash,
                indexed_at, indexed_param_count, status
            )
            VALUES ($1, $2, $3, $4, NOW(), $5, 'completed')
            ON CONFLICT (resource_type, resource_id, version_id)
            DO UPDATE SET
                search_params_hash = EXCLUDED.search_params_hash,
                indexed_at = NOW(),
                indexed_param_count = EXCLUDED.indexed_param_count,
                status = 'completed',
                error_message = NULL
            "#,
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(&hash)
        .bind(param_count as i32)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(())
    }

    // ==================== Search Parameter Queries ====================

    /// Fetch search parameters for a resource type
    pub async fn fetch_search_parameters(
        &self,
        resource_type: &str,
    ) -> Result<Vec<sqlx::postgres::PgRow>> {
        let query = r#"
            SELECT
                resource_type,
                code,
                type,
                expression,
                url,
                modifiers,
                comparators,
                targets,
                active
            FROM search_parameters
            WHERE resource_type = $1 AND active = TRUE
            ORDER BY code
        "#;

        let rows = sqlx::query(query)
            .bind(resource_type)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

        Ok(rows)
    }

    // ==================== Deletion ====================

    /// Delete all search indices for a resource
    pub async fn delete_all_search_indices(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource_type: &str,
        resource_id: &str,
        version_id: Option<i32>,
    ) -> Result<()> {
        let tables = vec![
            "search_string",
            "search_token",
            "search_token_identifier",
            "search_reference",
            "search_date",
            "search_number",
            "search_quantity",
            "search_uri",
            "search_composite",
            "search_text",
            "search_content",
        ];

        for table in tables {
            let query = if let Some(_vid) = version_id {
                format!(
                    "DELETE FROM {} WHERE resource_type = $1 AND resource_id = $2 AND version_id = $3",
                    table
                )
            } else {
                format!(
                    "DELETE FROM {} WHERE resource_type = $1 AND resource_id = $2",
                    table
                )
            };

            let mut q = sqlx::query(&query).bind(resource_type).bind(resource_id);

            if let Some(vid) = version_id {
                q = q.bind(vid);
            }

            q.execute(&mut **tx).await.map_err(crate::Error::Database)?;
        }

        Ok(())
    }

    /// Delete search indices for specific parameters
    pub async fn delete_parameter_indices(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource_type: &str,
        resource_id: &str,
        version_id: i32,
        param_codes: &[String],
    ) -> Result<()> {
        let tables = vec![
            "search_string",
            "search_token",
            "search_token_identifier",
            "search_reference",
            "search_date",
            "search_number",
            "search_quantity",
            "search_uri",
            "search_composite",
            "search_text",
            "search_content",
        ];

        for table in tables {
            let query = format!(
                "DELETE FROM {} WHERE resource_type = $1 AND resource_id = $2 AND version_id = $3 AND parameter_name = ANY($4)",
                table
            );

            sqlx::query(&query)
                .bind(resource_type)
                .bind(resource_id)
                .bind(version_id)
                .bind(param_codes)
                .execute(&mut **tx)
                .await
                .map_err(crate::Error::Database)?;
        }

        Ok(())
    }

    // ==================== Parameter Version Management ====================

    /// Get old parameter codes for a resource type
    pub async fn get_old_parameter_codes(&self, resource_type: &str) -> Result<Vec<String>> {
        let query = r#"
            SELECT code
            FROM search_parameters
            WHERE resource_type = $1 AND active = FALSE
        "#;

        let codes = sqlx::query_scalar(query)
            .bind(resource_type)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

        Ok(codes)
    }
}
