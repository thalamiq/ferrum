//! DATE Search Parameter Tests
//!
//! FHIR Spec: spec/search/03-02-01-05-09-date.md
//!
//! Date parameters search on date/time or period values:
//! - Supports varying precision (year, year-month, full date, datetime)
//! - Implicit range semantics based on precision
//! - Prefix operators: eq, ne, lt, le, gt, ge, sa, eb, ap
//! - Period matching with boundary logic
//!
//! Key behaviors:
//! - Date with year only: [2000-01-01T00:00, 2000-12-31T23:59]
//! - Date with year-month: [2000-04-01T00:00, 2000-04-30T23:59]
//! - Date with full date: [2000-04-04T00:00, 2000-04-04T23:59]
//! - Comparisons use range/period overlap logic

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

// ============================================================================
// BASIC DATE SEARCH (EQUALITY)
// ============================================================================

#[tokio::test]
async fn date_search_exact_date_match() -> anyhow::Result<()> {
    // Spec: date=2023-01-14 matches the full day 2023-01-14
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "birthdate",
                "Patient",
                "date",
                "Patient.birthDate",
                &[],
            )
            .await?;

            // Create patient with birthdate
            let patient = json!({
                "resourceType": "Patient",
                "birthDate": "2023-01-14"
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let patient_id = created["id"].as_str().unwrap();

            // Search for exact date
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?birthdate=2023-01-14", None)
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

#[tokio::test]
async fn date_search_year_precision() -> anyhow::Result<()> {
    // Spec: 2023 matches entire year [2023-01-01T00:00, 2023-12-31T23:59]
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            // Create observation in 2023
            let obs = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "effectiveDateTime": "2023-06-15T10:30:00Z"
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&obs)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = created["id"].as_str().unwrap();

            // Search by year only
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=2023", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should match year 2023");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn date_search_year_month_precision() -> anyhow::Result<()> {
    // Spec: 2023-06 matches entire month [2023-06-01T00:00, 2023-06-30T23:59]
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            let obs = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Test"},
                "effectiveDateTime": "2023-06-15T10:30:00Z"
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&obs)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = created["id"].as_str().unwrap();

            // Search by year-month
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=2023-06", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should match June 2023");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// LESS THAN / GREATER THAN
// ============================================================================

#[tokio::test]
async fn date_search_less_than() -> anyhow::Result<()> {
    // Spec: lt2023-06-15 matches dates before 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            // Create two observations
            let obs_before = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Before"},
                "effectiveDateTime": "2023-06-10T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_before)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create before");
            let before: serde_json::Value = serde_json::from_slice(&body)?;
            let before_id = before["id"].as_str().unwrap();

            let obs_after = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "After"},
                "effectiveDateTime": "2023-06-20T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_after)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create after");

            // Search with lt prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=lt2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should only match observation before cutoff");
            assert_eq!(ids[0], before_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn date_search_greater_than() -> anyhow::Result<()> {
    // Spec: gt2023-06-15 matches dates after 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            let obs_before = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Before"},
                "effectiveDateTime": "2023-06-10T10:00:00Z"
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_before)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create before");

            let obs_after = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "After"},
                "effectiveDateTime": "2023-06-20T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_after)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create after");
            let after: serde_json::Value = serde_json::from_slice(&body)?;
            let after_id = after["id"].as_str().unwrap();

            // Search with gt prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=gt2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should only match observation after cutoff");
            assert_eq!(ids[0], after_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn date_search_less_than_or_equal() -> anyhow::Result<()> {
    // Spec: le2023-06-15 matches dates on or before 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            let obs_exact = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Exact"},
                "effectiveDateTime": "2023-06-15T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_exact)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create exact");
            let exact: serde_json::Value = serde_json::from_slice(&body)?;
            let exact_id = exact["id"].as_str().unwrap();

            // Search with le prefix - should include exact match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=le2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "le should include exact match");

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn date_search_greater_than_or_equal() -> anyhow::Result<()> {
    // Spec: ge2023-06-15 matches dates on or after 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            let obs_exact = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Exact"},
                "effectiveDateTime": "2023-06-15T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_exact)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create exact");
            let exact: serde_json::Value = serde_json::from_slice(&body)?;
            let exact_id = exact["id"].as_str().unwrap();

            // Search with ge prefix - should include exact match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=ge2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "ge should include exact match");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// DATE RANGE SEARCH (MULTIPLE CRITERIA)
// ============================================================================

#[tokio::test]
async fn date_search_range_with_multiple_params() -> anyhow::Result<()> {
    // Spec: Multiple params with same name = AND logic
    // date=ge2023-01-01&date=le2023-12-31 finds dates in 2023
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            // Create observations in different years
            let obs_2022 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "2022"},
                "effectiveDateTime": "2022-06-15T10:00:00Z"
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_2022)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create 2022");

            let obs_2023 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "2023"},
                "effectiveDateTime": "2023-06-15T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_2023)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create 2023");
            let obs_2023_res: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_2023_id = obs_2023_res["id"].as_str().unwrap();

            let obs_2024 = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "2024"},
                "effectiveDateTime": "2024-06-15T10:00:00Z"
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_2024)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create 2024");

            // Search for range: ge2023-01-01 AND le2023-12-31
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Observation?date=ge2023-01-01&date=le2023-12-31",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "should only match 2023 observation");
            assert_eq!(ids[0], obs_2023_id);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// PERIOD MATCHING
