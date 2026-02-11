//! NUMBER Search Parameter Tests
//!
//! FHIR Spec: spec/search/03-02-01-05-10-number.md
//!
//! Number parameters search on numerical values:
//! - Implicit precision based on significant figures
//! - Exact matching for integers
//! - Prefix operators: eq, lt, le, gt, ge, ne
//! - Exponential notation support
//!
//! Key behaviors:
//! - 100 matches [99.5, 100.5) (3 sig figs)
//! - 100.00 matches [99.995, 100.005) (5 sig figs)
//! - lt100, gt100, etc. use exact values
//! - Integer matching is exact when no decimal point

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

// ============================================================================
// BASIC NUMBER SEARCH (EQUALITY WITH IMPLICIT PRECISION)
// ============================================================================

#[tokio::test]
async fn number_search_integer_exact_match() -> anyhow::Result<()> {
    // Spec: Integer matching is exact when no decimal point
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "dose-number",
                "ImmunizationRecommendation",
                "number",
                "ImmunizationRecommendation.recommendation.doseNumber",
                &[],
            )
            .await?;

            // Create immunization recommendation with dose number
            let imm_rec = json!({
                "resourceType": "ImmunizationRecommendation",
                "patient": {"reference": "Patient/example"},
                "recommendation": [{
                    "vaccineCode": [{"coding": [{"system": "http://example.org", "code": "COVID"}]}],
                    "doseNumber": 2
                }]
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/ImmunizationRecommendation", Some(to_json_body(&imm_rec)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let rec_id = created["id"].as_str().unwrap();

            // Search for exact integer
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/ImmunizationRecommendation?dose-number=2", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "ImmunizationRecommendation")?;
            assert_eq!(ids.len(), 1);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn number_search_decimal_implicit_range() -> anyhow::Result<()> {
    // Spec: 100 matches [99.5, 100.5) with 3 significant figures
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "probability",
                "RiskAssessment",
                "number",
                "RiskAssessment.prediction.probability",
                &[],
            )
            .await?;

            // Create risk assessment with probability 100.2 (should match search for 100)
            let risk = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{
                    "outcome": {"text": "Heart Attack"},
                    "probabilityDecimal": 100.2
                }]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let risk_id = created["id"].as_str().unwrap();

            // Search for 100 (implicit range [99.5, 100.5))
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/RiskAssessment?probability=100", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "RiskAssessment")?;
            assert_eq!(
                ids.len(),
                1,
                "100.2 should match search for 100 (within implicit range)"
            );

            Ok(())
        })
    })
    .await
}

// ============================================================================
// COMPARISON OPERATORS
// ============================================================================

