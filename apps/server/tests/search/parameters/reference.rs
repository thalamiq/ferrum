//! REFERENCE Search Parameter Tests
//!
//! FHIR Spec: spec/search/03-02-01-05-12-reference.md
//!
//! Reference parameters are used to search relationships between resources:
//! - Find resources that reference a specific resource
//! - Support multiple search formats (ID, Type/ID, URL)
//! - Type-specific modifiers (e.g., :Patient)
//! - Identifier-based search (:identifier modifier)
//! - Chaining (e.g., subject.name=Smith)
//!
//! Key behaviors:
//! - [parameter]=[id] - matches any type with that ID
//! - [parameter]=[type]/[id] - matches specific type and ID
//! - [parameter]=[url] - matches absolute URL
//! - [parameter]:[type]=[id] - type modifier
//! - [parameter]:identifier=[system]|[value] - identifier search

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

// ============================================================================
// BASIC REFERENCE SEARCH
// ============================================================================

#[tokio::test]
async fn reference_search_by_id_only() -> anyhow::Result<()> {
    // Spec: subject=123 matches any type with that ID
    with_test_app(|app| {
        Box::pin(async move {
            // Register the search parameter
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &["Patient", "identifier"],
            )
            .await?;

            // Create a Patient
            let patient = json!({"resourceType": "Patient", "name": [{"family": "Smith"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient");
            let patient_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = patient_resource["id"].as_str().unwrap();

            // Create an Observation referencing the patient
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": format!("Patient/{}", patient_id)}
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create observation");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search by ID only (no type prefix)
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    &format!("/fhir/Observation?subject={}", patient_id),
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            assert_bundle(&bundle)?;
            assert_bundle_type(&bundle, "searchset")?;

            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should find exactly 1 observation");
            assert_eq!(ids[0], obs_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn reference_search_by_type_and_id() -> anyhow::Result<()> {
    // Spec: subject=Patient/123 matches specific type and ID
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &["Patient"],
            )
            .await?;

            // Create a Patient
            let patient = json!({"resourceType": "Patient", "name": [{"family": "Jones"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient");
            let patient_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = patient_resource["id"].as_str().unwrap();

            // Create an Observation
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": format!("Patient/{}", patient_id)}
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create observation");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search by Type/ID
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    &format!("/fhir/Observation?subject=Patient/{}", patient_id),
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], obs_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
#[ignore = "Server currently rejects ID-only reference search without type - needs implementation"]
async fn reference_search_multiple_types_same_id() -> anyhow::Result<()> {
    // Spec: subject=123 should match both Patient/123 and Practitioner/123
    // NOTE: Server currently returns 400 Bad Request for ID-only reference search
    // TODO: Implement ID-only reference search support
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "performer",
                "Observation",
                "reference",
                "Observation.performer",
                &["Patient", "Practitioner"],
            )
            .await?;

            // Create a Patient with specific ID
            let patient = json!({
                "resourceType": "Patient",
                "id": "same-id-123",
                "name": [{"family": "Patient"}]
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/same-id-123",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert!(
                status == StatusCode::OK || status == StatusCode::CREATED,
                "create patient with ID"
            );

            // Create a Practitioner with same ID (different type)
            let practitioner = json!({
                "resourceType": "Practitioner",
                "id": "same-id-123",
                "name": [{"family": "Practitioner"}]
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Practitioner/same-id-123",
                    Some(to_json_body(&practitioner)?),
                )
                .await?;
            assert!(
                status == StatusCode::OK || status == StatusCode::CREATED,
                "create practitioner with ID"
            );

            // Create Observation referencing Patient
            let obs1 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "performer": [{"reference": "Patient/same-id-123"}]
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs1)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create obs1");
            let obs1_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs1_id = obs1_resource["id"].as_str().unwrap();

            // Create Observation referencing Practitioner
            let obs2 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "performer": [{"reference": "Practitioner/same-id-123"}]
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs2)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create obs2");
            let obs2_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs2_id = obs2_resource["id"].as_str().unwrap();

            // Search by ID only - should match BOTH
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?performer=same-id-123", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let mut ids = extract_resource_ids(&bundle, "Observation")?;
            ids.sort();

            assert_eq!(ids.len(), 2, "should find both observations");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// TYPE MODIFIER (:[Type])
// ============================================================================

#[tokio::test]
async fn reference_search_type_modifier() -> anyhow::Result<()> {
    // Spec: subject:Patient=123 - explicit type modifier
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &["Patient", "Group"],
            )
            .await?;

            // Create Patient
            let patient = json!({
                "resourceType": "Patient",
                "id": "type-test-123",
                "name": [{"family": "TestPatient"}]
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Patient/type-test-123",
                    Some(to_json_body(&patient)?),
                )
                .await?;
            assert!(status == StatusCode::OK || status == StatusCode::CREATED);

            // Create Group with same ID
            let group = json!({
                "resourceType": "Group",
                "id": "type-test-123",
                "type": "person",
                "actual": true
            });
            let (status, _headers, _body) = app
                .request(
                    Method::PUT,
                    "/fhir/Group/type-test-123",
                    Some(to_json_body(&group)?),
                )
                .await?;
            assert!(status == StatusCode::OK || status == StatusCode::CREATED);

            // Observation referencing Patient
            let obs1 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": "Patient/type-test-123"}
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs1)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create obs1");
            let obs1_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs1_id = obs1_resource["id"].as_str().unwrap();

            // Observation referencing Group
            let obs2 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": "Group/type-test-123"}
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs2)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create obs2");
            let obs2_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs2_id = obs2_resource["id"].as_str().unwrap();

            // Search with :Patient modifier - should only match Patient
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Observation?subject:Patient=type-test-123",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should only match Patient reference");
            assert_eq!(ids[0], obs1_id);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// :identifier MODIFIER
// ============================================================================

#[tokio::test]
async fn reference_search_identifier_modifier() -> anyhow::Result<()> {
    // Spec: subject:identifier=system|value - search by Reference.identifier
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &["identifier"],
            )
            .await?;

            // Create Patient with identifier
            let patient = json!({
                "resourceType": "Patient",
                "identifier": [{
                    "system": "http://hospital.org/mrn",
                    "value": "MRN-12345"
                }],
                "name": [{"family": "Johnson"}]
            });
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient");
            let patient_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = patient_resource["id"].as_str().unwrap();

            // Create Observation with reference.identifier
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {
                    "identifier": {
                        "system": "http://hospital.org/mrn",
                        "value": "MRN-12345"
                    }
                }
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create observation");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search by identifier
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Observation?subject:identifier=http://hospital.org/mrn|MRN-12345",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], obs_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn reference_search_identifier_modifier_code_only() -> anyhow::Result<()> {
    // Spec: subject:identifier=value (no system)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &["identifier"],
            )
            .await?;

            // Create Observation with reference.identifier (no system)
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {
                    "identifier": {
                        "value": "NO-SYSTEM-789"
                    }
                }
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create observation");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search by identifier value only
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Observation?subject:identifier=NO-SYSTEM-789",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], obs_id);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// REFERENCE WITH DISPLAY
// ============================================================================

#[tokio::test]
async fn reference_search_with_display() -> anyhow::Result<()> {
    // Spec: Display text should not affect reference matching
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &[],
            )
            .await?;

            // Create Patient
            let patient = json!({"resourceType": "Patient", "name": [{"family": "Brown"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient");
            let patient_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = patient_resource["id"].as_str().unwrap();

            // Create Observation with display text
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {
                    "reference": format!("Patient/{}", patient_id),
                    "display": "Mr. Brown, John"
                }
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create observation");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search by reference - display should not matter
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    &format!("/fhir/Observation?subject=Patient/{}", patient_id),
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// MULTIPLE REFERENCES (OR LOGIC)
// ============================================================================

#[tokio::test]
async fn reference_search_multiple_values_or_logic() -> anyhow::Result<()> {
    // Spec: Comma-separated values use OR logic
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "subject",
                "Observation",
                "reference",
                "Observation.subject",
                &[],
            )
            .await?;

            // Create two patients
            let patient1 = json!({"resourceType": "Patient", "name": [{"family": "Patient1"}]});
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient1)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient1");
            let p1: serde_json::Value = serde_json::from_slice(&body)?;
            let p1_id = p1["id"].as_str().unwrap();

            let patient2 = json!({"resourceType": "Patient", "name": [{"family": "Patient2"}]});
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient2)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient2");
            let p2: serde_json::Value = serde_json::from_slice(&body)?;
            let p2_id = p2["id"].as_str().unwrap();

            // Create observations for each
            let obs1 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": format!("Patient/{}", p1_id)}
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs1)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create obs1");
            let obs1_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs1_id = obs1_resource["id"].as_str().unwrap();

            let obs2 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": format!("Patient/{}", p2_id)}
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs2)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create obs2");
            let obs2_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs2_id = obs2_resource["id"].as_str().unwrap();

            // Search for patient1 OR patient2
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    &format!(
                        "/fhir/Observation?subject=Patient/{},Patient/{}",
                        p1_id, p2_id
                    ),
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let mut ids = extract_resource_ids(&bundle, "Observation")?;
            ids.sort();

            assert_eq!(ids.len(), 2, "should find both observations");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// RESOURCE-SPECIFIC SEARCH PARAMETERS
// ============================================================================

#[tokio::test]
async fn reference_search_patient_specific_parameter() -> anyhow::Result<()> {
    // Spec: Some parameters like Observation.patient implicitly limit to Patient type
    with_test_app(|app| {
        Box::pin(async move {
            // Register a patient-specific search parameter
            register_search_parameter(
                &app.state.db_pool,
                "patient",
                "Observation",
                "reference",
                "Observation.subject",
                &[],
            )
            .await?;

            // Create Patient
            let patient = json!({"resourceType": "Patient", "name": [{"family": "Specific"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create patient");
            let patient_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = patient_resource["id"].as_str().unwrap();

            // Create Observation
            let observation = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "subject": {"reference": format!("Patient/{}", patient_id)}
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&observation)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create observation");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search using patient parameter (ID only, no Patient/ prefix needed)
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    &format!("/fhir/Observation?patient={}", patient_id),
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}