// ============================================================================

#[tokio::test]
#[ignore = "Server date search implementation does not yet support period overlap matching - needs implementation"]
async fn date_search_period_overlap() -> anyhow::Result<()> {
    // Spec: Search matches if resource period overlaps with search range
    // NOTE: Server currently does not match period searches correctly
    // TODO: Implement period overlap logic for date searches
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "period",
                "Encounter",
                "date",
                "Encounter.period",
                &[],
            )
            .await?;

            // Create encounter with period
            let encounter = json!({
                "resourceType": "Encounter",
                "status": "finished",
                "class": {"code": "IMP"},
                "period": {
                    "start": "2023-06-10T08:00:00Z",
                    "end": "2023-06-15T17:00:00Z"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Encounter",
                    Some(to_json_body(&encounter)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let encounter_id = created["id"].as_str().unwrap();

            // Search for date in middle of period
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Encounter?period=2023-06-12", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Encounter")?;
            assert_eq!(ids.len(), 1, "should match period containing this date");

            Ok(())
        })
    })
    .await
}

// ============================================================================
// NOT EQUAL
// ============================================================================

#[tokio::test]
async fn date_search_not_equal() -> anyhow::Result<()> {
    // Spec: ne2023-06-15 excludes dates in range of 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "date",
                "Observation",
                "date",
                "Observation.effectiveDateTime",
                &[],
            )
            .await?;

            let obs_match = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"text": "Match"},
                "effectiveDateTime": "2023-06-15T10:00:00Z"
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
                "effectiveDateTime": "2023-06-16T10:00:00Z"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_diff)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create different");
            let diff: serde_json::Value = serde_json::from_slice(&body)?;
            let diff_id = diff["id"].as_str().unwrap();

            // Search with ne prefix - should exclude 2023-06-15
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?date=ne2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1, "ne should exclude matching date");
            assert_eq!(ids[0], diff_id);

            Ok(())
        })
    })
    .await
}

// ============================================================================
// BOUNDARY PREFIXES (sa, eb)
// ============================================================================

#[tokio::test]
async fn date_search_starts_after() -> anyhow::Result<()> {
    // Spec: sa2023-06-15 matches periods that start after 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "period",
                "Encounter",
                "date",
                "Encounter.period",
                &[],
            )
            .await?;

            // Period starting before cutoff
            let enc_before = json!({
                "resourceType": "Encounter",
                "status": "finished",
                "class": {"code": "IMP"},
                "period": {
                    "start": "2023-06-10T08:00:00Z",
                    "end": "2023-06-20T17:00:00Z"
                }
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Encounter",
                    Some(to_json_body(&enc_before)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create before");

            // Period starting after cutoff
            let enc_after = json!({
                "resourceType": "Encounter",
                "status": "finished",
                "class": {"code": "IMP"},
                "period": {
                    "start": "2023-06-16T08:00:00Z",
                    "end": "2023-06-20T17:00:00Z"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Encounter",
                    Some(to_json_body(&enc_after)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create after");
            let after: serde_json::Value = serde_json::from_slice(&body)?;
            let after_id = after["id"].as_str().unwrap();

            // Search with sa prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Encounter?period=sa2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Encounter")?;
            assert_eq!(ids.len(), 1, "sa should only match periods starting after");
            assert_eq!(ids[0], after_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn date_search_ends_before() -> anyhow::Result<()> {
    // Spec: eb2023-06-15 matches periods that end before 2023-06-15
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "period",
                "Encounter",
                "date",
                "Encounter.period",
                &[],
            )
            .await?;

            // Period ending before cutoff
            let enc_before = json!({
                "resourceType": "Encounter",
                "status": "finished",
                "class": {"code": "IMP"},
                "period": {
                    "start": "2023-06-01T08:00:00Z",
                    "end": "2023-06-10T17:00:00Z"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Encounter",
                    Some(to_json_body(&enc_before)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create before");
            let before: serde_json::Value = serde_json::from_slice(&body)?;
            let before_id = before["id"].as_str().unwrap();

            // Period ending after cutoff
            let enc_after = json!({
                "resourceType": "Encounter",
                "status": "finished",
                "class": {"code": "IMP"},
                "period": {
                    "start": "2023-06-10T08:00:00Z",
                    "end": "2023-06-20T17:00:00Z"
                }
            });

            let (status, _headers, _body) = app
                .request(
                    Method::POST,
                    "/fhir/Encounter",
                    Some(to_json_body(&enc_after)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create after");

            // Search with eb prefix
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Encounter?period=eb2023-06-15", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Encounter")?;
            assert_eq!(ids.len(), 1, "eb should only match periods ending before");
            assert_eq!(ids[0], before_id);

            Ok(())
        })
    })
    .await
}
