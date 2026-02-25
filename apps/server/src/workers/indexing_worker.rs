//! Search indexing worker

use super::base::{Worker, WorkerConfig};
use crate::{db::PostgresResourceStore, queue::Job, queue::JobQueue, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

pub struct IndexingWorker {
    job_queue: Arc<dyn JobQueue>,
    indexing_service: Arc<crate::services::IndexingService>,
    _config: WorkerConfig,
}

impl IndexingWorker {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        indexing_service: Arc<crate::services::IndexingService>,
        config: WorkerConfig,
    ) -> Self {
        Self {
            job_queue,
            indexing_service,
            _config: config,
        }
    }
}

#[async_trait]
impl Worker for IndexingWorker {
    fn name(&self) -> &str {
        "IndexingWorker"
    }

    fn supported_job_types(&self) -> &[&str] {
        &["index_search", "reindex"]
    }

    async fn start(&self) -> Result<()> {
        tracing::info!("{} starting...", self.name());
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("{} stopping...", self.name());
        Ok(())
    }

    async fn process_job(&self, job: Job) -> Result<()> {
        match job.job_type.as_str() {
            "index_search" => self.process_index_search(job).await,
            "reindex" => self.process_reindex(job).await,
            other => Err(crate::Error::Internal(format!(
                "Unsupported job type: {}",
                other
            ))),
        }
    }
}

impl IndexingWorker {
    async fn process_index_search(&self, job: Job) -> Result<()> {
        let job_start = std::time::Instant::now();
        tracing::info!("{} processing index_search job: {}", self.name(), job.id);

        let params: IndexSearchParams =
            serde_json::from_value(job.parameters.clone()).map_err(|e| {
                crate::Error::Internal(format!("Failed to parse job parameters: {}", e))
            })?;

        let resource_type = params.resource_type;
        let resource_ids = params.resource_ids;

        tracing::info!(
            "Indexing {} {} resources",
            resource_ids.len(),
            resource_type
        );

        let store = PostgresResourceStore::new(self.indexing_service.pool().clone());
        let resources = store
            .load_resources_batch(&resource_type, &resource_ids)
            .await?;

        let total = resources.len();
        let index_start = std::time::Instant::now();

        match self.indexing_service.index_resources_auto(&resources).await {
            Ok(_) => {
                let duration = index_start.elapsed();
                tracing::info!(
                    "Batch indexed {} {} resources in {:?} ({:.2} resources/sec)",
                    total,
                    resource_type,
                    duration,
                    total as f64 / duration.as_secs_f64()
                );
            }
            Err(e) => {
                tracing::error!("Batch indexing failed for {}: {}", resource_type, e);
                return Err(e);
            }
        }

        self.job_queue
            .update_progress(job.id, total as i32, Some(total as i32), None)
            .await?;
        self.job_queue.complete_job(job.id, None).await?;

        tracing::info!(
            "{} completed index_search job: {} - indexed {} resources in {:?}",
            self.name(),
            job.id,
            total,
            job_start.elapsed()
        );
        Ok(())
    }

    async fn process_reindex(&self, job: Job) -> Result<()> {
        let job_start = std::time::Instant::now();
        tracing::info!("{} processing reindex job: {}", self.name(), job.id);

        let params: ReindexParams =
            serde_json::from_value(job.parameters.clone()).map_err(|e| {
                crate::Error::Internal(format!("Failed to parse reindex parameters: {}", e))
            })?;

        let store = PostgresResourceStore::new(self.indexing_service.pool().clone());
        let batch_size: i64 = 500;
        let mut total_indexed: usize = 0;

        if let Some(ref resource_id) = params.resource_id {
            // Single resource reindex
            let resource_type = params.resource_type.as_deref().ok_or_else(|| {
                crate::Error::Internal(
                    "resource_type is required when resource_id is set".to_string(),
                )
            })?;

            let resources = store
                .load_resources_batch(resource_type, &[resource_id.clone()])
                .await?;

            if !resources.is_empty() {
                self.indexing_service
                    .index_resources_auto(&resources)
                    .await?;
                total_indexed = resources.len();
            }
        } else {
            // Type-level reindex: cursor through all resources in batches
            let resource_type = params.resource_type.as_deref();
            let mut after_id: Option<String> = None;

            loop {
                let page = store
                    .list_resource_ids(resource_type, after_id.as_deref(), batch_size)
                    .await?;

                if page.is_empty() {
                    break;
                }

                let is_last_page = (page.len() as i64) < batch_size;

                // Group by resource type for batch loading
                let mut by_type: std::collections::HashMap<String, Vec<String>> =
                    std::collections::HashMap::new();
                for (rt, id) in &page {
                    by_type
                        .entry(rt.clone())
                        .or_default()
                        .push(id.clone());
                }

                for (rt, ids) in &by_type {
                    let resources = store.load_resources_batch(rt, ids).await?;
                    if !resources.is_empty() {
                        self.indexing_service
                            .index_resources_auto(&resources)
                            .await?;
                        total_indexed += resources.len();
                    }
                }

                // Update progress
                self.job_queue
                    .update_progress(job.id, total_indexed as i32, None, None)
                    .await?;

                // Check cancellation
                if self.job_queue.is_cancelled(job.id).await? {
                    tracing::info!("Reindex job {} cancelled after {} resources", job.id, total_indexed);
                    return Ok(());
                }

                if is_last_page {
                    break;
                }

                // Advance cursor to the last ID in this page
                after_id = page.last().map(|(_, id)| id.clone());
            }
        }

        self.job_queue
            .update_progress(job.id, total_indexed as i32, Some(total_indexed as i32), None)
            .await?;
        self.job_queue.complete_job(job.id, None).await?;

        tracing::info!(
            "{} completed reindex job: {} - indexed {} resources in {:?}",
            self.name(),
            job.id,
            total_indexed,
            job_start.elapsed()
        );
        Ok(())
    }
}

/// Job parameters for IndexSearch jobs
#[derive(Debug, Deserialize)]
struct IndexSearchParams {
    resource_type: String,
    resource_ids: Vec<String>,
}

/// Job parameters for Reindex jobs
#[derive(Debug, Deserialize)]
struct ReindexParams {
    resource_type: Option<String>,
    resource_id: Option<String>,
}
