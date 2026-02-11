//! Metrics collection for FHIR server
//!
//! This module defines and manages Prometheus metrics for monitoring the FHIR server.

use lazy_static::lazy_static;
use prometheus::{
    register_histogram_vec, register_int_counter_vec, register_int_gauge, register_int_gauge_vec,
    HistogramVec, IntCounterVec, IntGauge, IntGaugeVec,
};

lazy_static! {
    // HTTP Request Metrics

    /// Total HTTP requests by method, path, and status
    pub static ref HTTP_REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_http_requests_total",
        "Total number of HTTP requests",
        &["method", "path", "status"]
    )
    .expect("Failed to register HTTP_REQUESTS_TOTAL");

    /// HTTP request duration in seconds
    pub static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "fhir_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "path"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .expect("Failed to register HTTP_REQUEST_DURATION_SECONDS");

    /// In-flight HTTP requests
    pub static ref HTTP_REQUESTS_IN_FLIGHT: IntGaugeVec = register_int_gauge_vec!(
        "fhir_http_requests_in_flight",
        "Number of HTTP requests currently being processed",
        &["method", "path"]
    )
    .expect("Failed to register HTTP_REQUESTS_IN_FLIGHT");

    /// HTTP request body size in bytes
    pub static ref HTTP_REQUEST_SIZE_BYTES: HistogramVec = register_histogram_vec!(
        "fhir_http_request_size_bytes",
        "HTTP request body size in bytes",
        &["method", "path"],
        vec![100.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0, 10_000_000.0]
    )
    .expect("Failed to register HTTP_REQUEST_SIZE_BYTES");

    /// HTTP response size in bytes
    pub static ref HTTP_RESPONSE_SIZE_BYTES: HistogramVec = register_histogram_vec!(
        "fhir_http_response_size_bytes",
        "HTTP response size in bytes",
        &["method", "path", "status"],
        vec![100.0, 1_000.0, 10_000.0, 100_000.0, 1_000_000.0, 10_000_000.0]
    )
    .expect("Failed to register HTTP_RESPONSE_SIZE_BYTES");

    // FHIR Operation Metrics

    /// FHIR operations by resource type and operation
    pub static ref FHIR_OPERATIONS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_operations_total",
        "Total number of FHIR operations",
        &["resource_type", "operation", "status"]
    )
    .expect("Failed to register FHIR_OPERATIONS_TOTAL");

    /// FHIR operation duration
    pub static ref FHIR_OPERATION_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "fhir_operation_duration_seconds",
        "FHIR operation duration in seconds",
        &["resource_type", "operation"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .expect("Failed to register FHIR_OPERATION_DURATION_SECONDS");

    /// FHIR search operations
    pub static ref FHIR_SEARCH_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_search_total",
        "Total number of FHIR search operations",
        &["resource_type", "status"]
    )
    .expect("Failed to register FHIR_SEARCH_TOTAL");

    /// FHIR search results count
    pub static ref FHIR_SEARCH_RESULTS: HistogramVec = register_histogram_vec!(
        "fhir_search_results",
        "Number of resources returned by search",
        &["resource_type"],
        vec![0.0, 1.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0]
    )
    .expect("Failed to register FHIR_SEARCH_RESULTS");

    /// FHIR batch/transaction operations
    pub static ref FHIR_BATCH_OPERATIONS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_batch_operations_total",
        "Total number of batch/transaction operations",
        &["type", "status"]
    )
    .expect("Failed to register FHIR_BATCH_OPERATIONS_TOTAL");

    /// FHIR batch/transaction entry count
    pub static ref FHIR_BATCH_ENTRIES: HistogramVec = register_histogram_vec!(
        "fhir_batch_entries",
        "Number of entries in batch/transaction",
        &["type"],
        vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0]
    )
    .expect("Failed to register FHIR_BATCH_ENTRIES");

    // Database Metrics

    /// Database query duration
    pub static ref DB_QUERY_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "fhir_db_query_duration_seconds",
        "Database query duration in seconds",
        &["query_type"],
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]
    )
    .expect("Failed to register DB_QUERY_DURATION_SECONDS");

    /// Database query errors
    pub static ref DB_QUERY_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_db_query_errors_total",
        "Total number of database query errors",
        &["query_type", "error_type"]
    )
    .expect("Failed to register DB_QUERY_ERRORS_TOTAL");

    /// Active database connections
    pub static ref DB_CONNECTIONS_ACTIVE: IntGauge = register_int_gauge!(
        "fhir_db_connections_active",
        "Number of active database connections"
    )
    .expect("Failed to register DB_CONNECTIONS_ACTIVE");

    /// Idle database connections
    pub static ref DB_CONNECTIONS_IDLE: IntGauge = register_int_gauge!(
        "fhir_db_connections_idle",
        "Number of idle database connections"
    )
    .expect("Failed to register DB_CONNECTIONS_IDLE");

    // Indexing Metrics

    /// Resources indexed
    pub static ref INDEXING_RESOURCES_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_indexing_resources_total",
        "Total number of resources indexed",
        &["resource_type", "status"]
    )
    .expect("Failed to register INDEXING_RESOURCES_TOTAL");

    /// Indexing duration
    pub static ref INDEXING_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "fhir_indexing_duration_seconds",
        "Resource indexing duration in seconds",
        &["resource_type"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]
    )
    .expect("Failed to register INDEXING_DURATION_SECONDS");

    /// Search parameters indexed per resource
    pub static ref INDEXING_PARAMETERS_COUNT: HistogramVec = register_histogram_vec!(
        "fhir_indexing_parameters_count",
        "Number of search parameters indexed per resource",
        &["resource_type"],
        vec![0.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0]
    )
    .expect("Failed to register INDEXING_PARAMETERS_COUNT");

    // Job Queue Metrics

    /// Jobs enqueued
    pub static ref JOBS_ENQUEUED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_jobs_enqueued_total",
        "Total number of jobs enqueued",
        &["job_type"]
    )
    .expect("Failed to register JOBS_ENQUEUED_TOTAL");

    /// Jobs completed
    pub static ref JOBS_COMPLETED_TOTAL: IntCounterVec = register_int_counter_vec!(
        "fhir_jobs_completed_total",
        "Total number of jobs completed",
        &["job_type", "status"]
    )
    .expect("Failed to register JOBS_COMPLETED_TOTAL");

    /// Job duration
    pub static ref JOB_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "fhir_job_duration_seconds",
        "Job execution duration in seconds",
        &["job_type"],
        vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    )
    .expect("Failed to register JOB_DURATION_SECONDS");

    /// Pending jobs in queue
    pub static ref JOBS_QUEUE_SIZE: IntGaugeVec = register_int_gauge_vec!(
        "fhir_jobs_queue_size",
        "Number of jobs in queue",
        &["status"]
    )
    .expect("Failed to register JOBS_QUEUE_SIZE");

    // Resource Metrics

    /// Total resources by type
    pub static ref RESOURCES_TOTAL: IntGaugeVec = register_int_gauge_vec!(
        "fhir_resources_total",
        "Total number of resources by type",
        &["resource_type"]
    )
    .expect("Failed to register RESOURCES_TOTAL");

    /// Resource versions per resource
    pub static ref RESOURCE_VERSIONS: HistogramVec = register_histogram_vec!(
        "fhir_resource_versions",
        "Number of versions per resource",
        &["resource_type"],
        vec![1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0]
    )
    .expect("Failed to register RESOURCE_VERSIONS");
}

