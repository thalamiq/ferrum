//! Terminology indexing worker

use super::base::{Worker, WorkerConfig};
use crate::{queue::Job, queue::JobQueue, Result};
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;

pub struct TerminologyWorker {
    #[allow(dead_code)]
    pool: PgPool,
    #[allow(dead_code)]
    job_queue: Arc<dyn JobQueue>,
    #[allow(dead_code)]
    config: WorkerConfig,
}

impl TerminologyWorker {
    pub fn new(pool: PgPool, job_queue: Arc<dyn JobQueue>, config: WorkerConfig) -> Self {
        Self {
            pool,
            job_queue,
            config,
        }
    }
}

#[async_trait]
impl Worker for TerminologyWorker {
    fn name(&self) -> &str {
        "TerminologyWorker"
    }

    fn supported_job_types(&self) -> &[&str] {
        &["index_terminology"]
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
        tracing::info!("{} processing job: {}", self.name(), job.id);
        // TODO: Process CodeSystem/ValueSet indexing
        Ok(())
    }
}
