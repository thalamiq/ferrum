//! Job queue domain models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "job_status", rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Retrying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(i32)]
pub enum JobPriority {
    Low = 0,
    Normal = 5,
    High = 10,
    Critical = 20,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: i32,
    pub initial_delay_seconds: i32,
    pub max_delay_seconds: i32,
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_seconds: 60,
            max_delay_seconds: 3600,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    pub fn calculate_delay(&self, retry_count: i32) -> i32 {
        let delay = self.initial_delay_seconds as f64 * self.backoff_multiplier.powi(retry_count);
        delay.min(self.max_delay_seconds as f64) as i32
    }
}

/// Job types for background processing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum JobType {
    /// Index search parameters for a batch of resources
    IndexSearch {
        resource_type: String,
        resource_ids: Vec<String>,
    },
    /// Index terminology for a CodeSystem or ValueSet
    IndexTerminology { resource_id: String },
    /// Index compartment memberships
    IndexCompartment { compartment_id: String },
    /// Update search parameter definitions
    UpdateSearchParameters { parameter_url: String },
    /// Install FHIR package from registry
    InstallPackage {
        package_name: String,
        package_version: Option<String>,
        include_dependencies: bool,
        include_examples: bool,
    },
}

impl JobType {
    pub fn job_type_name(&self) -> &'static str {
        match self {
            JobType::IndexSearch { .. } => "index_search",
            JobType::IndexTerminology { .. } => "index_terminology",
            JobType::IndexCompartment { .. } => "index_compartment",
            JobType::UpdateSearchParameters { .. } => "update_search_parameters",
            JobType::InstallPackage { .. } => "install_package",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub job_type: String,
    #[sqlx(try_from = "String")]
    pub status: JobStatus,
    pub priority: i32,
    pub parameters: serde_json::Value,
    pub progress: Option<serde_json::Value>,
    pub retry_policy: serde_json::Value,
    pub retry_count: i32,
    pub processed_items: i32,
    pub total_items: Option<i32>,
    pub error_message: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub cancel_requested: bool,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub worker_id: Option<String>,
}

// Conversion from DB string to JobStatus
impl TryFrom<String> for JobStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "pending" => Ok(JobStatus::Pending),
            "running" => Ok(JobStatus::Running),
            "completed" => Ok(JobStatus::Completed),
            "failed" => Ok(JobStatus::Failed),
            "cancelled" => Ok(JobStatus::Cancelled),
            "retrying" => Ok(JobStatus::Retrying),
            _ => Err(format!("Invalid job status: {}", value)),
        }
    }
}

impl Job {
    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }

    pub fn can_retry(&self) -> bool {
        if self.status != JobStatus::Failed {
            return false;
        }

        // Parse retry_policy from JSON
        if let Ok(policy) = serde_json::from_value::<RetryPolicy>(self.retry_policy.clone()) {
            return self.retry_count < policy.max_retries;
        }

        false
    }

    pub fn get_retry_policy(&self) -> RetryPolicy {
        serde_json::from_value(self.retry_policy.clone()).unwrap_or_default()
    }

    pub fn get_priority(&self) -> JobPriority {
        match self.priority {
            0 => JobPriority::Low,
            10 => JobPriority::High,
            20 => JobPriority::Critical,
            _ => JobPriority::Normal,
        }
    }

    pub fn progress_percent(&self) -> Option<f64> {
        if let Some(total) = self.total_items {
            if total > 0 {
                return Some((self.processed_items as f64 / total as f64) * 100.0);
            }
        }
        None
    }
}