/// Helper to sanitize path for metrics labels (remove IDs, limit cardinality)
pub fn sanitize_path(path: &str) -> String {
    // Remove /fhir prefix if present
    let path = path.strip_prefix("/fhir").unwrap_or(path);

    // Split path into segments
    let segments: Vec<&str> = path.split('/').collect();

    if segments.is_empty() {
        return "/".to_string();
    }

    // Handle different FHIR path patterns
    match segments.len() {
        0 | 1 => path.to_string(),
        2 => {
            // /ResourceType or /metadata
            segments[0..2].join("/")
        }
        3 => {
            // /ResourceType/:id -> /ResourceType/{id}
            format!("{}/{}/{}", segments[0], segments[1], "{id}")
        }
        4 => {
            // /ResourceType/:id/_history -> /ResourceType/{id}/_history
            if segments[3] == "_history" {
                format!("{}/{}/{}/{}", segments[0], segments[1], "{id}", segments[3])
            } else if segments[2].starts_with('$') {
                // /ResourceType/:id/$operation
                format!("{}/{}/{}/{}", segments[0], segments[1], "{id}", segments[3])
            } else {
                // Other patterns
                format!("{}/{}/{}", segments[0], segments[1], "{id}")
            }
        }
        5 => {
            // /ResourceType/:id/_history/:vid -> /ResourceType/{id}/_history/{vid}
            if segments[3] == "_history" {
                format!(
                    "{}/{}/{}/{}/{}",
                    segments[0], segments[1], "{id}", segments[3], "{vid}"
                )
            } else {
                format!("{}/{}/{}", segments[0], segments[1], "{id}")
            }
        }
        _ => {
            // Complex paths, just use first two segments
            segments[0..2].join("/")
        }
    }
}

