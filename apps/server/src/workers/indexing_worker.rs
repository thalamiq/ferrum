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
        &["index_search"]
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
        let job_start = std::time::Instant::now();
        tracing::info!("{} processing job: {}", self.name(), job.id);

        // 1. Parse job parameters
        let parse_start = std::time::Instant::now();
        let params: IndexSearchParams =
            serde_json::from_value(job.parameters.clone()).map_err(|e| {
                crate::Error::Internal(format!("Failed to parse job parameters: {}", e))
            })?;

        let resource_type = params.resource_type;
        let resource_ids = params.resource_ids;
        tracing::debug!("Parsed job parameters in {:?}", parse_start.elapsed());

        tracing::info!(
            "Indexing {} {} resources",
            resource_ids.len(),
            resource_type
        );

        // 2. Load resources from database in batch
        let load_start = std::time::Instant::now();
        let store = PostgresResourceStore::new(self.indexing_service.pool().clone());
        let resources = store
            .load_resources_batch(&resource_type, &resource_ids)
            .await?;
        tracing::info!(
            "Loaded {} resources from database in {:?}",
            resources.len(),
            load_start.elapsed()
        );

        // 3. Batch index all resources (much faster than one-by-one)
        let total = resources.len();
        let index_start = std::time::Instant::now();

        tracing::debug!("Starting batch indexing of {} resources", total);

        // Use auto-batching which chooses COPY-based bulk indexing for batches >= bulk_threshold
        // This eliminates UNIQUE INDEX predicate lock contention by using temp table staging
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

        // Update progress
        let progress_start = std::time::Instant::now();
        self.job_queue
            .update_progress(job.id, total as i32, Some(total as i32), None)
            .await?;
        tracing::debug!("Updated job progress in {:?}", progress_start.elapsed());

        // 5. Mark job complete
        let complete_start = std::time::Instant::now();
        self.job_queue.complete_job(job.id, None).await?;
        tracing::debug!("Marked job complete in {:?}", complete_start.elapsed());

        tracing::info!(
            "{} completed job: {} - indexed {} resources - total time: {:?}",
            self.name(),
            job.id,
            total,
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
