//! Background workers for asynchronous processing
//!
//! Workers poll the job queue and process jobs in the background.
//! Each worker type handles specific job types.

mod base;
mod indexing_worker;
mod package_worker;
mod runner;
mod state;
mod terminology_worker;

pub use base::{Worker, WorkerConfig};
pub use indexing_worker::IndexingWorker;
pub use package_worker::PackageWorker;
pub use runner::{
    run_worker, run_worker_with_config, spawn_workers, spawn_workers_with_config,
    WorkerRunnerConfig,
};
pub use state::WorkerState;
pub use terminology_worker::TerminologyWorker;

use crate::Result;

/// Create all configured workers using lightweight WorkerState
pub fn create_workers(state: &WorkerState, config: WorkerConfig) -> Result<Vec<Box<dyn Worker>>> {
    let mut workers: Vec<Box<dyn Worker>> = Vec::with_capacity(3);

    // Package installation worker
    // Note: registry_url in config is the package registry URL (e.g., https://packages.fhir.org)
    // RegistryClient cache directory defaults to ~/.fhir/packages when None is passed
    workers.push(Box::new(PackageWorker::new(
        state.job_queue.clone(),
        state.indexing_service.clone(),
        None, // Use default cache directory (~/.fhir/packages)
        state
            .config
            .fhir
            .search
            .search_parameter_active_statuses
            .clone(),
        config.clone(),
    )));

    // Search indexing worker
    workers.push(Box::new(IndexingWorker::new(
        state.job_queue.clone(),
        state.indexing_service.clone(),
        config.clone(),
    )));

    // Terminology indexing worker
    workers.push(Box::new(TerminologyWorker::new(
        state.db_pool.clone(),
        state.job_queue.clone(),
        config.clone(),
    )));

    Ok(workers)
}
