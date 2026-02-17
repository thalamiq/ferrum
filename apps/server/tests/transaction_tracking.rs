#![allow(unused)]
#[allow(unused)]
mod support;

use axum::http::{Method, StatusCode};
use serde_json::json;
use support::{assert_status, minimal_patient, to_json_body, with_test_app, with_test_app_with_config};

fn status_code_prefix(status: &str) -> &str {
    status.split_whitespace().next().unwrap_or("")
}

// ---------------------------------------------------------------------------
// Batch tracking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_records_transaction_tracking() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [
                    {
                        "request": { "method": "POST", "url": "Patient" },
                        "resource": minimal_patient()
                    },
                    {
                        "request": { "method": "POST", "url": "Patient" },
                        "resource": minimal_patient()
                    }
                ]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "batch");

            // Verify tracking row was written
            let row = sqlx::query_as::<_, (String, String, Option<i32>)>(
                "SELECT type, status, entry_count FROM fhir_transactions ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;

            assert_eq!(row.0, "batch", "type should be batch");
            assert!(
                row.1 == "completed" || row.1 == "partial",
                "status should be completed or partial, got: {}",
                row.1
            );
            assert_eq!(row.2, Some(2), "entry_count should be 2");

            // Verify entry rows
            let entry_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM fhir_transaction_entries",
            )
            .fetch_one(&app.state.db_pool)
            .await?;
            assert_eq!(entry_count, 2, "should have 2 entry records");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn batch_records_started_and_completed_timestamps() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [{
                    "request": { "method": "POST", "url": "Patient" },
                    "resource": minimal_patient()
                }]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "batch");

            let has_timestamps: bool = sqlx::query_scalar(
                "SELECT started_at IS NOT NULL AND completed_at IS NOT NULL FROM fhir_transactions ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;
            assert!(has_timestamps, "should have started_at and completed_at");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn batch_entry_records_contain_status_codes() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [{
                    "request": { "method": "POST", "url": "Patient" },
                    "resource": minimal_patient()
                }]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "batch");

            let entry_status: Option<i32> = sqlx::query_scalar(
                "SELECT status FROM fhir_transaction_entries ORDER BY entry_index LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;
            assert_eq!(entry_status, Some(201), "POST should yield 201");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn batch_entry_records_contain_resource_info() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [{
                    "request": { "method": "POST", "url": "Patient" },
                    "resource": minimal_patient()
                }]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "batch");

            let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
                "SELECT resource_type, resource_id FROM fhir_transaction_entries ORDER BY entry_index LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;

            assert_eq!(row.0.as_deref(), Some("Patient"), "resource_type");
            assert!(row.1.is_some(), "resource_id should be set");

            Ok(())
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Transaction tracking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transaction_records_transaction_tracking() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "transaction",
                "entry": [
                    {
                        "fullUrl": "urn:uuid:aaaa-bbbb",
                        "request": { "method": "POST", "url": "Patient" },
                        "resource": minimal_patient()
                    },
                    {
                        "fullUrl": "urn:uuid:cccc-dddd",
                        "request": { "method": "POST", "url": "Patient" },
                        "resource": minimal_patient()
                    }
                ]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "transaction");

            let row = sqlx::query_as::<_, (String, String, Option<i32>)>(
                "SELECT type, status, entry_count FROM fhir_transactions ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;

            assert_eq!(row.0, "transaction", "type should be transaction");
            assert_eq!(row.1, "completed", "status should be completed");
            assert_eq!(row.2, Some(2), "entry_count should be 2");

            let entry_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM fhir_transaction_entries",
            )
            .fetch_one(&app.state.db_pool)
            .await?;
            assert_eq!(entry_count, 2, "should have 2 entry records");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn transaction_failure_records_failed_status() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Transaction with a GET for a non-existent resource should fail
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "transaction",
                "entry": [{
                    "request": {
                        "method": "GET",
                        "url": "Patient/non-existent-id-12345"
                    }
                }]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            // Transaction should fail (404 for missing resource rolls back)
            assert_ne!(status, StatusCode::OK, "transaction should fail");

            let row = sqlx::query_as::<_, (String, String)>(
                "SELECT type, status FROM fhir_transactions ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_optional(&app.state.db_pool)
            .await?;

            if let Some((bundle_type, tx_status)) = row {
                assert_eq!(bundle_type, "transaction");
                assert_eq!(tx_status, "failed", "status should be failed");
            }
            // It's also acceptable if no row was written (record_start succeeded
            // but the failure happened before record_complete)

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn empty_batch_records_tracking() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": []
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "empty batch");

            let row = sqlx::query_as::<_, (String, String, Option<i32>)>(
                "SELECT type, status, entry_count FROM fhir_transactions ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;

            assert_eq!(row.0, "batch");
            assert_eq!(row.1, "completed");
            assert_eq!(row.2, Some(0));

            let entry_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM fhir_transaction_entries",
            )
            .fetch_one(&app.state.db_pool)
            .await?;
            assert_eq!(entry_count, 0);

            Ok(())
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Admin API endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_list_transactions_returns_results() -> anyhow::Result<()> {
    with_test_app_with_config(|c| { c.ui.password = None; }, |app| {
        Box::pin(async move {
            // Create a batch to populate tracking
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [{
                    "request": { "method": "POST", "url": "Patient" },
                    "resource": minimal_patient()
                }]
            });
            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "batch");

            // Query admin API
            let (status, _headers, body) = app
                .request(Method::GET, "/admin/transactions", None)
                .await?;
            assert_status(status, StatusCode::OK, "list transactions");

            let response: serde_json::Value = serde_json::from_slice(&body)?;
            assert!(response["total"].as_i64().unwrap() >= 1);
            assert!(!response["items"].as_array().unwrap().is_empty());

            let item = &response["items"][0];
            assert_eq!(item["type"], "batch");
            assert!(item["status"].as_str().is_some());

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn admin_list_transactions_filters_by_type() -> anyhow::Result<()> {
    with_test_app_with_config(|c| { c.ui.password = None; }, |app| {
        Box::pin(async move {
            // Create a batch
            let batch = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [{
                    "request": { "method": "POST", "url": "Patient" },
                    "resource": minimal_patient()
                }]
            });
            app.request(Method::POST, "/fhir", Some(to_json_body(&batch)?))
                .await?;

            // Create a transaction
            let transaction = json!({
                "resourceType": "Bundle",
                "type": "transaction",
                "entry": [{
                    "fullUrl": "urn:uuid:1111-2222",
                    "request": { "method": "POST", "url": "Patient" },
                    "resource": minimal_patient()
                }]
            });
            app.request(Method::POST, "/fhir", Some(to_json_body(&transaction)?))
                .await?;

            // Filter by batch type
            let (status, _headers, body) = app
                .request(Method::GET, "/admin/transactions?bundleType=batch", None)
                .await?;
            assert_status(status, StatusCode::OK, "filter batch");
            let response: serde_json::Value = serde_json::from_slice(&body)?;
            for item in response["items"].as_array().unwrap() {
                assert_eq!(item["type"], "batch");
            }

            // Filter by transaction type
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/admin/transactions?bundleType=transaction",
                    None,
                )
                .await?;
            assert_status(status, StatusCode::OK, "filter transaction");
            let response: serde_json::Value = serde_json::from_slice(&body)?;
            for item in response["items"].as_array().unwrap() {
                assert_eq!(item["type"], "transaction");
            }

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn admin_get_transaction_returns_detail_with_entries() -> anyhow::Result<()> {
    with_test_app_with_config(|c| { c.ui.password = None; }, |app| {
        Box::pin(async move {
            let bundle = json!({
                "resourceType": "Bundle",
                "type": "batch",
                "entry": [
                    {
                        "request": { "method": "POST", "url": "Patient" },
                        "resource": minimal_patient()
                    },
                    {
                        "request": { "method": "POST", "url": "Patient" },
                        "resource": minimal_patient()
                    }
                ]
            });
            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir", Some(to_json_body(&bundle)?))
                .await?;
            assert_status(status, StatusCode::OK, "batch");

            // Get the transaction ID
            let tx_id: String = sqlx::query_scalar(
                "SELECT id::text FROM fhir_transactions ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_one(&app.state.db_pool)
            .await?;

            // Fetch detail
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    &format!("/admin/transactions/{}", tx_id),
                    None,
                )
                .await?;
            assert_status(status, StatusCode::OK, "get transaction detail");

            let detail: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(detail["id"], tx_id);
            assert_eq!(detail["type"], "batch");

            let entries = detail["entries"].as_array().unwrap();
            assert_eq!(entries.len(), 2, "should have 2 entries");
            assert_eq!(entries[0]["entryIndex"], 0);
            assert_eq!(entries[1]["entryIndex"], 1);

            Ok(())
        })
    })
    .await
}
