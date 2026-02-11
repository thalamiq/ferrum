//! Runtime configuration repository
//!
//! Data access layer for runtime configuration stored in PostgreSQL.

use crate::runtime_config::{RuntimeConfigAuditEntry, RuntimeConfigEntry};
use crate::Result;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// Repository for runtime configuration database operations
#[derive(Debug, Clone)]
pub struct RuntimeConfigRepository {
    pool: PgPool,
}

impl RuntimeConfigRepository {
    /// Create a new repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all configuration entries from the database
    pub async fn get_all(&self) -> Result<Vec<RuntimeConfigEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT
                key,
                value,
                category,
                description,
                value_type,
                updated_at,
                updated_by,
                version
            FROM runtime_config
            ORDER BY category, key
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(crate::Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| RuntimeConfigEntry {
                key: row.get("key"),
                value: row.get("value"),
                category: row.get("category"),
                description: row.get("description"),
                value_type: row.get("value_type"),
                updated_at: row.get("updated_at"),
                updated_by: row.get("updated_by"),
                version: row.get("version"),
            })
            .collect())
    }

    /// Get all configuration entries as a key-value map
    pub async fn get_all_as_map(&self) -> Result<HashMap<String, JsonValue>> {
        let entries = self.get_all().await?;
        let map = entries.into_iter().map(|e| (e.key, e.value)).collect();
        Ok(map)
    }

    /// Get a single configuration entry by key
    pub async fn get(&self, key: &str) -> Result<Option<RuntimeConfigEntry>> {
        let row = sqlx::query(
            r#"
            SELECT
                key,
                value,
                category,
                description,
                value_type,
                updated_at,
                updated_by,
                version
            FROM runtime_config
            WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::Error::Database)?;

        Ok(row.map(|row| RuntimeConfigEntry {
            key: row.get("key"),
            value: row.get("value"),
            category: row.get("category"),
            description: row.get("description"),
            value_type: row.get("value_type"),
            updated_at: row.get("updated_at"),
            updated_by: row.get("updated_by"),
            version: row.get("version"),
        }))
    }

    /// Get configuration entries by category
    pub async fn get_by_category(&self, category: &str) -> Result<Vec<RuntimeConfigEntry>> {
        let rows = sqlx::query(
            r#"
            SELECT
                key,
                value,
                category,
                description,
                value_type,
                updated_at,
                updated_by,
                version
            FROM runtime_config
            WHERE category = $1
            ORDER BY key
            "#,
        )
        .bind(category)
        .fetch_all(&self.pool)
        .await
        .map_err(crate::Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| RuntimeConfigEntry {
                key: row.get("key"),
                value: row.get("value"),
                category: row.get("category"),
                description: row.get("description"),
                value_type: row.get("value_type"),
                updated_at: row.get("updated_at"),
                updated_by: row.get("updated_by"),
                version: row.get("version"),
            })
            .collect())
    }

    /// Insert or update a configuration value
    pub async fn upsert(
        &self,
        key: &str,
        value: &JsonValue,
        category: &str,
        description: &str,
        value_type: &str,
        updated_by: Option<&str>,
    ) -> Result<RuntimeConfigEntry> {
        let row = sqlx::query(
            r#"
            INSERT INTO runtime_config (key, value, category, description, value_type, updated_by, updated_at, version)
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), 1)
            ON CONFLICT (key) DO UPDATE SET
                value = EXCLUDED.value,
                description = EXCLUDED.description,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW(),
                version = runtime_config.version + 1
            RETURNING
                key,
                value,
                category,
                description,
                value_type,
                updated_at,
                updated_by,
                version
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(category)
        .bind(description)
        .bind(value_type)
        .bind(updated_by)
        .fetch_one(&self.pool)
        .await
        .map_err(crate::Error::Database)?;

        Ok(RuntimeConfigEntry {
            key: row.get("key"),
            value: row.get("value"),
            category: row.get("category"),
            description: row.get("description"),
            value_type: row.get("value_type"),
            updated_at: row.get("updated_at"),
            updated_by: row.get("updated_by"),
            version: row.get("version"),
        })
    }

    /// Delete a configuration entry (reset to default)
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM runtime_config
            WHERE key = $1
            "#,
        )
        .bind(key)
        .execute(&self.pool)
        .await
        .map_err(crate::Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    /// Get audit log entries
    pub async fn get_audit_log(
        &self,
        key: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RuntimeConfigAuditEntry>> {
        let rows = if let Some(key) = key {
            sqlx::query(
                r#"
                SELECT
                    id,
                    key,
                    old_value,
                    new_value,
                    changed_by,
                    changed_at,
                    change_type
                FROM runtime_config_audit
                WHERE key = $1
                ORDER BY changed_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(key)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?
        } else {
            sqlx::query(
                r#"
                SELECT
                    id,
                    key,
                    old_value,
                    new_value,
                    changed_by,
                    changed_at,
                    change_type
                FROM runtime_config_audit
                ORDER BY changed_at DESC
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?
        };

        Ok(rows
            .into_iter()
            .map(|row| RuntimeConfigAuditEntry {
                id: row.get("id"),
                key: row.get("key"),
                old_value: row.get("old_value"),
                new_value: row.get("new_value"),
                changed_by: row.get("changed_by"),
                changed_at: row.get("changed_at"),
                change_type: row.get("change_type"),
            })
            .collect())
    }

    /// Count audit log entries
    pub async fn count_audit_log(&self, key: Option<&str>) -> Result<i64> {
        let count: i64 = if let Some(key) = key {
            sqlx::query_scalar(
                r#"
                SELECT COUNT(*)
                FROM runtime_config_audit
                WHERE key = $1
                "#,
            )
            .bind(key)
            .fetch_one(&self.pool)
            .await
            .map_err(crate::Error::Database)?
        } else {
            sqlx::query_scalar(
                r#"
                SELECT COUNT(*)
                FROM runtime_config_audit
                "#,
            )
            .fetch_one(&self.pool)
            .await
            .map_err(crate::Error::Database)?
        };

        Ok(count)
    }
}
