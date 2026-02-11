//! Package admin handlers (internal API).

use crate::{queue::JobPriority, state::AppState, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPackagesQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_packages(
    State(state): State<AppState>,
    Query(q): Query<ListPackagesQuery>,
) -> Result<Response> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);

    let (packages, total) = state
        .package_service
        .list_packages(q.status.as_deref(), limit, offset)
        .await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "packages": packages,
            "total": total,
            "limit": limit,
            "offset": offset
        })),
    )
        .into_response())
}

pub async fn get_package(
    State(state): State<AppState>,
    Path(package_id): Path<i32>,
) -> Result<Response> {
    let pkg = state.package_service.get_package(package_id).await?;

    match pkg {
        Some(pkg) => Ok((StatusCode::OK, Json(pkg)).into_response()),
        None => Err(crate::Error::ResourceNotFound {
            resource_type: "Package".to_string(),
            id: package_id.to_string(),
        }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPackageResourcesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_package_resources(
    State(state): State<AppState>,
    Path(package_id): Path<i32>,
    Query(q): Query<ListPackageResourcesQuery>,
) -> Result<Response> {
    let limit = q.limit.map(|v| v.clamp(1, 10_000));
    let offset = q.offset.map(|v| v.max(0));

    let (resources, total) = state
        .package_service
        .list_package_resources(package_id, limit, offset)
        .await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "packageId": package_id,
            "resources": resources,
            "total": total,
            "limit": limit,
            "offset": offset
        })),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPackageRequest {
    pub name: String,
    pub version: Option<String>,
    pub include_dependencies: Option<bool>,
    pub include_examples: Option<bool>,
    pub include_resource_types: Option<Vec<String>>,
    pub exclude_resource_types: Option<Vec<String>>,
}

pub async fn install_package(
    State(state): State<AppState>,
    Json(req): Json<InstallPackageRequest>,
) -> Result<Response> {
    if req.name.trim().is_empty() {
        return Err(crate::Error::Validation(
            "Package name is required".to_string(),
        ));
    }

    // Validate filter
    let filter = crate::config::ResourceTypeFilter {
        include_resource_types: req.include_resource_types.clone(),
        exclude_resource_types: req.exclude_resource_types.clone(),
    };
    if let Err(e) = filter.validate() {
        return Err(crate::Error::Validation(format!(
            "Invalid resource type filter: {}",
            e
        )));
    }

    let include_dependencies = req.include_dependencies.unwrap_or(true);
    let include_examples = req.include_examples.unwrap_or(false);

    // Queue package installation job
    let mut params = serde_json::json!({
        "package_name": req.name,
        "package_version": req.version,
        "include_dependencies": include_dependencies,
        "include_examples": include_examples,
    });

    // Add filter params if specified
    if let Some(include_types) = &req.include_resource_types {
        params.as_object_mut().unwrap().insert(
            "include_resource_types".to_string(),
            serde_json::json!(include_types),
        );
    }
    if let Some(exclude_types) = &req.exclude_resource_types {
        params.as_object_mut().unwrap().insert(
            "exclude_resource_types".to_string(),
            serde_json::json!(exclude_types),
        );
    }

    let job_id = state
        .job_queue
        .enqueue(
            "install_package".to_string(),
            params,
            JobPriority::Normal,
            None,
        )
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "jobId": job_id,
            "name": req.name,
            "version": req.version,
            "includeDependencies": include_dependencies,
            "includeExamples": include_examples
        })),
    )
        .into_response())
}
