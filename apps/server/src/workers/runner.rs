//! Worker runner that executes workers using the job queue

use super::base::Worker;
use crate::{queue::JobQueue, Result};
use futures::StreamExt;
use std::sync::Arc;
use tokio::{
    sync::watch,
    time::{sleep, Duration},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct WorkerRunnerConfig {
    pub reconnect_initial: Duration,
    pub reconnect_max: Duration,
    pub reconnect_jitter_ratio: f64,
}

impl WorkerRunnerConfig {
    pub fn from_config(config: &crate::config::WorkerConfig) -> Self {
        Self {
            reconnect_initial: Duration::from_secs(config.reconnect_initial_seconds),
            reconnect_max: Duration::from_secs(config.reconnect_max_seconds),
            reconnect_jitter_ratio: config.reconnect_jitter_ratio,
        }
    }
}

impl Default for WorkerRunnerConfig {
    fn default() -> Self {
        Self {
            reconnect_initial: Duration::from_secs(1),
            reconnect_max: Duration::from_secs(30),
            reconnect_jitter_ratio: 0.2,
        }
    }
}

fn jittered_duration(base: Duration, jitter_ratio: f64) -> Duration {
    if base.is_zero() || jitter_ratio <= 0.0 {
        return base;
    }

    // Deterministic-enough jitter source without adding a new RNG dependency.
    let bytes = *Uuid::new_v4().as_bytes();
    let value = u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"));
    let unit = (value as f64) / (u64::MAX as f64); // [0,1]
    let signed = unit * 2.0 - 1.0; // [-1,1]
    let factor = (1.0 + signed * jitter_ratio).max(0.0);
    base.mul_f64(factor)
}

/// Run a worker by listening to the job queue and processing jobs
pub async fn run_worker(worker: Arc<dyn Worker>, job_queue: Arc<dyn JobQueue>) -> Result<()> {
    run_worker_with_config(worker, job_queue, WorkerRunnerConfig::default(), None).await
}

pub async fn run_worker_with_config(
    worker: Arc<dyn Worker>,
    job_queue: Arc<dyn JobQueue>,
    runner_config: WorkerRunnerConfig,
    mut shutdown: Option<watch::Receiver<bool>>,
) -> Result<()> {
    let job_types: Vec<String> = worker
        .supported_job_types()
        .iter()
        .map(|s| s.to_string())
        .collect();

    tracing::info!(
        "{} starting to listen for job types: {:?}",
        worker.name(),
        job_types
    );

    worker.start().await?;

    let mut reconnect_delay = runner_config.reconnect_initial;

    loop {
        if let Some(rx) = shutdown.as_ref() {
            if *rx.borrow() {
                tracing::info!("{} shutdown requested, stopping...", worker.name());
                worker.stop().await?;
                return Ok(());
            }
        }

        // (Re)create the job stream. LISTEN/NOTIFY connections can drop; when that happens the
        // stream ends and we need to re-establish the listener.
        let mut job_stream = match job_queue.listen(&job_types).await {
            Ok(stream) => {
                // Connection established: reset backoff.
                reconnect_delay = runner_config.reconnect_initial;
                stream
            }
            Err(e) => {
                tracing::error!(
                    "{} failed to create job listener: {} (reconnecting in {:?})",
                    worker.name(),
                    e,
                    reconnect_delay
                );
                let sleep_for =
                    jittered_duration(reconnect_delay, runner_config.reconnect_jitter_ratio);
                sleep(sleep_for).await;
                reconnect_delay = (reconnect_delay * 2).min(runner_config.reconnect_max);
                continue;
            }
        };

        // Process jobs as they come in
        loop {
            tokio::select! {
                _ = async {
                    if let Some(rx) = shutdown.as_mut() {
                        let _ = rx.changed().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    if let Some(rx) = shutdown.as_ref() {
                        if *rx.borrow() {
                            tracing::info!("{} shutdown requested, stopping...", worker.name());
                            worker.stop().await?;
                            return Ok(());
                        }
                    }
                }
                next = job_stream.next() => {
                    match next {
                        Some(Ok(job)) => {
                            tracing::info!("{} received job: {}", worker.name(), job.id);
                            match worker.process_job(job).await {
                                Ok(()) => tracing::info!("{} successfully processed job", worker.name()),
                                Err(e) => tracing::error!("{} failed to process job: {}", worker.name(), e),
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("{} error receiving job: {}", worker.name(), e);
                        }
                        None => break,
                    }
                }
            }
        }

        tracing::warn!(
            "{} job stream ended (db connection lost?), reconnecting in {:?}",
            worker.name(),
            reconnect_delay
        );
        let sleep_for = jittered_duration(reconnect_delay, runner_config.reconnect_jitter_ratio);
        sleep(sleep_for).await;
        reconnect_delay = (reconnect_delay * 2).min(runner_config.reconnect_max);
    }
}

/// Spawn multiple workers
pub fn spawn_workers(
    workers: Vec<Box<dyn Worker>>,
    job_queue: Arc<dyn JobQueue>,
) -> Vec<tokio::task::JoinHandle<Result<()>>> {
    spawn_workers_with_config(workers, job_queue, WorkerRunnerConfig::default(), None)
}

pub fn spawn_workers_with_config(
    workers: Vec<Box<dyn Worker>>,
    job_queue: Arc<dyn JobQueue>,
    runner_config: WorkerRunnerConfig,
    shutdown: Option<watch::Receiver<bool>>,
) -> Vec<tokio::task::JoinHandle<Result<()>>> {
    workers
        .into_iter()
        .map(|worker| {
            let worker_arc: Arc<dyn Worker> = Arc::from(worker);
            let queue = job_queue.clone();
            let cfg = runner_config.clone();
            let shutdown_rx = shutdown.clone();
            tokio::spawn(async move {
                run_worker_with_config(worker_arc, queue, cfg, shutdown_rx).await
            })
        })
        .collect()
}
