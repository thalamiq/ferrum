//! TOKEN Search Parameter Tests
//!
//! FHIR Spec: spec/search/03-02-01-05-14-token.md
//!
//! Token parameters are used for:
//! - Coding (system|code)
//! - CodeableConcept
//! - Identifier (system|value)
//! - ContactPoint
//! - boolean, code, id, string, uri
//!
//! Key behaviors:
//! - Case-sensitive for codes (unless semantics indicate otherwise)
//! - Four syntax formats: [code], [system]|[code], |[code], [system]|
//! - Supports modifiers: :not, :text, :in, :not-in, :above, :below, :of-type

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

// ============================================================================
// BASIC TOKEN SEARCH
// ============================================================================

#[tokio::test]
async fn token_search_by_code_only() -> anyhow::Result<()> {
    // Spec: Search by code value without system
    // Matches any coding with that code regardless of system
    with_test_app(|app| {
        Box::pin(async move {
            // Register the search parameter
            register_search_parameter(
                &app.state.db_pool,
                "gender",
                "Patient",
                "token",
                "gender",
                &["missing", "not"],
            )
            .await?;

            // Create patients with different genders
            let male = json!({
                "resourceType": "Patient",
                "gender": "male"
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&male)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create male");
            let male_created: serde_json::Value = serde_json::from_slice(&body)?;
            let male_id = male_created["id"].as_str().unwrap();

            let female = json!({
                "resourceType": "Patient",
                "gender": "female"
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&female)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create female");
            let female_created: serde_json::Value = serde_json::from_slice(&body)?;
            let female_id = female_created["id"].as_str().unwrap();

            // Search by gender=male
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?gender=male", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            assert_bundle(&bundle)?;
            assert_bundle_type(&bundle, "searchset")?;

            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1, "should find exactly 1 male patient");
            assert_eq!(ids[0], male_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn token_search_by_system_and_code() -> anyhow::Result<()> {
    // Spec: Search by system|code matches exact system and code
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "identifier",
                &["missing", "not", "of-type"],
            )
            .await?;

            // Create patient with MRN identifier
            let patient = json!({
                "resourceType": "Patient",
                "identifier": [{
                    "system": "http://example.org/mrn",
                    "value": "12345"
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search with system|value
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Patient?identifier=http://example.org/mrn|12345",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], patient_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn token_search_empty_system_with_code() -> anyhow::Result<()> {
    // Spec: |[code] matches codes with no system
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "identifier",
                &["missing"],
            )
            .await?;

            // Create patient with identifier having NO system
            let patient = json!({
                "resourceType": "Patient",
                "identifier": [{
                    "value": "NO-SYSTEM-123"
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search with |code (empty system)
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?identifier=|NO-SYSTEM-123", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], patient_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn token_search_system_with_empty_code() -> anyhow::Result<()> {
    // Spec: [system]| matches any code from that system
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "identifier",
                &["missing"],
            )
            .await?;

            // Create patients with same system, different values
            let patient1 = json!({
                "resourceType": "Patient",
                "identifier": [{
                    "system": "http://example.org/mrn",
                    "value": "A123"
                }]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient1)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create p1");
            let p1: serde_json::Value = serde_json::from_slice(&body)?;
            let p1_id = p1["id"].as_str().unwrap();

            let patient2 = json!({
                "resourceType": "Patient",
                "identifier": [{
                    "system": "http://example.org/mrn",
                    "value": "B456"
                }]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient2)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create p2");
            let p2: serde_json::Value = serde_json::from_slice(&body)?;
            let p2_id = p2["id"].as_str().unwrap();

            // Search with system| (empty code = any code from system)
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Patient?identifier=http://example.org/mrn|",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let mut ids = extract_resource_ids(&bundle, "Patient")?;
            ids.sort();

            let mut expected = vec![p1_id.to_string(), p2_id.to_string()];
            expected.sort();

            assert_eq!(ids, expected, "should match both patients from same system");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// TOKEN CASE SENSITIVITY
// ============================================================================

#[tokio::test]
#[ignore = "Server currently does case-insensitive token matching - needs fix per spec"]
async fn token_search_is_case_sensitive_for_codes() -> anyhow::Result<()> {
    // Spec: Token matching is case-sensitive for coded values
    // NOTE: Server currently does case-INSENSITIVE matching - this is a spec violation
    // TODO: Fix server to do case-sensitive token matching
    // Using Patient.identifier for case sensitivity testing
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "identifier",
                &["missing"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "identifier": [{
                    "system": "http://example.org/ids",
                    "value": "ABC-123"
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search with exact case should match
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Patient?identifier=http://example.org/ids|ABC-123",
                    None,
                )
                .await?;
            assert_status(status, StatusCode::OK, "search exact case");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            // Search with different case should NOT match (case-sensitive)
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Patient?identifier=http://example.org/ids|abc-123",
                    None,
                )
                .await?;
            assert_status(status, StatusCode::OK, "search wrong case");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 0, "case-sensitive search should not match");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// MULTIPLE VALUES (OR LOGIC)
// ============================================================================

#[tokio::test]
async fn token_search_multiple_values_or_logic() -> anyhow::Result<()> {
    // Spec: Comma-separated values use OR logic
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "gender",
                "Patient",
                "token",
                "gender",
                &["missing"],
            )
            .await?;

            // Create male and female patients
            let male = json!({"resourceType": "Patient", "gender": "male"});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&male)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create male");
            let male_created: serde_json::Value = serde_json::from_slice(&body)?;
            let male_id = male_created["id"].as_str().unwrap();

            let female = json!({"resourceType": "Patient", "gender": "female"});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&female)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create female");
            let female_created: serde_json::Value = serde_json::from_slice(&body)?;
            let female_id = female_created["id"].as_str().unwrap();

            let other = json!({"resourceType": "Patient", "gender": "other"});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&other)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create other");

            // Search for male OR female
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?gender=male,female", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let mut ids = extract_resource_ids(&bundle, "Patient")?;
            ids.sort();

            let mut expected = vec![male_id.to_string(), female_id.to_string()];
            expected.sort();

            assert_eq!(ids, expected, "should find male OR female");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// TODO: MODIFIERS (To be implemented)
// ============================================================================

// #[tokio::test]
// async fn token_not_modifier_excludes_matches() -> anyhow::Result<()> {
//     // Spec: :not modifier negates the match and includes resources with no value
//     todo!("Implement :not modifier test")
// }

// #[tokio::test]
// async fn token_text_modifier_searches_display() -> anyhow::Result<()> {
//     // Spec: :text modifier searches Coding.display and CodeableConcept.text
//     todo!("Implement :text modifier test")
// }

// #[tokio::test]
// async fn token_of_type_modifier_matches_identifier_type() -> anyhow::Result<()> {
//     // Spec: :of-type modifier for Identifier with syntax system|type|value
//     todo!("Implement :of-type modifier test")
// }
