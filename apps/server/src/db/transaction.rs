//! PostgreSQL transaction support for atomic operations

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{Postgres, Row, Transaction};

use super::{
    traits::{ResourceTransaction, TransactionContext},
    PostgresResourceStore,
};
use crate::{models::Resource, Error, Result};

/// PostgreSQL transaction context
pub struct PostgresTransactionContext {
    tx: Option<Transaction<'static, Postgres>>,
}

impl PostgresTransactionContext {
    pub fn new(tx: Transaction<'static, Postgres>) -> Self {
        Self { tx: Some(tx) }
    }

    pub async fn version_exists(
        &mut self,
        resource_type: &str,
        id: &str,
        version_id: i32,
    ) -> Result<bool> {
        let tx = self.tx_mut()?;

        let row = sqlx::query(
            "SELECT 1
             FROM resources
             WHERE resource_type = $1 AND id = $2 AND version_id = $3",
        )
        .bind(resource_type)
        .bind(id)
        .bind(version_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.is_some())
    }

    /// Physically delete a resource and its full version history inside this transaction.
    ///
    /// Returns the number of rows removed from the `resources` table.
    pub async fn hard_delete(&mut self, resource_type: &str, id: &str) -> Result<u64> {
        let tx = self.tx_mut()?;

        let resources_deleted = sqlx::query(
            "DELETE FROM resources
             WHERE resource_type = $1 AND id = $2",
        )
        .bind(resource_type)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?
        .rows_affected();

        let _ = sqlx::query(
            "DELETE FROM resource_versions
             WHERE resource_type = $1 AND id = $2",
        )
        .bind(resource_type)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(resources_deleted)
    }

    /// Replace the JSON payload of the current version without creating a new version.
    ///
    /// This is used for transaction-time reference rewriting (e.g. turning versionless references
    /// into version-specific references) while keeping the resulting version id stable.
    pub async fn update_current_resource_json(
        &mut self,
        resource_type: &str,
        id: &str,
        version_id: i32,
        resource: &JsonValue,
    ) -> Result<()> {
        let tx = self.tx_mut()?;

        sqlx::query(
            "UPDATE resources
             SET resource = $4
             WHERE resource_type = $1
               AND id = $2
               AND version_id = $3
               AND is_current = true",
        )
        .bind(resource_type)
        .bind(id)
        .bind(version_id)
        .bind(resource)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Get mutable reference to transaction
    pub(crate) fn tx_mut(&mut self) -> Result<&mut Transaction<'static, Postgres>> {
        self.tx.as_mut().ok_or_else(|| {
            Error::Internal("Transaction already committed or rolled back".to_string())
        })
    }
}

