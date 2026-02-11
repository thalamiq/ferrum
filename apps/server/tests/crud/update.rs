//! UPDATE operation tests (PUT /{resourceType}/{id})
//!
//! FHIR Spec Reference: https://hl7.org/fhir/R4/http.html#update
//!
//! Tests cover:
//! - Updating existing resources
//! - Version ID increments on update
//! - meta.lastUpdated changes on update
//! - Update-as-create (client-defined IDs)
//! - Conditional update (If-Match)
//! - ID validation (body must match URL)
//! - Resource type validation

use crate::support::{
    assert_resource_id, assert_status, assert_version_id, constants, minimal_patient,
    patient_with_mrn, register_search_parameter, to_json_body, with_test_app,
};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn update_existing_resource() -> anyhow::Result<()> {
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

            // Update it
            let mut updated_patient = created.clone();
            updated_patient["active"] = json!(false);

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "update");

            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&updated, id)?;
            assert_eq!(updated["active"], false);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_increments_version_id() -> anyhow::Result<()> {
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
            assert_version_id(&created, "1")?;

            // Update it
            let mut updated_patient = created.clone();
            updated_patient["active"] = json!(false);

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "update");

            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            assert_version_id(&updated, "2")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_changes_last_updated() -> anyhow::Result<()> {
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
            let original_last_updated = created["meta"]["lastUpdated"].as_str().unwrap();

            // Small delay to ensure timestamp difference
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

            // Update it
            let mut updated_patient = created.clone();
            updated_patient["active"] = json!(false);

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "update");

            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            let new_last_updated = updated["meta"]["lastUpdated"].as_str().unwrap();

            assert_ne!(
                original_last_updated, new_last_updated,
                "lastUpdated should change on update"
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_as_create_with_client_id_when_allowed() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Try to update a non-existent resource (update-as-create)
            let patient = json!({
                "resourceType": "Patient",
                "id": "my-client-id",
                "active": true
            });

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/my-client-id",
                    Some(to_json_body(&patient)?),
                )
                .await?;

            // Server allows update-as-create by default
            assert_status(status, StatusCode::CREATED, "update-as-create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&created, "my-client-id")?;
            assert_version_id(&created, "1")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_without_id_in_body_is_allowed() -> anyhow::Result<()> {
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

            // Update without ID in body (server should populate it)
            let updated_patient = json!({
                "resourceType": "Patient",
                "active": false
            });

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "update");

            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&updated, id)?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_update_creates_when_no_match_and_no_id() -> anyhow::Result<()> {
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

            let patient = minimal_patient();
            let (status, headers, body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&patient)?),
                )
                .await?;

            assert_status(status, StatusCode::CREATED, "conditional update create");
            assert!(headers.get("location").is_some());
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(created["resourceType"], "Patient");
            assert!(created["id"].as_str().is_some());
            assert_version_id(&created, "1")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_update_updates_when_one_match() -> anyhow::Result<()> {
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

            let update_body = json!({
                "resourceType": "Patient",
                "active": false
            });
            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&update_body)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "conditional update");
            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&updated, &id)?;
            assert_version_id(&updated, "2")?;
            assert_eq!(updated["active"], false);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_update_returns_412_on_multiple_matches() -> anyhow::Result<()> {
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

            let update_body = json!({
                "resourceType": "Patient",
                "active": false
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|DUP",
                    Some(to_json_body(&update_body)?),
                )
                .await?;

            assert_status(
                status,
                StatusCode::PRECONDITION_FAILED,
                "conditional update",
            );
            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_update_returns_400_on_body_id_mismatch() -> anyhow::Result<()> {
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

            let update_body = json!({
                "resourceType": "Patient",
                "id": "different",
                "active": false
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&update_body)?),
                )
                .await?;

            assert_status(status, StatusCode::BAD_REQUEST, "conditional update");
            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_update_returns_409_when_no_match_but_id_exists() -> anyhow::Result<()> {
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

            let patient = json!({
                "resourceType": "Patient",
                "id": "exists",
                "active": true
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/exists",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            // Criteria has no matches, but body supplies an id that already exists -> 409.
            let update_body = json!({
                "resourceType": "Patient",
                "id": "exists",
                "active": false
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|nope",
                    Some(to_json_body(&update_body)?),
                )
                .await?;

            assert_status(status, StatusCode::CONFLICT, "conditional update");
            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_update_if_none_match_is_enforced() -> anyhow::Result<()> {
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

            let update_body = json!({
                "resourceType": "Patient",
                "active": false
            });
            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&update_body)?),
                    &[("if-none-match", "*")],
                )
                .await?;
            assert_status(status, StatusCode::PRECONDITION_FAILED, "if-none-match *");

            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&update_body)?),
                    &[("if-none-match", "W/\"1\"")],
                )
                .await?;
            assert_status(status, StatusCode::PRECONDITION_FAILED, "if-none-match v1");

            let (status, _headers, body) = app
                .request_with_extra_headers(
                    Method::PUT,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&update_body)?),
                    &[("if-none-match", "W/\"2\"")],
                )
                .await?;
            assert_status(status, StatusCode::OK, "if-none-match v2");
            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&updated, &id)?;
            assert_version_id(&updated, "2")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_rejects_mismatched_id() -> anyhow::Result<()> {
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

            // Try to update with different ID in body
            let mut updated_patient = created.clone();
            updated_patient["id"] = json!("different-id");

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;

            assert_status(status, StatusCode::BAD_REQUEST, "mismatched ID");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_validates_resource_type() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Try to update Patient with Observation resource
            let observation = json!({
                "resourceType": "Observation",
                "id": "test-id",
                "status": "final",
                "code": { "text": "test" }
            });

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/test-id",
                    Some(to_json_body(&observation)?),
                )
                .await?;

            assert_status(status, StatusCode::BAD_REQUEST, "mismatched resource type");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_ignores_client_provided_version_id() -> anyhow::Result<()> {
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

            // Update with wrong versionId in body
            let updated_patient = json!({
                "resourceType": "Patient",
                "id": id,
                "active": false,
                "meta": {
                    "versionId": "999"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "update");

            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            // Server ignores client versionId and sets correct value
            assert_version_id(&updated, "2")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn multiple_updates_increment_version() -> anyhow::Result<()> {
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

            // Perform multiple updates
            for expected_version in 2..=5 {
                current["active"] = json!(expected_version % 2 == 0);

                let (status, _headers, body) = app
                    .request(
                        Method::PUT,
                        &format!("/fhir/Patient/{id}"),
                        Some(to_json_body(&current)?),
                    )
                    .await?;

                assert_status(
                    status,
                    StatusCode::OK,
                    &format!("update {expected_version}"),
                );

                current = serde_json::from_slice(&body)?;
                assert_version_id(&current, &expected_version.to_string())?;
            }

            Ok(())
        })
    })
    .await
}

// TODO: Conditional update tests require header support
// #[tokio::test]
// async fn conditional_update_with_if_match_succeeds_on_version_match() -> anyhow::Result<()> {
//     // Test If-Match header with matching version
// }

// #[tokio::test]
// async fn conditional_update_with_if_match_fails_on_version_mismatch() -> anyhow::Result<()> {
//     // Test If-Match header with mismatched version (409 Conflict)
// }
