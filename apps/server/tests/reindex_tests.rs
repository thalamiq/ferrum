#![allow(unused)]
#[allow(unused)]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};
use support::*;

/// Register the $reindex OperationDefinition so the operation router accepts it.
async fn setup_reindex(app: &TestApp) -> anyhow::Result<()> {
    let op_def = json!({
        "resourceType": "OperationDefinition",
        "id": "reindex",
        "url": "http://ferrum.fhir.server/OperationDefinition/reindex",
        "status": "active",
        "kind": "operation",
        "code": "reindex",
        "system": true,
        "type": true,
        "instance": true,
        "affectsState": true
    });
    let (status, _headers, _body) = app
        .request(
            Method::POST,
            "/fhir/OperationDefinition",
            Some(to_json_body(&op_def)?),
        )
        .await?;
    assert_status(status, StatusCode::CREATED, "create OperationDefinition");

    app.state.operation_registry.load_definitions().await?;
    Ok(())
}

/// Register a search parameter so we can verify reindex actually rebuilds indexes.
async fn setup_search_param(app: &TestApp) -> anyhow::Result<()> {
    register_search_parameter(
        &app.state.db_pool,
        "family",
        "Patient",
        "string",
        "Patient.name.family",
        &[],
    )
    .await?;
    app.state.search_engine.invalidate_param_cache();
    Ok(())
}

fn parse_json(body: &[u8]) -> anyhow::Result<Value> {
    Ok(serde_json::from_slice(body)?)
}

#[tokio::test]
async fn reindex_system_level_enqueues_jobs() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            setup_reindex(app).await?;
            setup_search_param(app).await?;

            // Create a Patient so there's something to reindex
            let patient = minimal_patient();
            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create Patient");

            // System-level $reindex
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/$reindex", None)
                .await?;
            assert_status(status, StatusCode::OK, "$reindex system-level");

            let result = parse_json(&body)?;
            assert_eq!(result["resourceType"], "Parameters");

            // Should have enqueued at least 1 job (for Patient + OperationDefinition types)
            let jobs_enqueued = result["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "jobsEnqueued")
                .and_then(|p| p["valueInteger"].as_i64())
                .unwrap_or(0);
            assert!(
                jobs_enqueued >= 1,
                "expected at least 1 job enqueued, got {}",
                jobs_enqueued
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn reindex_type_level_enqueues_one_job() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            setup_reindex(app).await?;
            setup_search_param(app).await?;

            let patient = minimal_patient();
            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create Patient");

            // Type-level $reindex
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient/$reindex", None)
                .await?;
            assert_status(status, StatusCode::OK, "$reindex type-level");

            let result = parse_json(&body)?;
            assert_eq!(result["resourceType"], "Parameters");

            let jobs_enqueued = result["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "jobsEnqueued")
                .and_then(|p| p["valueInteger"].as_i64())
                .unwrap_or(0);
            assert_eq!(jobs_enqueued, 1, "type-level should enqueue exactly 1 job");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn reindex_instance_level_enqueues_one_job() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            setup_reindex(app).await?;
            setup_search_param(app).await?;

            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create Patient");

            let created: Value = parse_json(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Instance-level $reindex
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    &format!("/fhir/Patient/{}/$reindex", patient_id),
                    None,
                )
                .await?;
            assert_status(status, StatusCode::OK, "$reindex instance-level");

            let result = parse_json(&body)?;
            assert_eq!(result["resourceType"], "Parameters");

            let jobs_enqueued = result["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == "jobsEnqueued")
                .and_then(|p| p["valueInteger"].as_i64())
                .unwrap_or(0);
            assert_eq!(
                jobs_enqueued, 1,
                "instance-level should enqueue exactly 1 job"
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn reindex_rebuilds_search_indexes() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            setup_reindex(app).await?;
            setup_search_param(app).await?;

            // Create a Patient
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create Patient");

            let created: Value = parse_json(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Delete the search index rows to simulate stale/missing indexes
            sqlx::query(
                "DELETE FROM search_string WHERE resource_type = 'Patient' AND resource_id = $1",
            )
            .bind(patient_id)
            .execute(&app.state.db_pool)
            .await?;

            // Verify index rows are gone
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM search_string WHERE resource_type = 'Patient' AND resource_id = $1 AND parameter_name = 'family'",
            )
            .bind(patient_id)
            .fetch_one(&app.state.db_pool)
            .await?;
            assert_eq!(count, 0, "index rows should be deleted");

            // Run $reindex — uses InlineJobQueue so it executes synchronously
            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    &format!("/fhir/Patient/{}/$reindex", patient_id),
                    None,
                )
                .await?;
            assert_status(status, StatusCode::OK, "$reindex");

            // Verify index rows are rebuilt
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM search_string WHERE resource_type = 'Patient' AND resource_id = $1 AND parameter_name = 'family'",
            )
            .bind(patient_id)
            .fetch_one(&app.state.db_pool)
            .await?;
            assert!(
                count > 0,
                "index rows should be rebuilt after reindex, got {}",
                count
            );

            Ok(())
        })
    })
    .await
}
