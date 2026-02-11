//! Background tasks and worker management
//!
//! This module manages background workers that process jobs from the queue.
//! Workers use PostgreSQL LISTEN/NOTIFY for efficient job processing.

use crate::{
    config::Config,
    workers::{
        create_workers, spawn_workers_with_config, WorkerConfig, WorkerRunnerConfig, WorkerState,
    },
    Result,
};

/// Start all background workers
pub async fn start_workers(config: Config) -> Result<()> {
    tracing::info!("Initializing worker environment...");

    // Create worker state
    let worker_state = WorkerState::new(config.clone()).await?;

    let worker_config = WorkerConfig {
        max_concurrent_jobs: config.workers.max_concurrent_jobs,
        poll_interval_seconds: config.workers.poll_interval_seconds,
    };

    let workers = create_workers(&worker_state, worker_config)?;
    let worker_count = workers.len();

    tracing::info!("Starting {} background workers...", worker_count);

    // Spawn all workers
    let runner_config = WorkerRunnerConfig::from_config(&config.workers);
    let _handles =
        spawn_workers_with_config(workers, worker_state.job_queue.clone(), runner_config, None);

    tracing::info!(
        "{} background workers started and listening for jobs",
        worker_count
    );

    Ok(())
}