#[tokio::test]
async fn number_search_greater_than() -> anyhow::Result<()> {
    // Spec: gt100 matches values > 100 (exact)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "probability",
                "RiskAssessment",
                "number",
                "RiskAssessment.prediction.probability",
                &[],
            )
            .await?;

            // Create two risk assessments
            let risk_low = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{"outcome": {"text": "Low"}, "probabilityDecimal": 0.7}]
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk_low)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create low");

            let risk_high = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{"outcome": {"text": "High"}, "probabilityDecimal": 0.9}]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk_high)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create high");
            let high_created: serde_json::Value = serde_json::from_slice(&body)?;
            let high_id = high_created["id"].as_str().unwrap();

            // Search with gt prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/RiskAssessment?probability=gt0.8", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "RiskAssessment")?;
            assert_eq!(ids.len(), 1, "should only match probability > 0.8");
            assert_eq!(ids[0], high_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn number_search_less_than() -> anyhow::Result<()> {
    // Spec: lt100 matches values < 100 (exact)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "probability",
                "RiskAssessment",
                "number",
                "RiskAssessment.prediction.probability",
                &[],
            )
            .await?;

            let risk_low = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{"outcome": {"text": "Low"}, "probabilityDecimal": 0.3}]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk_low)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create low");
            let low_created: serde_json::Value = serde_json::from_slice(&body)?;
            let low_id = low_created["id"].as_str().unwrap();

            let risk_high = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{"outcome": {"text": "High"}, "probabilityDecimal": 0.9}]
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk_high)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create high");

            // Search with lt prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/RiskAssessment?probability=lt0.5", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "RiskAssessment")?;
            assert_eq!(ids.len(), 1, "should only match probability < 0.5");
            assert_eq!(ids[0], low_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn number_search_greater_or_equal() -> anyhow::Result<()> {
    // Spec: ge100 matches values >= 100 (exact)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "probability",
                "RiskAssessment",
                "number",
                "RiskAssessment.prediction.probability",
                &[],
            )
            .await?;

            let risk_exact = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{"outcome": {"text": "Exact"}, "probabilityDecimal": 0.5}]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk_exact)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let exact_created: serde_json::Value = serde_json::from_slice(&body)?;
            let exact_id = exact_created["id"].as_str().unwrap();

            // Search with ge prefix - should include exact match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/RiskAssessment?probability=ge0.5", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "RiskAssessment")?;
            assert_eq!(ids.len(), 1, "ge should include exact match");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn number_search_less_or_equal() -> anyhow::Result<()> {
    // Spec: le100 matches values <= 100 (exact)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "probability",
                "RiskAssessment",
                "number",
                "RiskAssessment.prediction.probability",
                &[],
            )
            .await?;

            let risk_exact = json!({
                "resourceType": "RiskAssessment",
                "status": "final",
                "subject": {"reference": "Patient/example"},
                "prediction": [{"outcome": {"text": "Exact"}, "probabilityDecimal": 0.5}]
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/RiskAssessment",
                    Some(to_json_body(&risk_exact)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let exact_created: serde_json::Value = serde_json::from_slice(&body)?;
            let exact_id = exact_created["id"].as_str().unwrap();

            // Search with le prefix - should include exact match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/RiskAssessment?probability=le0.5", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "RiskAssessment")?;
            assert_eq!(ids.len(), 1, "le should include exact match");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn number_search_not_equal() -> anyhow::Result<()> {
    // Spec: ne100 excludes values in implicit range
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value",
                "Observation",
                "number",
                "Observation.valueQuantity.value",
                &[],
            )
            .await?;

            let obs_match = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Match"},
                "valueQuantity": {"value": 100.0, "unit": "mg"}
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_match)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create match");

            let obs_diff = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Different"},
                "valueQuantity": {"value": 200.0, "unit": "mg"}
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_diff)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create different");
            let diff_created: serde_json::Value = serde_json::from_slice(&body)?;
            let diff_id = diff_created["id"].as_str().unwrap();

            // Search with ne prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value=ne100", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "ne should exclude matching value");
            assert_eq!(ids[0], diff_id);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// RANGE SEARCH (MULTIPLE CRITERIA)
// ============================================================================

#[tokio::test]
async fn number_search_range_with_multiple_params() -> anyhow::Result<()> {
    // Spec: Multiple params with same name = AND logic
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value",
                "Observation",
                "number",
                "Observation.valueQuantity.value",
                &[],
            )
            .await?;

            // Create observations with different values
            let obs_low = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Low"},
                "valueQuantity": {"value": 50.0, "unit": "mg"}
            });
            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_low)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create low");

            let obs_mid = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Mid"},
                "valueQuantity": {"value": 100.0, "unit": "mg"}
            });
            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_mid)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create mid");
            let mid_created: serde_json::Value = serde_json::from_slice(&body)?;
            let mid_id = mid_created["id"].as_str().unwrap();

            let obs_high = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "High"},
                "valueQuantity": {"value": 150.0, "unit": "mg"}
            });
            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_high)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create high");

            // Search for range: ge75 AND le125
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Observation?value=ge75&value=le125",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should only match mid observation in range");
            assert_eq!(ids[0], mid_id);

            Ok(())
        })
    })
    .await
}
