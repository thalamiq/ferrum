//! Tests for Configurable Server Behaviors
//!
//! FHIR servers can configure certain behaviors based on deployment needs.
//! These tests verify that configurable options work correctly.
//!
//! Configurable options (see src/config.rs):
//! - `allow_update_create`: Allow client-defined IDs via PUT (default: true)
//! - `hard_delete`: Physically remove resources vs soft delete (default: false)
//! - `default_prefer_return`: Default Prefer header behavior (default: "representation")
//!
//! Note: These tests use `with_test_app_with_config` to override config per test.

use crate::support::{
    assert_status, minimal_patient, to_json_body, with_test_app, with_test_app_with_config,
    ObservationBuilder,
};
use axum::http::{Method, StatusCode};
use serde_json::json;
use ferrum::db::{PostgresResourceStore, ResourceStore};

// ============================================================================
// Interaction Enable/Disable Tests
// ============================================================================

#[tokio::test]
async fn create_is_blocked_when_interaction_disabled() -> anyhow::Result<()> {
    with_test_app_with_config(
        |config| {
            config.fhir.interactions.type_level.create = false;
        },
        |app| {
            Box::pin(async move {
                let patient = minimal_patient();
                let (status, _headers, _body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::METHOD_NOT_ALLOWED, "create disabled");
                Ok(())
            })
        },
    )
    .await
}

#[tokio::test]
async fn supported_resources_are_enforced_for_type_level_writes() -> anyhow::Result<()> {
    with_test_app_with_config(
        |config| {
            config.fhir.capability_statement.supported_resources = vec!["Patient".to_string()];
        },
        |app| {
            Box::pin(async move {
                // Allowed type.
                let patient = minimal_patient();
                let (status, _headers, _body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "Patient allowed");

                // Disallowed type.
                let observation = ObservationBuilder::new()
                    .code_text("Weight")
                    .subject("Patient/example")
                    .build();
                let (status, _headers, _body) = app
                    .request(
                        Method::POST,
                        "/fhir/Observation",
                        Some(to_json_body(&observation)?),
                    )
                    .await?;
                assert_status(
                    status,
                    StatusCode::METHOD_NOT_ALLOWED,
                    "Observation not in supported_resources",
                );

                Ok(())
            })
        },
    )
    .await
}

// ============================================================================
// allow_update_create Configuration Tests
// ============================================================================

#[tokio::test]
async fn update_create_allowed_by_default() -> anyhow::Result<()> {
    // Default config: allow_update_create = true
    // Server SHOULD allow clients to define their own IDs via PUT
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "id": "my-custom-id",
                "name": [{"family": "Custom"}]
            });

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/my-custom-id",
                    Some(to_json_body(&patient)?),
                )
                .await?;

            // With allow_update_create=true, this should succeed
            assert_status(status, StatusCode::CREATED, "update-as-create allowed");

            Ok(())
        })
    })
    .await
}

// NOTE: This test would require config override capability
#[tokio::test]
async fn update_create_forbidden_when_disabled() -> anyhow::Result<()> {
    // With allow_update_create = false:
    // PUT to a non-existent resource should return 405 Method Not Allowed.
    with_test_app_with_config(
        |config| {
            config.fhir.allow_update_create = false;
        },
        |app| {
            Box::pin(async move {
                let patient = json!({
                    "resourceType": "Patient",
                    "id": "my-custom-id",
                    "name": [{"family": "Custom"}]
                });

                let (status, _headers, _body) = app
                    .request(
                        Method::PUT,
                        "/fhir/Patient/my-custom-id",
                        Some(to_json_body(&patient)?),
                    )
                    .await?;

                assert_status(
                    status,
                    StatusCode::METHOD_NOT_ALLOWED,
                    "update-as-create disabled",
                );

                Ok(())
            })
        },
    )
    .await
}

