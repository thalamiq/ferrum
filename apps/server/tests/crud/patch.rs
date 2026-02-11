//! PATCH operation tests (PATCH /{resourceType}/{id} and conditional PATCH)
//!
//! FHIR Spec Reference: https://hl7.org/fhir/R4/http.html#patch
//!
//! Tests cover:
//! - JSON Patch application and version increment
//! - Conditional patch resolution (0/1/many matches)
//! - 422 Unprocessable Entity on failing JSON Patch test op
//! - Narrative safety behavior (narrative removed after patch)

use crate::support::{
    assert_resource_id, assert_status, assert_version_id, constants, patient_with_mrn,
    register_search_parameter, to_json_body, with_test_app,
};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn patch_removes_narrative_for_safety() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({
                "resourceType": "Patient",
                "active": true,
                "text": {
                    "status": "additional",
                    "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">unsafe</div>"
                }
            });
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let id = created["id"].as_str().unwrap().to_string();

            let patch = json!([
                { "op": "replace", "path": "/active", "value": false }
            ]);
            let (status, _headers, body) = app
                .request_with_extra_headers(
                    Method::PATCH,
                    &format!("/fhir/Patient/{id}"),
                    Some(to_json_body(&patch)?),
                    &[("content-type", "application/json-patch+json")],
                )
                .await?;
            assert_status(status, StatusCode::OK, "patch");
            let patched: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&patched, &id)?;
            assert!(patched.get("text").is_none(), "narrative should be removed");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_patch_returns_404_when_no_match() -> anyhow::Result<()> {
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

            let patch = json!([{ "op": "add", "path": "/active", "value": true }]);
            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::PATCH,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|missing",
                    Some(to_json_body(&patch)?),
                    &[("content-type", "application/json-patch+json")],
                )
                .await?;
            assert_status(status, StatusCode::NOT_FOUND, "conditional patch");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_patch_applies_when_one_match() -> anyhow::Result<()> {
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

            let patch = json!([{ "op": "replace", "path": "/active", "value": false }]);
            let (status, _headers, body) = app
                .request_with_extra_headers(
                    Method::PATCH,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&patch)?),
                    &[("content-type", "application/json-patch+json")],
                )
                .await?;
            assert_status(status, StatusCode::OK, "conditional patch");
            let patched: serde_json::Value = serde_json::from_slice(&body)?;
            assert_resource_id(&patched, &id)?;
            assert_version_id(&patched, "2")?;
            assert_eq!(patched["active"], false);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_patch_returns_412_on_multiple_matches() -> anyhow::Result<()> {
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

            let patch = json!([{ "op": "replace", "path": "/active", "value": false }]);
            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::PATCH,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|DUP",
                    Some(to_json_body(&patch)?),
                    &[("content-type", "application/json-patch+json")],
                )
                .await?;
            assert_status(status, StatusCode::PRECONDITION_FAILED, "conditional patch");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn conditional_patch_returns_422_on_failed_test_op() -> anyhow::Result<()> {
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

            // active is true in `patient_with_mrn`, so this test op should fail and yield 422.
            let patch = json!([{ "op": "test", "path": "/active", "value": false }]);
            let (status, _headers, _body) = app
                .request_with_extra_headers(
                    Method::PATCH,
                    "/fhir/Patient?identifier=http://example.org/fhir/mrn|123",
                    Some(to_json_body(&patch)?),
                    &[("content-type", "application/json-patch+json")],
                )
                .await?;
            assert_status(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "conditional patch",
            );

            Ok(())
        })
    })
    .await
}
