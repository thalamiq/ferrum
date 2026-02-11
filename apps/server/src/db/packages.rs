//! Package repository - direct package database operations.

use crate::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRecord {
    pub id: i32,
    pub name: String,
    pub version: String,
    pub status: String,
    /// Count of current, non-deleted resources linked to this package.
    /// Always computed dynamically from resources table on query.
    pub resource_count: i32,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageResourceRecord {
    pub resource_type: String,
    pub resource_id: String,
    pub version_id: i32,
    pub loaded_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub deleted: bool,
    pub resource: JsonValue,
}

#[derive(Clone)]
pub struct PackageRepository {
    pool: PgPool,
}

impl PackageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Return package id if already loaded (or partially loaded).
    pub async fn get_existing_loaded_package_id(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Option<i32>> {
        let row = sqlx::query(
            r#"
            SELECT id
            FROM fhir_packages
            WHERE name = $1 AND version = $2 AND status IN ('loaded', 'partial')
            "#,
        )
        .bind(name)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.get::<i32, _>("id")))
    }

    /// Return package id if any version of the package already exists (loaded or partially loaded).
    pub async fn get_existing_package_id_by_name(
        &self,
        name: &str,
    ) -> Result<Option<(i32, String)>> {
        let row = sqlx::query(
            r#"
            SELECT id, version
            FROM fhir_packages
            WHERE name = $1 AND status IN ('loaded', 'partial')
            LIMIT 1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| (r.get::<i32, _>("id"), r.get::<String, _>("version"))))
    }

    pub async fn mark_package_loading(&self, name: &str, version: &str) -> Result<i32> {
        let row = sqlx::query(
            r#"
            INSERT INTO fhir_packages (name, version, status)
            VALUES ($1, $2, 'loading')
            ON CONFLICT (name, version)
            DO UPDATE SET status = 'loading', created_at = NOW(), error_message = NULL
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(version)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get::<i32, _>("id"))
    }

    /// Finalizes package loading by updating its status and metadata.
    pub async fn finalize_package_load(
        &self,
        name: &str,
        version: &str,
        status: &str,
        metadata: Option<&JsonValue>,
        error_message: Option<&str>,
    ) -> Result<i32> {
        let row = sqlx::query(
            r#"
            INSERT INTO fhir_packages (name, version, status, error_message, metadata)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (name, version)
            DO UPDATE SET
                status = EXCLUDED.status,
                error_message = EXCLUDED.error_message,
                metadata = EXCLUDED.metadata,
                created_at = NOW()
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(version)
        .bind(status)
        .bind(error_message)
        .bind(metadata)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get::<i32, _>("id"))
    }

    pub async fn mark_package_failed(
        &self,
        name: &str,
        version: &str,
        error_message: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE fhir_packages
            SET status = 'failed', error_message = $3, created_at = NOW()
            WHERE name = $1 AND version = $2
            "#,
        )
        .bind(name)
        .bind(version)
        .bind(error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn link_resource_to_package(
        &self,
        package_id: i32,
        resource_type: &str,
        resource_id: &str,
        version_id: i32,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO resource_packages (resource_type, resource_id, version_id, package_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (resource_type, resource_id, version_id, package_id) DO NOTHING
            "#,
        )
        .bind(resource_type)
        .bind(resource_id)
        .bind(version_id)
        .bind(package_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_package(&self, package_id: i32) -> Result<Option<PackageRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                p.id,
                p.name,
                p.version,
                p.status,
                COALESCE(COUNT(r.resource_type), 0)::INT as resource_count,
                p.created_at,
                p.error_message,
                p.metadata
            FROM fhir_packages p
            LEFT JOIN resource_packages rp ON p.id = rp.package_id
            LEFT JOIN resources r ON r.resource_type = rp.resource_type
                AND r.id = rp.resource_id
                AND r.version_id = rp.version_id
                AND r.is_current = TRUE
                AND r.deleted = FALSE
            WHERE p.id = $1
            GROUP BY p.id, p.name, p.version, p.status, p.created_at, p.error_message, p.metadata
            "#,
        )
        .bind(package_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| PackageRecord {
            id: r.get("id"),
            name: r.get("name"),
            version: r.get("version"),
            status: r.get("status"),
            resource_count: r.get("resource_count"),
            created_at: r.get("created_at"),
            error_message: r.get("error_message"),
            metadata: r.get("metadata"),
        }))
    }

    pub async fn list_packages(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<PackageRecord>, i64)> {
        let total: i64 = if let Some(status) = status {
            let row = sqlx::query(
                "SELECT COUNT(*)::BIGINT as count FROM fhir_packages WHERE status = $1",
            )
            .bind(status)
            .fetch_one(&self.pool)
            .await?;
            row.get("count")
        } else {
            let row = sqlx::query("SELECT COUNT(*)::BIGINT as count FROM fhir_packages")
                .fetch_one(&self.pool)
                .await?;
            row.get("count")
        };

        let rows = if let Some(status) = status {
            sqlx::query(
                r#"
                SELECT
                    p.id,
                    p.name,
                    p.version,
                    p.status,
                    COALESCE(COUNT(r.resource_type), 0)::INT as resource_count,
                    p.created_at,
                    p.error_message,
                    p.metadata
                FROM fhir_packages p
                LEFT JOIN resource_packages rp ON p.id = rp.package_id
                LEFT JOIN resources r ON r.resource_type = rp.resource_type
                    AND r.id = rp.resource_id
                    AND r.version_id = rp.version_id
                    AND r.is_current = TRUE
                    AND r.deleted = FALSE
                WHERE p.status = $1
                GROUP BY p.id, p.name, p.version, p.status, p.created_at, p.error_message, p.metadata
                ORDER BY p.created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT
                    p.id,
                    p.name,
                    p.version,
                    p.status,
                    COALESCE(COUNT(r.resource_type), 0)::INT as resource_count,
                    p.created_at,
                    p.error_message,
                    p.metadata
                FROM fhir_packages p
                LEFT JOIN resource_packages rp ON p.id = rp.package_id
                LEFT JOIN resources r ON r.resource_type = rp.resource_type
                    AND r.id = rp.resource_id
                    AND r.version_id = rp.version_id
                    AND r.is_current = TRUE
                    AND r.deleted = FALSE
                GROUP BY p.id, p.name, p.version, p.status, p.created_at, p.error_message, p.metadata
                ORDER BY p.created_at DESC
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        let packages = rows
            .into_iter()
            .map(|r| PackageRecord {
                id: r.get("id"),
                name: r.get("name"),
                version: r.get("version"),
                status: r.get("status"),
                resource_count: r.get("resource_count"),
                created_at: r.get("created_at"),
                error_message: r.get("error_message"),
                metadata: r.get("metadata"),
            })
            .collect();

        Ok((packages, total))
    }

    pub async fn list_package_resources(
        &self,
        package_id: i32,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<(Vec<PackageResourceRecord>, i64)> {
        let total: i64 = sqlx::query(
            "SELECT COUNT(*)::BIGINT as count FROM resource_packages WHERE package_id = $1",
        )
        .bind(package_id)
        .fetch_one(&self.pool)
        .await?
        .get("count");

        let base_query = r#"
            SELECT
                rp.resource_type,
                rp.resource_id,
                rp.version_id,
                rp.loaded_at,
                r.last_updated,
                r.deleted,
                r.resource
            FROM resource_packages rp
            JOIN resources r
              ON r.resource_type = rp.resource_type
             AND r.id = rp.resource_id
             AND r.version_id = rp.version_id
            WHERE rp.package_id = $1
            ORDER BY rp.resource_type, rp.resource_id, rp.version_id
        "#;

        let rows = match (limit, offset) {
            (Some(limit), Some(offset)) => {
                sqlx::query(&format!("{base_query} LIMIT $2 OFFSET $3"))
                    .bind(package_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?
            }
            (Some(limit), None) => {
                sqlx::query(&format!("{base_query} LIMIT $2"))
                    .bind(package_id)
                    .bind(limit)
                    .fetch_all(&self.pool)
                    .await?
            }
            (None, Some(offset)) => {
                sqlx::query(&format!("{base_query} OFFSET $2"))
                    .bind(package_id)
                    .bind(offset)
                    .fetch_all(&self.pool)
                    .await?
            }
            (None, None) => {
                sqlx::query(base_query)
                    .bind(package_id)
                    .fetch_all(&self.pool)
                    .await?
            }
        };

        let resources = rows
            .into_iter()
            .map(|row| PackageResourceRecord {
                resource_type: row.get("resource_type"),
                resource_id: row.get("resource_id"),
                version_id: row.get("version_id"),
                loaded_at: row.get("loaded_at"),
                last_updated: row.get("last_updated"),
                deleted: row.get("deleted"),
                resource: row.get("resource"),
            })
            .collect();

        Ok((resources, total))
    }
}
