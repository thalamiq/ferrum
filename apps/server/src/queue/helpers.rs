//! Helper functions for job queue operations

use super::models::Job;
use crate::Result;
use sqlx::PgPool;

/// Try to dequeue a job without blocking
pub async fn try_dequeue_job(
    pool: &PgPool,
    job_types: &[String],
    worker_id: &str,
) -> Result<Option<Job>> {
    let now = chrono::Utc::now();

    let result = sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs
        SET status = 'running',
            started_at = $1,
            worker_id = $2
        WHERE id = (
            SELECT id
            FROM jobs
            WHERE job_type = ANY($3)
              AND status = 'pending'
              AND cancel_requested = FALSE
              AND (scheduled_at IS NULL OR scheduled_at <= $1)
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, job_type, status, priority, parameters, progress,
                  retry_policy, retry_count, processed_items, total_items,
                  error_message, last_error_at, scheduled_at, cancel_requested,
                  created_at, started_at, completed_at, worker_id
        "#,
    )
    .bind(now)
    .bind(worker_id)
    .bind(job_types)
    .fetch_optional(pool)
    .await
    .map_err(crate::Error::Database)?;

    Ok(result)
}

/// List jobs with filtering and pagination
pub async fn list_jobs(
    pool: &PgPool,
    job_type: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Job>, i64)> {
    let mut query = String::from(
        "SELECT id, job_type, status, priority, parameters, progress, \
         retry_policy, retry_count, processed_items, total_items, \
         error_message, last_error_at, scheduled_at, cancel_requested, \
         created_at, started_at, completed_at, worker_id \
         FROM jobs WHERE 1=1",
    );
    let mut count_query = String::from("SELECT COUNT(*) FROM jobs WHERE 1=1");
    let mut params: Vec<String> = Vec::new();

    if let Some(jt) = job_type {
        params.push(jt.to_string());
        let param_num = params.len();
        query.push_str(&format!(" AND job_type = ${}", param_num));
        count_query.push_str(&format!(" AND job_type = ${}", param_num));
    }

    if let Some(s) = status {
        params.push(s.to_string());
        let param_num = params.len();
        query.push_str(&format!(" AND status = ${}", param_num));
        count_query.push_str(&format!(" AND status = ${}", param_num));
    }

    query.push_str(" ORDER BY created_at DESC");
    let limit_param = params.len() + 1;
    let offset_param = params.len() + 2;
    query.push_str(&format!(" LIMIT ${} OFFSET ${}", limit_param, offset_param));

    // Get total count
    let total: i64 = if !params.is_empty() {
        let mut q = sqlx::query_scalar(&count_query);
        for param in &params {
            q = q.bind(param);
        }
        q.fetch_one(pool).await.map_err(crate::Error::Database)?
    } else {
        sqlx::query_scalar(&count_query)
            .fetch_one(pool)
            .await
            .map_err(crate::Error::Database)?
    };

    // Get jobs
    let jobs: Vec<Job> = if !params.is_empty() {
        let mut q = sqlx::query_as(&query);
        for param in &params {
            q = q.bind(param);
        }
        q.bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(crate::Error::Database)?
    } else {
        sqlx::query_as(&query)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(crate::Error::Database)?
    };

    Ok((jobs, total))
}