/// Extract FHIR resource type from path
pub fn extract_resource_type(path: &str) -> Option<String> {
    let path = path
        .strip_prefix("/fhir/")
        .or_else(|| path.strip_prefix("/fhir"))?;
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.is_empty() {
        return None;
    }

    // First segment should be resource type (unless it's a special path)
    let first = segments[0];
    if first.starts_with('_') || first.starts_with('$') || first == "metadata" {
        return None;
    }

    Some(first.to_string())
}

/// Extract FHIR operation from path and method
pub fn extract_operation(method: &str, path: &str) -> Option<String> {
    let path = path
        .strip_prefix("/fhir/")
        .or_else(|| path.strip_prefix("/fhir"))?;
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match method {
        "GET" => {
            if segments.contains(&"_history") {
                Some("history".to_string())
            } else if segments.len() == 1 || path.contains("_search") || path.contains('?') {
                Some("search".to_string())
            } else {
                Some("read".to_string())
            }
        }
        "POST" => {
            if segments.is_empty() || path.contains("_search") {
                Some("search".to_string())
            } else if segments.len() == 1 {
                Some("create".to_string())
            } else {
                Some("custom".to_string())
            }
        }
        "PUT" => Some("update".to_string()),
        "PATCH" => Some("patch".to_string()),
        "DELETE" => Some("delete".to_string()),
        "HEAD" => Some("head".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        assert_eq!(sanitize_path("/fhir/Patient"), "/Patient");
        assert_eq!(sanitize_path("/fhir/Patient/123"), "/Patient/{id}");
        assert_eq!(
            sanitize_path("/fhir/Patient/123/_history"),
            "/Patient/{id}/_history"
        );
        assert_eq!(
            sanitize_path("/fhir/Patient/123/_history/1"),
            "/Patient/{id}/_history/{vid}"
        );
        assert_eq!(sanitize_path("/health"), "/health");
        assert_eq!(sanitize_path("/"), "/");
    }

    #[test]
    fn test_extract_resource_type() {
        assert_eq!(
            extract_resource_type("/fhir/Patient"),
            Some("Patient".to_string())
        );
        assert_eq!(
            extract_resource_type("/fhir/Patient/123"),
            Some("Patient".to_string())
        );
        assert_eq!(extract_resource_type("/fhir/metadata"), None);
        assert_eq!(extract_resource_type("/fhir/_search"), None);
        assert_eq!(extract_resource_type("/health"), None);
    }

    #[test]
    fn test_extract_operation() {
        assert_eq!(
            extract_operation("GET", "/fhir/Patient"),
            Some("search".to_string())
        );
        assert_eq!(
            extract_operation("GET", "/fhir/Patient/123"),
            Some("read".to_string())
        );
        assert_eq!(
            extract_operation("POST", "/fhir/Patient"),
            Some("create".to_string())
        );
        assert_eq!(
            extract_operation("PUT", "/fhir/Patient/123"),
            Some("update".to_string())
        );
        assert_eq!(
            extract_operation("DELETE", "/fhir/Patient/123"),
            Some("delete".to_string())
        );
    }
}
