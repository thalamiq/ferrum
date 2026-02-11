//! Search indexing hook - indexes resources inline for single operations

use super::ResourceHook;
use crate::models::Resource;
use async_trait::async_trait;
use std::sync::Arc;

use crate::{services::IndexingService, Result};

pub struct SearchIndexHook {
    indexing_service: Arc<IndexingService>,
}

impl SearchIndexHook {
    pub fn new(indexing_service: Arc<IndexingService>) -> Self {
        Self { indexing_service }
    }
}

#[async_trait]
impl ResourceHook for SearchIndexHook {
    async fn on_created(&self, resource: &Resource) -> Result<()> {
        // Index inline for single resources (fast operation)
        if let Err(e) = self.indexing_service.index_resource(resource).await {
            tracing::error!(
                "Failed to index resource {}/{}: {}",
                resource.resource_type,
                resource.id,
                e
            );
        }
        Ok(())
    }

    async fn on_updated(&self, resource: &Resource) -> Result<()> {
        // Re-index on update
        if let Err(e) = self.indexing_service.index_resource(resource).await {
            tracing::error!(
                "Failed to index resource {}/{}: {}",
                resource.resource_type,
                resource.id,
                e
            );
        }
        Ok(())
    }

    async fn on_deleted(&self, _resource_type: &str, _id: &str, _version: i32) -> Result<()> {
        // For deletes, search entries are already cleared by the indexing logic
        // No action needed here
        Ok(())
    }

    async fn on_batch_updated(&self, resources: &[Resource]) -> Result<()> {
        // Use batch indexing for efficiency - single transaction per resource type
        if let Err(e) = self.indexing_service.index_resources_batch(resources).await {
            tracing::error!("Failed to batch index {} resources: {}", resources.len(), e);
        }
        Ok(())
    }
}
