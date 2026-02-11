//! DELETE operation tests (DELETE /{resourceType}/{id})
//!
//! FHIR Spec Reference: https://hl7.org/fhir/R4/http.html#delete
//!
//! Tests cover:
//! - Soft delete (default behavior)
//! - Deleted resources return 410 Gone on read
//! - Delete is idempotent
//! - Deleting non-existent resources succeeds
//! - Version ID on delete
//! - 204 No Content response

use crate::support::{
    assert_status, minimal_patient, patient_with_mrn, register_search_parameter, to_json_body,
    with_test_app,
};
use axum::http::{Method, StatusCode};

#[tokio::test]
async fn delete_existing_resource() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Create a patient
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete it
            let (status, _headers, body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::NO_CONTENT, "delete");
            assert!(body.is_empty(), "delete should return empty body");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn delete_returns_etag_with_version() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Create a patient
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete it
            let (status, headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::NO_CONTENT, "delete");

            // ETag MAY be returned (optional per spec)
            if let Some(etag) = headers.get("etag") {
                let etag_str = etag.to_str()?;
                // ETag should reference version 2 (deletion creates new version)
                assert!(
                    etag_str.contains('2'),
                    "ETag should reference version 2: {etag_str}"
                );
            }

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn soft_delete_creates_new_version() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Create a patient
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete it
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "delete");

            // Try to read - should return 410 Gone
            let (status, _headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::GONE, "read deleted resource");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn delete_is_idempotent() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Create a patient
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete it first time
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "first delete");

            // Delete it again - should still succeed (idempotent)
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "second delete");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn delete_non_existent_resource_succeeds() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Delete a resource that never existed
            let (status, _headers, _body) = app
                .request(Method::DELETE, "/fhir/Patient/never-existed", None)
                .await?;

            // Per FHIR spec: DELETE is idempotent, should succeed
            assert_status(status, StatusCode::NO_CONTENT, "delete non-existent");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn deleted_resource_can_be_recreated_with_update() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            use serde_json::json;

            // Create with client ID
            let patient = json!({
                "resourceType": "Patient",
                "id": "recreate-test",
                "active": true
            });

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/recreate-test",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "initial create");

            // Delete it
            let (status, _headers, _body) = app
                .request(Method::DELETE, "/fhir/Patient/recreate-test", None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "delete");

            // Recreate with same ID
            // Note: Server may treat this as UPDATE (200) if the deleted version still exists
            // or as CREATE (201) if it's truly a new resource. Both are valid.
            let patient = json!({
                "resourceType": "Patient",
                "id": "recreate-test",
                "active": false
            });

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/recreate-test",
                    Some(to_json_body(&patient)?),
                )
                .await?;

            // Accept both 200 OK (update) and 201 Created
            assert!(
                status == StatusCode::OK || status == StatusCode::CREATED,
                "recreate should succeed with 200 or 201, got {status}"
            );

            let recreated: serde_json::Value = serde_json::from_slice(&body)?;
            // Version continues from where it left off
            assert!(recreated["meta"]["versionId"].is_string());

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn delete_after_multiple_updates() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Create a patient
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let mut current: serde_json::Value = serde_json::from_slice(&body)?;
            let id = current["id"].as_str().unwrap().to_string();

            // Update multiple times
            for i in 1..=3 {
                current["active"] = serde_json::json!(i % 2 == 0);
                let (status, _headers, body) = app
                    .request(
                        Method::PUT,
                        &format!("/fhir/Patient/{id}"),
                        Some(to_json_body(&current)?),
                    )
                    .await?;
                assert_status(status, StatusCode::OK, &format!("update {i}"));
                current = serde_json::from_slice(&body)?;
            }

            // Now delete
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "delete");

            // Verify it's gone
            let (status, _headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::GONE, "read deleted");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_delete_returns_404_when_no_match() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            let (status, _headers, _body) = app
                .request(
                    Method::DELETE,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|missing",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::NOT_FOUND, "conditional delete");
            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_delete_deletes_when_one_match() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            let patient = patient_with_mrn("Doe", "123");
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap().to_string();

            let (status, _headers, body) = app
                .request(
                    Method::DELETE,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    None,
                )
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "conditional delete");
            assert!(body.is_empty());

            let (status, _headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::GONE, "read deleted");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_delete_returns_412_on_multiple_matches() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            let mut ids = Vec::new();
            for _ in 0..2 {
                let patient = patient_with_mrn("Doe", "DUP");
                let (status, _headers, body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let created: serde_json::Value = serde_json::from_slice(&body)?;
                let id = created["id"].as_str().unwrap().to_string();
                ids.push(id.clone());
            }

            let (status, _headers, _body) = app
                .request(
                    Method::DELETE,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|DUP",
                    None,
                )
                .await?;
            assert_status(
                status,
                StatusCode::PRECONDITION_FAILED,
                "conditional delete",
            );

            // Ensure both still exist.
            for id in ids {
                let (status, _headers, _body) = app
                    .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                    .await?;
                assert_status(status, StatusCode::OK, "read not deleted");
            }

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_delete_honors_if_match() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            let patient = patient_with_mrn("Doe", "123");
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let _id = created["id"].as_str().unwrap().to_string();

            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::DELETE,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    None,
                    &[("if-match", "W/\"2\"")],
                )
                .await?;
            assert_status(status, StatusCode::PRECONDITION_FAILED, "if-match mismatch");

            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::DELETE,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    None,
                    &[("if-match", "W/\"1\"")],
                )
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "if-match match");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn system_delete_supports_single_match() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            let patient = patient_with_mrn("Doe", "123");
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let _id = created["id"].as_str().unwrap().to_string();

            let (status, _headers, _body) = app
                .request(
                    Method::DELETE,
                    "/fhir?_type=Patient&identifier=http://example.org/fhir/mrn|123",
                    None,
                )
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "system delete");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn system_delete_returns_412_on_multiple_matches() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            for _ in 0..2 {
                let patient = patient_with_mrn("Doe", "DUP");
                let (status, _headers, body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let _created: serde_json::Value = serde_json::from_slice(&body)?;
            }

            let (status, _headers, _body) = app
                .request(
                    Method::DELETE,
                    "/fhir?_type=Patient&identifier=http://example.org/fhir/mrn|DUP",
                    None,
                )
                .await?;
            assert_status(status, StatusCode::PRECONDITION_FAILED, "system delete");

            Ok(())
        })
    })
    .await
}