#[tokio::test]
async fn update_existing_works_regardless_of_update_create_setting() -> anyhow::Result<()> {
    // Updating an existing resource should always work, regardless of
    // allow_update_create setting
    with_test_app(|app| {
        Box::pin(async move {
            // Create a resource first
            let patient = minimal_patient();
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap();

            // Update should work
            let mut updated = created.clone();
            updated["active"] = json!(false);

            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&updated)?),
                )
                .await?;

            assert_status(status, StatusCode::OK, "update existing");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// hard_delete Configuration Tests
// ============================================================================

#[tokio::test]
async fn soft_delete_default_behavior() -> anyhow::Result<()> {
    // Default config: hard_delete = false
    // DELETE should create a deleted version (soft delete)
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

            // With soft delete, reading returns 410 Gone (resource exists but is deleted)
            let (status, _headers, _body) = app
                .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                .await?;

            assert_status(status, StatusCode::GONE, "read soft-deleted");

            Ok(())
        })
    })
    .await
}

// NOTE: This test would require config override capability
#[tokio::test]
async fn hard_delete_removes_resource_completely() -> anyhow::Result<()> {
    // With hard_delete = true:
    // - DELETE physically removes the resource and history
    // - Subsequent reads return 404 Not Found (not 410 Gone)
    with_test_app_with_config(
        |config| {
            config.fhir.hard_delete = true;
        },
        |app| {
            Box::pin(async move {
                let patient = minimal_patient();
                let (status, _headers, body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");

                let created: serde_json::Value = serde_json::from_slice(&body)?;
                let id = created["id"].as_str().unwrap();

                let (status, _headers, _body) = app
                    .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                    .await?;
                assert_status(status, StatusCode::NO_CONTENT, "delete");

                let (status, _headers, _body) = app
                    .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                    .await?;
                assert_status(status, StatusCode::NOT_FOUND, "read hard-deleted");

                Ok(())
            })
        },
    )
    .await
}

#[tokio::test]
async fn hard_delete_can_expunge_previously_soft_deleted_resources() -> anyhow::Result<()> {
    // When switching from soft delete to hard delete, a second DELETE should expunge the
    // previously soft-deleted resource (remove rows, not just keep the deleted current version).
    with_test_app_with_config(
        |config| {
            config.fhir.hard_delete = true;
        },
        |app| {
            Box::pin(async move {
                let patient = minimal_patient();
                let (status, _headers, body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");

                let created: serde_json::Value = serde_json::from_slice(&body)?;
                let id = created["id"].as_str().unwrap();

                // Force a soft-delete version into the DB to simulate existing soft-deleted data.
                let store = PostgresResourceStore::new(app.state.db_pool.clone());
                let deleted_version = store.delete("Patient", id).await?;
                assert_eq!(deleted_version, 2, "soft delete should create version 2");

                let (status, _headers, _body) = app
                    .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                    .await?;
                assert_status(status, StatusCode::GONE, "read soft-deleted");

                // In hard-delete mode, DELETE should now physically remove the resource.
                let (status, _headers, _body) = app
                    .request(Method::DELETE, &format!("/fhir/Patient/{id}"), None)
                    .await?;
                assert_status(status, StatusCode::NO_CONTENT, "delete expunges");

                let (status, _headers, _body) = app
                    .request(Method::GET, &format!("/fhir/Patient/{id}"), None)
                    .await?;
                assert_status(status, StatusCode::NOT_FOUND, "read expunged");

                Ok(())
            })
        },
    )
    .await
}

// ============================================================================
// Prefer Header and Return Content
// ============================================================================

#[tokio::test]
async fn create_returns_representation_by_default() -> anyhow::Result<()> {
    // Default: return full representation of created resource
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;

            // Should return full resource
            assert_eq!(created["resourceType"], "Patient");
            assert!(created["id"].is_string());
            assert!(created["meta"].is_object());

            Ok(())
        })
    })
    .await
}

// TODO: Test Prefer: return=minimal (returns empty body with Location header)
// TODO: Test Prefer: return=representation (returns full resource)
// TODO: Test Prefer: return=OperationOutcome (returns OperationOutcome)

