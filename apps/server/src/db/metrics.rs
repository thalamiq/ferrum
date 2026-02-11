//! Metrics repository - database queries for metrics and monitoring

use sqlx::PgPool;

/// Repository for metrics database operations
#[derive(Clone)]
pub struct MetricsRepository {
    pool: PgPool,
}

impl MetricsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get job queue size by status
    pub async fn get_job_queue_size(&self, status: &str) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status = $1")
            .bind(status)
            .fetch_one(&self.pool)
            .await?;

        Ok(result.0)
    }

    /// Get connection pool size (for metrics)
    pub fn get_pool_size(&self) -> u32 {
        self.pool.size()
    }

    /// Get number of idle connections (for metrics)
    pub fn get_num_idle(&self) -> usize {
        self.pool.num_idle()
    }
}
