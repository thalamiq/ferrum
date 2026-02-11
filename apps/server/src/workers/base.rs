//! Base worker trait and common functionality

use crate::{queue::Job, Result};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub max_concurrent_jobs: usize,
    pub poll_interval_seconds: u64,
}

/// Base trait for all background workers
#[async_trait]
pub trait Worker: Send + Sync {
    /// Get worker name for logging
    fn name(&self) -> &str;

    /// Get supported job types
    fn supported_job_types(&self) -> &[&str];

    /// Start the worker (begins polling for jobs)
    async fn start(&self) -> Result<()>;

    /// Stop the worker gracefully
    async fn stop(&self) -> Result<()>;

    /// Process a single job
    async fn process_job(&self, job: Job) -> Result<()>;
}
