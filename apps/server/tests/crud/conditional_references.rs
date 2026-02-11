//! Conditional reference resolution tests (search-URI in `Reference.reference`).

use crate::support::{
    assert_status, patient_with_mrn, register_search_parameter, to_json_body, with_test_app,
};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn create_resolves_conditional_reference() -> anyhow::Result<()> {
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
            let (_status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            let created_patient: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created_patient["id"].as_str().unwrap().to_string();

            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": { "text": "test" },
                "subject": { "reference": "Patient?identifier=http://example.org/fhir/mrn|123" }
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            let created: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(
                created["subject"]["reference"].as_str().unwrap(),
                format!("Patient/{}", patient_id)
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn create_conditional_reference_no_match_fails_and_does_not_persist() -> anyhow::Result<()> {
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

            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": { "text": "test" },
                "subject": { "reference": "Patient?identifier=http://example.org/fhir/mrn|missing" }
            });
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&observation)?))
                .await?;
            assert_status(status, StatusCode::PRECONDITION_FAILED, "create");

            let outcome: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(outcome["resourceType"], "OperationOutcome");

            let obs_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM resources WHERE resource_type = 'Observation' AND is_current = true AND deleted = false",
            )
            .fetch_one(&app.state.db_pool)
            .await?;
            assert_eq!(obs_count, 0);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn update_resolves_conditional_reference() -> anyhow::Result<()> {
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

            let patient_a = patient_with_mrn("Doe", "123");
            let (_status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient_a)?),
                )
                .await?;
            let created_a: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_a_id = created_a["id"].as_str().unwrap().to_string();

            let patient_b = patient_with_mrn("Roe", "456");
            let (_status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient_b)?),
                )
                .await?;
            let created_b: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_b_id = created_b["id"].as_str().unwrap().to_string();

            let obs = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": { "text": "test" },
                "subject": { "reference": format!("Patient/{patient_a_id}") }
            });
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&obs)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created_obs: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = created_obs["id"].as_str().unwrap().to_string();

            let update = json!({
                "resourceType": "Observation",
                "id": obs_id,
                "status": "final",
                "code": { "text": "test" },
                "subject": { "reference": "Patient?identifier=http://example.org/fhir/mrn|456" }
            });
            let (status, _headers, body) = app
                .request(
                    Method::PUT,
                    &format!("/fhir/Observation/{}", update["id"].as_str().unwrap()),
                    Some(to_json_body(&update)?),
                )
                .await?;
            assert_status(status, StatusCode::OK, "update");

            let updated: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(
                updated["subject"]["reference"].as_str().unwrap(),
                format!("Patient/{}", patient_b_id)
            );

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn patch_resolves_conditional_reference() -> anyhow::Result<()> {
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

            let patient_a = patient_with_mrn("Doe", "123");
            let (_status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient_a)?))
                .await?;
            let created_a: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_a_id = created_a["id"].as_str().unwrap().to_string();

            let patient_b = patient_with_mrn("Roe", "456");
            let (_status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient_b)?))
                .await?;
            let created_b: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_b_id = created_b["id"].as_str().unwrap().to_string();

            let obs = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": { "text": "test" },
                "subject": { "reference": format!("Patient/{patient_a_id}") }
            });
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&obs)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created_obs: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = created_obs["id"].as_str().unwrap().to_string();

            let patch = json!([
                { "op": "replace", "path": "/subject/reference", "value": "Patient?identifier=http://example.org/fhir/mrn|456" }
            ]);
            let (status, _headers, body) = app
                .request_with_extra_headers(
                    Method::PATCH,
                    &format!("/fhir/Observation/{obs_id}"),
                    Some(to_json_body(&patch)?),
                    &[("content-type", "application/json-patch+json")],
                )
                .await?;
            assert_status(status, StatusCode::OK, "patch");

            let patched: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(
                patched["subject"]["reference"].as_str().unwrap(),
                format!("Patient/{}", patient_b_id)
            );

            Ok(())
        })
    })
    .await
}