// #[tokio::test]
// async fn prefer_return_minimal_returns_empty_body() -> anyhow::Result<()> {
//     // With Prefer: return=minimal header
//     // Server should return 201 with Location but empty body
//     with_test_app(|app| {
//         Box::pin(async move {
//             let patient = minimal_patient();
//
//             let mut headers = HeaderMap::new();
//             headers.insert("prefer", "return=minimal".parse()?);
//
//             let (status, headers, body) = app
//                 .request_with_headers(
//                     Method::POST,
//                     "/fhir/Patient",
//                     Some(to_json_body(&patient)?),
//                     headers
//                 )
//                 .await?;
//
//             assert_status(status, StatusCode::CREATED, "create");
//             assert!(headers.get("location").is_some());
//             assert!(body.is_empty(), "minimal should return empty body");
//
//             Ok(())
//         })
//     })
//     .await
// }

#[tokio::test]
async fn prefer_return_minimal_returns_empty_body() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, headers, body) = app
                .request_with_extra_headers(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                    &[("prefer", "return=minimal")],
                )
                .await?;

            assert_status(status, StatusCode::CREATED, "create");
            assert!(headers.get("location").is_some());
            assert!(body.is_empty(), "minimal should return empty body");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn prefer_return_operationoutcome_returns_outcome() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = minimal_patient();

            let (status, _headers, body) = app
                .request_with_extra_headers(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient)?),
                    &[("prefer", "return=operationoutcome")],
                )
                .await?;

            assert_status(status, StatusCode::CREATED, "create");

            let outcome: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(outcome["resourceType"], "OperationOutcome");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn default_prefer_return_can_be_configured() -> anyhow::Result<()> {
    with_test_app_with_config(
        |config| {
            config.fhir.default_prefer_return = "minimal".to_string();
        },
        |app| {
            Box::pin(async move {
                let patient = minimal_patient();

                // No Prefer header: should use configured default (minimal).
                let (status, headers, body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;

                assert_status(status, StatusCode::CREATED, "create");
                assert!(headers.get("location").is_some());
                assert!(body.is_empty(), "default minimal should return empty body");

                Ok(())
            })
        },
    )
    .await
}

// ============================================================================
// Capability Statement Reflection
// ============================================================================

// These tests verify that the capability statement correctly reflects
// the server's configuration

// #[tokio::test]
// async fn capability_statement_reflects_update_create_setting() -> anyhow::Result<()> {
//     // GET /metadata should return CapabilityStatement
//     // CapabilityStatement.rest.resource.updateCreate should match config
//     with_test_app(|app| {
//         Box::pin(async move {
//             let (status, _headers, body) = app
//                 .request(Method::GET, "/metadata", None)
//                 .await?;
//
//             assert_status(status, StatusCode::OK, "metadata");
//
//             let capability: serde_json::Value = serde_json::from_slice(&body)?;
//
//             // Find Patient resource capability
//             let rest = &capability["rest"][0];
//             let patient_resource = rest["resource"]
//                 .as_array()
//                 .unwrap()
//                 .iter()
//                 .find(|r| r["type"] == "Patient")
//                 .expect("Patient resource should be in capability statement");
//
//             // Should reflect allow_update_create config
//             let update_create = patient_resource["updateCreate"].as_bool();
//             // assert_eq!(update_create, Some(true)); // or false based on config
//
//             Ok(())
//         })
//     })
//     .await
// }

// ============================================================================
// Notes on Testing Different Configurations
// ============================================================================

// To properly test different configurations, we would need to enhance TestApp
// to allow config overrides. Here's how that could work:
//
// ```rust
// pub struct TestApp {
//     // ... existing fields
//     config_overrides: Option<ConfigOverrides>,
// }
//
// impl TestApp {
//     pub async fn with_config(overrides: ConfigOverrides) -> anyhow::Result<Self> {
//         let mut config = Config::load()?;
//         // Apply overrides
//         if let Some(allow_update_create) = overrides.allow_update_create {
//             config.fhir.allow_update_create = allow_update_create;
//         }
//         if let Some(hard_delete) = overrides.hard_delete {
//             config.fhir.hard_delete = hard_delete;
//         }
//         // ... create state with modified config
//     }
// }
// ```
//
// Then tests could be written like:
// ```rust
// let app = TestApp::with_config(ConfigOverrides {
//     allow_update_create: Some(false),
//     ..Default::default()
// }).await?;
// ```
