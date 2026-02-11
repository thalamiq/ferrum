//! Metrics service for collecting application metrics

use crate::db::MetricsRepository;

/// Service for collecting application metrics
pub struct MetricsService {
    repo: MetricsRepository,
}

impl MetricsService {
    pub fn new(repo: MetricsRepository) -> Self {
        Self { repo }
    }

    /// Get job queue size by status
    pub async fn get_job_queue_size(&self, status: &str) -> Result<i64, sqlx::Error> {
        self.repo.get_job_queue_size(status).await
    }

    /// Update database connection pool metrics
    pub fn update_db_connection_metrics(&self) {
        let pool_size = self.repo.get_pool_size();
        let idle = self.repo.get_num_idle() as u32;

        crate::metrics::DB_CONNECTIONS_ACTIVE.set((pool_size - idle) as i64);
        crate::metrics::DB_CONNECTIONS_IDLE.set(idle as i64);
    }

    /// Update all job queue metrics
    pub async fn update_job_queue_metrics(&self) {
        // Get pending jobs
        if let Ok(pending) = self.get_job_queue_size("pending").await {
            crate::metrics::JOBS_QUEUE_SIZE
                .with_label_values(&["pending"])
                .set(pending);
        }

        // Get running jobs
        if let Ok(running) = self.get_job_queue_size("running").await {
            crate::metrics::JOBS_QUEUE_SIZE
                .with_label_values(&["running"])
                .set(running);
        }

        // Get failed jobs (for monitoring)
        if let Ok(failed) = self.get_job_queue_size("failed").await {
            crate::metrics::JOBS_QUEUE_SIZE
                .with_label_values(&["failed"])
                .set(failed);
        }
    }

    /// Collect all custom application metrics
    pub async fn collect_custom_metrics(&self, server_version: &str, fhir_version: &str) -> String {
        let mut output = String::new();

        // Update real-time metrics
        self.update_db_connection_metrics();
        self.update_job_queue_metrics().await;

        // Server info
        output.push_str("# HELP fhir_server_info FHIR server information\n");
        output.push_str("# TYPE fhir_server_info gauge\n");
        output.push_str(&format!(
            "fhir_server_info{{version=\"{}\",fhir_version=\"{}\"}} 1\n",
            server_version, fhir_version
        ));

        output
    }
}
