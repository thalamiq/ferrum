//! Resource lifecycle hooks

use crate::models::Resource;
use async_trait::async_trait;

use crate::Result;

pub mod compartment_definition;
pub mod computed;
pub mod search_index;
pub mod search_parameter;
pub mod terminology;

/// Hook trait for reacting to resource lifecycle events
#[async_trait]
pub trait ResourceHook: Send + Sync {
    /// Called after a single resource is created
    async fn on_created(&self, resource: &Resource) -> Result<()>;

    /// Called after a single resource is updated
    async fn on_updated(&self, resource: &Resource) -> Result<()>;

    /// Called after a single resource is deleted
    async fn on_deleted(&self, resource_type: &str, id: &str, version: i32) -> Result<()>;

    /// Called after multiple resources are created/updated in batch
    ///
    /// Default implementation calls on_updated for each resource.
    /// Override for more efficient batch processing.
    async fn on_batch_updated(&self, resources: &[Resource]) -> Result<()> {
        for resource in resources {
            self.on_updated(resource).await?;
        }
        Ok(())
    }
}
