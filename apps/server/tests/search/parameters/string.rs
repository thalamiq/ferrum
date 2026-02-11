//! STRING Search Parameter Tests
//!
//! FHIR Spec: spec/search/03-02-01-05-13-string.md
//!
//! String parameters are used for:
//! - Names (HumanName: family, given, prefix, suffix)
//! - Addresses (Address: line, city, state, postalCode, country)
//! - Simple string fields
//!
//! Key behaviors:
//! - Default: case-insensitive prefix matching
//! - Ignores accents/diacritical marks (combining characters)
//! - Ignores punctuation and non-significant whitespace
//! - :contains modifier - substring matching anywhere in field
//! - :exact modifier - exact match including case and accents
//! - HumanName/Address parts searched independently

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

// ============================================================================
// BASIC STRING SEARCH (PREFIX MATCHING)
// ============================================================================

#[tokio::test]
async fn string_search_default_prefix_matching() -> anyhow::Result<()> {
    // Spec: Default string search uses prefix matching (starts with)
    with_test_app(|app| {
        Box::pin(async move {
            // Register the search parameter
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["missing", "exact", "contains"],
            )
            .await?;

            // Create patients with different family names
            let patient1 = json!({
                "resourceType": "Patient",
                "name": [{"family": "Johnson"}]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient1)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create Johnson");
            let p1: serde_json::Value = serde_json::from_slice(&body)?;
            let p1_id = p1["id"].as_str().unwrap();

            let patient2 = json!({
                "resourceType": "Patient",
                "name": [{"family": "Smith"}]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Patient",
                    Some(to_json_body(&patient2)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create Smith");

            // Search by prefix "John" - should match "Johnson"
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family=John", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            assert_bundle(&bundle)?;
            assert_bundle_type(&bundle, "searchset")?;

            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1, "should find exactly 1 patient");
            assert_eq!(ids[0], p1_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn string_search_case_insensitive() -> anyhow::Result<()> {
    // Spec: String search is case-insensitive by default
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["exact"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "Smith"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search with lowercase - should match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family=smith", None)
                .await?;

            assert_status(status, StatusCode::OK, "search lowercase");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            // Search with uppercase - should also match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family=SMITH", None)
                .await?;

            assert_status(status, StatusCode::OK, "search uppercase");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn string_search_prefix_matching_not_substring() -> anyhow::Result<()> {
    // Spec: Default search is PREFIX, not substring (use :contains for substring)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["contains"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "Johnson"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search for "son" (suffix) - should NOT match without :contains
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family=son", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 0, "prefix search should NOT match suffix");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// :contains MODIFIER (SUBSTRING MATCHING)
// ============================================================================

#[tokio::test]
async fn string_contains_modifier_substring_matching() -> anyhow::Result<()> {
    // Spec: :contains modifier matches substring anywhere in field
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["contains"],
            )
            .await?;

            // Create patients with different family names
            let patients = vec![
                ("Son", "son"),             // Exact match
                ("Sonder", "sonder"),       // Starts with
                ("Erikson", "erikson"),     // Ends with
                ("Samsonite", "samsonite"), // Contains
            ];

            let mut patient_ids = Vec::new();

            for (name, normalized) in &patients {
                let patient = json!({
                    "resourceType": "Patient",
                    "name": [{"family": name}]
                });

                let (status, _headers, body) = app
                    .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, &format!("create {}", name));
                let created: serde_json::Value = serde_json::from_slice(&body)?;
                let id = created["id"].as_str().unwrap();
                patient_ids.push(id.to_string());
            }

            // Search with :contains for "son"
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:contains=son", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;

            // Should match all 4 patients
            assert_eq!(ids.len(), 4, "should match substring anywhere");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn string_contains_modifier_case_insensitive() -> anyhow::Result<()> {
    // Spec: :contains is case-insensitive
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["contains"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "McDonald"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search with different case
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:contains=DONALD", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1, "case-insensitive substring match");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// :exact MODIFIER (EXACT MATCHING)
// ============================================================================

#[tokio::test]
async fn string_exact_modifier_case_sensitive() -> anyhow::Result<()> {
    // Spec: :exact modifier is case-sensitive
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["exact"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "Son"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search with exact case - should match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:exact=Son", None)
                .await?;

            assert_status(status, StatusCode::OK, "search exact case");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            // Search with wrong case - should NOT match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:exact=son", None)
                .await?;

            assert_status(status, StatusCode::OK, "search wrong case");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 0, "exact search is case-sensitive");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn string_exact_modifier_full_match_only() -> anyhow::Result<()> {
    // Spec: :exact requires full match, not prefix
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &["exact"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "Johnson"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Exact full match - should match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:exact=Johnson", None)
                .await?;

            assert_status(status, StatusCode::OK, "search exact");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            // Prefix only - should NOT match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:exact=John", None)
                .await?;

            assert_status(status, StatusCode::OK, "search prefix");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 0, "exact requires full match");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// HUMANNAME SEARCH (MULTIPLE PARTS)
// ============================================================================

#[tokio::test]
async fn string_search_humanname_given_name() -> anyhow::Result<()> {
    // Spec: String search on HumanName should search name parts
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "given",
                "Patient",
                "string",
                "Patient.name.given",
                &["exact"],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{
                    "given": ["John", "Michael"],
                    "family": "Smith"
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search for first given name
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?given=John", None)
                .await?;

            assert_status(status, StatusCode::OK, "search first given");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            // Search for second given name
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?given=Mich", None)
                .await?;

            assert_status(status, StatusCode::OK, "search second given");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn string_search_humanname_family_parts() -> anyhow::Result<()> {
    // Spec: Family name parts should be searched independently
    // e.g., "Carreno" or "Quinones" should match "Carreno Quinones"
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &[],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "name": [{"family": "Carreno Quinones"}]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search for first part
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family=Carreno", None)
                .await?;

            assert_status(status, StatusCode::OK, "search first part");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            // Search for second part
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family:contains=Quinones", None)
                .await?;

            assert_status(status, StatusCode::OK, "search second part");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// MULTIPLE VALUES (OR LOGIC)
// ============================================================================

#[tokio::test]
async fn string_search_multiple_values_or_logic() -> anyhow::Result<()> {
    // Spec: Comma-separated values use OR logic
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "family",
                "Patient",
                "string",
                "Patient.name.family",
                &[],
            )
            .await?;

            // Create multiple patients
            let smith = json!({"resourceType": "Patient", "name": [{"family": "Smith"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&smith)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create Smith");
            let smith_created: serde_json::Value = serde_json::from_slice(&body)?;
            let smith_id = smith_created["id"].as_str().unwrap();

            let jones = json!({"resourceType": "Patient", "name": [{"family": "Jones"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&jones)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create Jones");
            let jones_created: serde_json::Value = serde_json::from_slice(&body)?;
            let jones_id = jones_created["id"].as_str().unwrap();

            let brown = json!({"resourceType": "Patient", "name": [{"family": "Brown"}]});
            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&brown)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create Brown");

            // Search for Smith OR Jones
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?family=Smith,Jones", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let mut ids = extract_resource_ids(&bundle, "Patient")?;
            ids.sort();

            let mut expected = vec![smith_id.to_string(), jones_id.to_string()];
            expected.sort();

            assert_eq!(ids, expected, "should find Smith OR Jones");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// ADDRESS SEARCH
// ============================================================================

#[tokio::test]
async fn string_search_address_city() -> anyhow::Result<()> {
    // Spec: String search on Address should search address parts
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "address-city",
                "Patient",
                "string",
                "Patient.address.city",
                &[],
            )
            .await?;

            let patient = json!({
                "resourceType": "Patient",
                "address": [{
                    "line": ["123 Main St"],
                    "city": "Springfield",
                    "state": "IL",
                    "postalCode": "62701"
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search by city prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?address-city=Spring", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}
