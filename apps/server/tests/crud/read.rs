//! READ operation tests (GET /{resourceType}/{id})
//!
//! FHIR Spec Reference: https://hl7.org/fhir/R4/http.html#read
//!
//! Tests cover:
//! - Reading existing resources
//! - 404 Not Found for non-existent resources
//! - 410 Gone for deleted resources
//! - Reading returns current version only
//! - ETag header with version ID

use crate::support::{
    assert_resource_id, assert_status, minimal_patient, to_json_body, with_test_app,
};
use axum::http::{Method, StatusCode};

#[tokio::test]
async fn read_existing_resource() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Create a patient first
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Now read it
            let (status, _headers, body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::OK, "read");

            let read: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&read, id)?;
            assert_eq!(read["meta"]["versionId"], "1");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_returns_404_for_non_existent_resource() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let (status, _headers, _body) = app
                .request(Method::GET, "/fhir/Patient/non-existent-id", None)
                .await?;

            assert_status(status, StatusCode::NOT_FOUND, "read non-existent");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_returns_410_for_deleted_resource() -> anyhow::Result<()> {
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

            // Try to read deleted resource
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
async fn read_returns_current_version_only() -> anyhow::Result<()> {
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
            updated_patient["active"] = serde_json::json!(false);

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated_patient)?),
                )
                .await?;
            assert_status(status, StatusCode::OK, "update");

            // Read should return version 2 (current)
            let (status, _headers, body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::OK, "read");

            let read: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(read["meta"]["versionId"], "2");
            assert_eq!(read["active"], false);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_returns_etag_header() -> anyhow::Result<()> {
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

            // Read it
            let (status, headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::OK, "read");

            // ETag should contain version ID
            if let Some(etag) = headers.get("etag") {
                let etag_str = etag.to_str()?;
                // ETag format varies, but should reference version 1
                assert!(
                    etag_str.contains('1'),
                    "ETag should reference version 1: {etag_str}"
                );
            }

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_preserves_all_resource_data() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            use serde_json::json;

            let patient = json!({
                "resourceType": "Patient",
                "active": true,
                "name": [{
                    "family": "Smith",
                    "given": ["John"]
                }],
                "gender": "male",
                "birthDate": "1970-01-01"
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Read and verify all data is preserved
            let (status, _headers, body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::OK, "read");

            let read: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(read["active"], true);
            assert_eq!(read["name"][0]["family"], "Smith");
            assert_eq!(read["name"][0]["given"][0], "John");
            assert_eq!(read["gender"], "male");
            assert_eq!(read["birthDate"], "1970-01-01");

            Ok(())
        })
    })
    .await
}