#[async_trait]
impl TransactionContext for PostgresTransactionContext {
    async fn commit(mut self) -> Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| Error::Internal("Transaction already committed".to_string()))?;

        tx.commit().await.map_err(Error::Database)
    }

    async fn rollback(mut self) -> Result<()> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| Error::Internal("Transaction already rolled back".to_string()))?;

        tx.rollback().await.map_err(Error::Database)
    }

    async fn create(&mut self, resource_type: &str, resource: JsonValue) -> Result<Resource> {
        let id = resource
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidResource("Missing id field".to_string()))?
            .to_string();

        let now =
            PostgresResourceStore::extract_meta_last_updated(&resource).unwrap_or_else(Utc::now);
        let url = PostgresResourceStore::extract_url(&resource);
        let meta_source = PostgresResourceStore::extract_meta_source(&resource);
        let meta_tags = PostgresResourceStore::extract_meta_tags(&resource);

        let tx = self.tx_mut()?;

        let version_row = sqlx::query(
            "INSERT INTO resource_versions (resource_type, id, next_version)
             VALUES ($1, $2, 1)
             ON CONFLICT (resource_type, id)
             DO UPDATE SET next_version = resource_versions.next_version + 1
             RETURNING next_version",
        )
        .bind(resource_type)
        .bind(&id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let version_id: i32 = version_row.get("next_version");

        sqlx::query(
            "INSERT INTO resources (id, resource_type, version_id, resource, last_updated, url, meta_source, meta_tags, deleted, is_current)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, true)",
        )
        .bind(&id)
        .bind(resource_type)
        .bind(version_id)
        .bind(&resource)
        .bind(now)
        .bind(url)
        .bind(meta_source)
        .bind(meta_tags)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(Resource {
            id,
            resource_type: resource_type.to_string(),
            version_id,
            resource,
            last_updated: now,
            deleted: false,
        })
    }

    async fn upsert(
        &mut self,
        resource_type: &str,
        id: &str,
        resource: JsonValue,
    ) -> Result<Resource> {
        let now =
            PostgresResourceStore::extract_meta_last_updated(&resource).unwrap_or_else(Utc::now);
        let url = PostgresResourceStore::extract_url(&resource);
        let meta_source = PostgresResourceStore::extract_meta_source(&resource);
        let meta_tags = PostgresResourceStore::extract_meta_tags(&resource);

        let tx = self.tx_mut()?;

        let version_row = sqlx::query(
            "INSERT INTO resource_versions (resource_type, id, next_version)
             VALUES ($1, $2, 1)
             ON CONFLICT (resource_type, id)
             DO UPDATE SET next_version = resource_versions.next_version + 1
             RETURNING next_version",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let version_id: i32 = version_row.get("next_version");

        sqlx::query(
            "UPDATE resources SET is_current = false
             WHERE resource_type = $1 AND id = $2 AND is_current = true",
        )
        .bind(resource_type)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        sqlx::query(
            "INSERT INTO resources (id, resource_type, version_id, resource, last_updated, url, meta_source, meta_tags, deleted, is_current)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, true)",
        )
        .bind(id)
        .bind(resource_type)
        .bind(version_id)
        .bind(&resource)
        .bind(now)
        .bind(url)
        .bind(meta_source)
        .bind(meta_tags)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(Resource {
            id: id.to_string(),
            resource_type: resource_type.to_string(),
            version_id,
            resource,
            last_updated: now,
            deleted: false,
        })
    }

    async fn read(&mut self, resource_type: &str, id: &str) -> Result<Option<Resource>> {
        let tx = self.tx_mut()?;

        let current = sqlx::query(
            "SELECT id, resource_type, version_id, resource, last_updated, deleted
             FROM resources
             WHERE resource_type = $1 AND id = $2 AND is_current = true",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(current.map(|row| Resource {
            id: row.get("id"),
            resource_type: row.get("resource_type"),
            version_id: row.get("version_id"),
            resource: row.get("resource"),
            last_updated: row.get("last_updated"),
            deleted: row.get("deleted"),
        }))
    }

    async fn delete(&mut self, resource_type: &str, id: &str) -> Result<i32> {
        let tx = self.tx_mut()?;

        let current = sqlx::query(
            "SELECT version_id, deleted FROM resources
             WHERE resource_type = $1 AND id = $2 AND is_current = true",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?
        .ok_or_else(|| Error::ResourceNotFound {
            resource_type: resource_type.to_string(),
            id: id.to_string(),
        })?;

        let is_deleted: bool = current.get("deleted");

        if is_deleted {
            let current_version: i32 = current.get("version_id");
            return Ok(current_version);
        }

        let version_row = sqlx::query(
            "INSERT INTO resource_versions (resource_type, id, next_version)
             VALUES ($1, $2, 1)
             ON CONFLICT (resource_type, id)
             DO UPDATE SET next_version = resource_versions.next_version + 1
             RETURNING next_version",
        )
        .bind(resource_type)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let new_version: i32 = version_row.get("next_version");
        let now = Utc::now();

        sqlx::query(
            "UPDATE resources SET is_current = false
             WHERE resource_type = $1 AND id = $2 AND is_current = true",
        )
        .bind(resource_type)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let resource = serde_json::json!({
            "resourceType": resource_type,
            "id": id
        });

        sqlx::query(
            "INSERT INTO resources (id, resource_type, version_id, resource, last_updated, url, meta_source, meta_tags, deleted, is_current)
             VALUES ($1, $2, $3, $4, $5, NULL, NULL, NULL, true, true)",
        )
        .bind(id)
        .bind(resource_type)
        .bind(new_version)
        .bind(resource)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(new_version)
    }
}

#[async_trait]
impl ResourceTransaction for PostgresResourceStore {
    type Context = PostgresTransactionContext;

    async fn begin_transaction(&self) -> Result<Self::Context> {
        let tx = self.pool.begin().await.map_err(Error::Database)?;

        // SAFETY: This extends the lifetime to 'static
        // This is safe because the transaction will be consumed by commit/rollback
        let tx: Transaction<'static, Postgres> = unsafe { std::mem::transmute(tx) };

        Ok(PostgresTransactionContext::new(tx))
    }
}
