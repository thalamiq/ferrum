//! CREATE operation tests (POST /{resourceType})
//!
//! FHIR Spec Reference: https://hl7.org/fhir/R4/http.html#create
//!
//! Tests cover:
//! - Server-assigned IDs (UUID generation)
//! - meta.versionId = 1 for new resources
//! - meta.lastUpdated population
//! - Ignoring client-provided id, versionId, lastUpdated
//! - Conditional create (If-None-Exist header)
//! - Resource type validation
//! - Error handling

use crate::support::{
    assert_resource_id, assert_status, assert_version_id, constants, minimal_patient,
    patient_with_mrn, register_search_parameter, to_json_body, with_test_app,
};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn create_assigns_server_id() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");
            assert!(
                headers.get("location").is_some(),
                "Location header must be present"
            );

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            // Server assigns a UUID
            let id = created["id"]
                .as_str()
                .expect("created resource must have id");
            assert!(
                uuid::Uuid::parse_str(id).is_ok(),
                "id should be a valid UUID: {id}"
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_sets_version_id_to_one() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            assert_version_id(&created, "1")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_sets_last_updated() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            let last_updated = created["meta"]["lastUpdated"]
                .as_str()
                .expect("meta.lastUpdated must be present");

            // Verify it's a valid ISO 8601 timestamp
            chrono::DateTime::parse_from_rfc3339(last_updated)
                .expect("lastUpdated should be valid RFC3339 timestamp");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_ignores_client_provided_id() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "id": "client-provided-id",
                "active": true
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            // Server assigns its own ID, ignoring client-provided one
            let id = created["id"].as_str().expect("must have id");
            assert_ne!(id, "client-provided-id");
            assert!(uuid::Uuid::parse_str(id).is_ok(), "should be server UUID");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_ignores_client_provided_meta() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "active": true,
                "meta": {
                    "versionId": "999",
                    "lastUpdated": "2020-01-01T00:00:00Z"
                }
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            // Server overwrites client-provided meta
            assert_version_id(&created, "1")?;

            let last_updated = created["meta"]["lastUpdated"].as_str().unwrap();
            assert_ne!(last_updated, "2020-01-01T00:00:00Z");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_validates_resource_type() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Trying to create an Observation at /fhir/Patient endpoint
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": { "text": "test" }
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
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
async fn create_rejects_missing_resource_type() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let invalid = json!({
                "active": true
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&invalid)?))
                .await?;

            assert_status(status, StatusCode::BAD_REQUEST, "missing resourceType");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_returns_location_header() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let location = headers
                .get("location")
                .expect("Location header must be present")
                .to_str()?;

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();
            let version_id = created["meta"]["versionId"].as_str().unwrap();

            // Location should be in format: {base}/Patient/{id}/_history/{vid}
            assert!(
                location.contains(&format!("Patient/{id}")),
                "Location should contain resource path: {location}"
            );
            assert!(
                location.contains(&format!("_history/{version_id}")),
                "Location should contain version: {location}"
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_with_nested_objects() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "name": [{
                    "use": "official",
                    "family": "Smith",
                    "given": ["John", "Adam"]
                }],
                "telecom": [{
                    "system": "phone",
                    "value": "555-1234"
                }],
                "address": [{
                    "line": ["123 Main St"],
                    "city": "Boston",
                    "state": "MA"
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            // Verify nested structure is preserved
            assert_eq!(created["name"][0]["family"], "Smith");
            assert_eq!(created["name"][0]["given"][0], "John");
            assert_eq!(created["telecom"][0]["value"], "555-1234");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_create_no_matches_creates_resource() -> anyhow::Result<()> {
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
                .request_with_extra_headers(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                    &[(
                        "if-none-exist",
                        "identifier=http://example.org/fhir/mrn|123",
                    )],
                )
                .await?;

            assert_status(status, StatusCode::CREATED, "conditional create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(created["resourceType"], "Patient");
            assert_version_id(&created, "1")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_create_one_match_returns_existing() -> anyhow::Result<()> {
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

            let request_patient = patient_with_mrn("Other", "123");
            let (status, headers, body) = app
                .request_with_extra_headers(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&request_patient)?),
                    &[(
                        "if-none-exist",
                        "identifier=http://example.org/fhir/mrn|123",
                    )],
                )
                .await?;

            assert_status(status, StatusCode::OK, "conditional create");
            assert!(headers.get("location").is_some());

            let matched: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&matched, &id)?;
            assert_version_id(&matched, "1")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_create_multiple_matches_returns_412() -> anyhow::Result<()> {
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

            let request_patient = patient_with_mrn("Other", "999");
            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&request_patient)?),
                    &[(
                        "if-none-exist",
                        "identifier=http://example.org/fhir/mrn|DUP",
                    )],
                )
                .await?;

            assert_status(
                status,
                StatusCode::PRECONDITION_FAILED,
                "conditional create",
            );

            Ok(())
        })
    })
    .await
}
