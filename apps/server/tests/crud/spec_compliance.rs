//! HTTP Spec Compliance Tests
//!
//! FHIR Spec References:
//! - CREATE: spec/http/03-02-00-10-create.md
//! - READ: spec/http/03-02-00-02-read.md
//! - UPDATE: spec/http/03-02-00-04-01-update-as-create.md
//! - DELETE: spec/http/03-02-00-08-delete-history.md
//! - Contention: spec/http/03-02-00-05-managing-resource-contention.md
//!
//! Tests the exact requirements specified in the FHIR HTTP spec,
//! including configurable behaviors and edge cases.

use crate::support::{
    assert_status, assert_version_id, minimal_patient, to_json_body, with_test_app,
};
use axum::http::{Method, StatusCode};
use serde_json::json;

// ============================================================================
// CREATE Spec Compliance
// ============================================================================

#[tokio::test]
async fn create_ignores_client_id_per_spec() -> anyhow::Result<()> {
    // Spec: "If an `id` is provided, the server SHALL ignore it."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "id": "client-wants-this-id",
                "name": [{"family": "Test"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let server_id = created["id"].as_str().unwrap();

            // Server SHALL ignore client-provided ID
            assert_ne!(server_id, "client-wants-this-id");
            assert!(uuid::Uuid::parse_str(server_id).is_ok());

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_ignores_client_meta_per_spec() -> anyhow::Result<()> {
    // Spec: "If the request body includes a meta, the server SHALL ignore
    // the existing versionId and lastUpdated values."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "Test"}],
                "meta": {
                    "versionId": "999",
                    "lastUpdated": "1970-01-01T00:00:00Z",
                    "profile": ["http://example.org/StructureDefinition/my-patient"]
                }
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            // Server SHALL ignore versionId and lastUpdated
            assert_eq!(created["meta"]["versionId"], "1");
            assert_ne!(created["meta"]["lastUpdated"], "1970-01-01T00:00:00Z");

            // Server SHOULD preserve other meta values (like profile)
            // This is optional per spec ("SHOULD refrain from altering")
            // but we test for preservation if server supports it
            if let Some(profile) = created["meta"]["profile"].as_array() {
                assert!(
                    profile.contains(&json!("http://example.org/StructureDefinition/my-patient"))
                );
            }

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_returns_201_with_location_header_per_spec() -> anyhow::Result<()> {
    // Spec: "If the `create` request is successful, the server returns a
    // `201 Created` HTTP status code, and SHALL also return a `Location`
    // header which contains the new Logical Id and Version Id"
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            // SHALL return Location header
            let location = headers
                .get("location")
                .expect("Location header is required per spec")
                .to_str()?;

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();
            let vid = created["meta"]["versionId"].as_str().unwrap();

            // Location format: [base]/[type]/[id]/_history/[vid]
            assert!(location.contains(&format!("Patient/{id}")));
            assert!(location.contains(&format!("_history/{vid}")));

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_returns_etag_and_last_modified_per_spec() -> anyhow::Result<()> {
    // Spec: "Servers SHOULD return an `ETag` header with the `versionId`
    // (if versioning is supported) and a `Last-Modified` header."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, headers, _body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            // SHOULD return ETag
            if let Some(etag) = headers.get("etag") {
                let etag_str = etag.to_str()?;
                // ETag should reference version 1
                assert!(
                    etag_str.contains('1'),
                    "ETag should contain version: {etag_str}"
                );
            }

            // SHOULD return Last-Modified
            if let Some(last_modified) = headers.get("last-modified") {
                let _last_mod_str = last_modified.to_str()?;
                // Just verify it's present and parseable
            }

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_rejects_invalid_with_400_per_spec() -> anyhow::Result<()> {
    // Spec: "When the resource syntax or data is incorrect or invalid,
    // and cannot be used to create a new resource, the server returns
    // a `400 Bad Request` HTTP status code."
    with_test_app(|app| {
        Box::pin(async move {
            // Missing required resourceType
            let invalid = json!({
                "name": [{"family": "Test"}]
            });

            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&invalid)?))
                .await?;

            assert_status(status, StatusCode::BAD_REQUEST, "invalid resource");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// READ Spec Compliance
// ============================================================================

#[tokio::test]
async fn read_returns_resource_with_id_per_spec() -> anyhow::Result<()> {
    // Spec: "The returned resource SHALL have an `id` element with a value
    // that is the `[id]`."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Read the resource
            let (status, _headers, body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::OK, "read");

            let read: serde_json::Value = serde_json::from_slice(&body)?;

            // SHALL have id element matching URL id
            assert_eq!(read["id"].as_str().unwrap(), id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_returns_etag_and_last_modified_per_spec() -> anyhow::Result<()> {
    // Spec: "Servers SHOULD return an `ETag` header with the versionId of
    // the resource (if versioning is supported) and a `Last-Modified` header."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Read the resource
            let (status, headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::OK, "read");

            // SHOULD return ETag
            if let Some(etag) = headers.get("etag") {
                let etag_str = etag.to_str()?;
                assert!(!etag_str.is_empty());
            }

            // SHOULD return Last-Modified
            assert!(headers.get("last-modified").is_some());

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_deleted_returns_410_gone_per_spec() -> anyhow::Result<()> {
    // Spec: "a `GET` for a deleted resource returns a `410 Gone` status code"
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete the resource
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "delete");

            // Read deleted resource
            let (status, _headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            // SHALL return 410 Gone
            assert_status(status, StatusCode::GONE, "read deleted");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn read_unknown_returns_404_not_found_per_spec() -> anyhow::Result<()> {
    // Spec: "a `GET` for an unknown resource returns `404 Not Found`"
    with_test_app(|app| {
        Box::pin(async move {
            let (status, _headers, _body) = app
                .request(Method::GET, "/fhir/Patient/unknown-id-12345", None)
                .await?;

            // SHALL return 404 Not Found
            assert_status(status, StatusCode::NOT_FOUND, "read unknown");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// UPDATE Spec Compliance
// ============================================================================

#[tokio::test]
async fn update_as_create_with_allow_update_create_true() -> anyhow::Result<()> {
    // Spec: "Servers MAY choose to allow clients to `PUT` a resource to a
    // location that does not yet exist on the server"
    // Config: allow_update_create = true (default)
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "id": "client-defined-id",
                "name": [{"family": "UpdateCreate"}]
            });

            let (status, headers, body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/client-defined-id",
                    Some(to_json_body(&patient)?),
                )
                .await?;

            // Spec: "A server SHALL NOT return a `201` response if it did
            // not create a new resource."
            assert_status(status, StatusCode::CREATED, "update-as-create");

            // Spec: "If a new resource is created, a location header SHALL
            // be returned (though it SHALL be the same as the location in
            // the URL of the PUT request)."
            let location = headers
                .get("location")
                .expect("Location required for update-as-create")
                .to_str()?;
            assert!(location.contains("Patient/client-defined-id"));

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(created["id"], "client-defined-id");
            assert_version_id(&created, "1")?;

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_existing_returns_200_not_201_per_spec() -> anyhow::Result<()> {
    // Spec: "A server SHALL NOT return a `201` response if it did not
    // create a new resource."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Update existing resource
            let mut updated = created.clone();
            updated["active"] = json!(false);

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated)?),
                )
                .await?;

            // SHALL return 200 OK (not 201 Created) for updates
            assert_status(status, StatusCode::OK, "update existing");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_validates_id_matches_url_per_spec() -> anyhow::Result<()> {
    // Spec (from crud.rs): "If no id element is provided, or the id disagrees
    // with the id in the URL, the server SHALL respond with an HTTP 400 error"
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Try to update with mismatched ID in body
            let mismatched = json!({
                "resourceType": "Patient",
                "id": "wrong-id",
                "name": [{"family": "Test"}]
            });

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&mismatched)?),
                )
                .await?;

            // SHALL return 400 Bad Request
            assert_status(status, StatusCode::BAD_REQUEST, "mismatched id");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// DELETE Spec Compliance
// ============================================================================

#[tokio::test]
async fn delete_returns_204_no_content_per_spec() -> anyhow::Result<()> {
    // Spec: "Upon successful deletion, or if the resource does not exist at all,
    // the server should return either a `200 OK` if the response contains a
    // payload, or a `204 No Content` with no response payload."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete the resource
            let (status, _headers, body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;

            // Server returns 204 No Content
            assert_status(status, StatusCode::NO_CONTENT, "delete");
            assert!(body.is_empty(), "204 should have no payload");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn delete_nonexistent_succeeds_per_spec() -> anyhow::Result<()> {
    // Spec: "Upon successful deletion, or if the resource does not exist at all,
    // the server should return ... a `204 No Content`"
    // (DELETE is idempotent)
    with_test_app(|app| {
        Box::pin(async move {
            let (status, _headers, _body) = app
                .request(Method::DELETE, "/fhir/Patient/nonexistent-123", None)
                .await?;

            // Should succeed even if resource doesn't exist
            assert_status(status, StatusCode::NO_CONTENT, "delete nonexistent");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn delete_is_idempotent_per_spec() -> anyhow::Result<()> {
    // Spec: DELETE should be idempotent - deleting twice succeeds
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Delete once
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "first delete");

            // Delete again - should still succeed (idempotent)
            let (status, _headers, _body) = app
                .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::NO_CONTENT, "second delete");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// ETag and Version Management (Spec: Managing Resource Contention)
// ============================================================================

#[tokio::test]
async fn etag_matches_version_id_per_spec() -> anyhow::Result<()> {
    // Spec: "If provided, the value of the `ETag` SHALL match the value
    // of the version id for the resource."
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();
            let version_id = created["meta"]["versionId"].as_str().unwrap();

            // Read to get ETag
            let (status, headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;
            assert_status(status, StatusCode::OK, "read");

            if let Some(etag) = headers.get("etag") {
                let etag_str = etag.to_str()?;
                // ETag SHALL match versionId
                // Format may be W/"1" or just "1", both valid
                assert!(
                    etag_str.contains(version_id),
                    "ETag {etag_str} should contain versionId {version_id}"
                );
            }

            Ok(())
        })
    })
    .await
}

// ============================================================================
// Resource Type Validation
// ============================================================================

#[tokio::test]
async fn rejects_resource_type_mismatch_per_spec() -> anyhow::Result<()> {
    // Test that server validates resourceType matches endpoint
    with_test_app(|app| {
        Box::pin(async move {
            // Try to POST an Observation to /Patient endpoint
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "test"}
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&observation)?),
                )
                .await?;

            assert_status(status, StatusCode::BAD_REQUEST, "type mismatch");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// Conditional Operations
// ============================================================================
//
// Covered in focused suites:
// - Conditional create (If-None-Exist): `server/tests/crud/create.rs`
// - Batch/transaction conditional behavior: `server/tests/batch_transaction_conditional.rs`
