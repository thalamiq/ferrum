// FHIR R4 Quantity Search Parameter Tests
//
// Spec: spec/search/03-02-01-05-11-quantity.md
//
// Quantity search tests cover:
// - Value-only search
// - System|code|value search
// - Prefix operators (eq, ne, gt, ge, lt, le)
// - Canonical form matching

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

const UCUM: &str = "http://unitsofmeasure.org";

#[tokio::test]
async fn quantity_search_value_only() -> anyhow::Result<()> {
    // Spec: Searching by value only (e.g., value-quantity=5.4) should match
    // any quantity with that value regardless of units
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            // Create observation with temperature in Celsius
            let obs_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "8310-5",
                        "display": "Body temperature"
                    }]
                },
                "valueQuantity": {
                    "value": 37.5,
                    "unit": "degrees C",
                    "system": UCUM,
                    "code": "Cel"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let obs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = obs_resource["id"].as_str().unwrap();

            // Search by value only - should match
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value-quantity=37.5", None)
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

#[tokio::test]
async fn quantity_search_with_system_and_code() -> anyhow::Result<()> {
    // Spec: Search with system|code|value (e.g., value-quantity=5.4|http://unitsofmeasure.org|mg)
    // should match only quantities with that system, code, and value
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            // Create observation with weight in kg
            let obs_kg_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "29463-7",
                        "display": "Body weight"
                    }]
                },
                "valueQuantity": {
                    "value": 70.5,
                    "unit": "kg",
                    "system": UCUM,
                    "code": "kg"
                }
            });

            // Create observation with weight in pounds (same value, different unit)
            let obs_lb_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "29463-7",
                        "display": "Body weight"
                    }]
                },
                "valueQuantity": {
                    "value": 70.5,
                    "unit": "pounds",
                    "system": UCUM,
                    "code": "[lb_av]"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_kg_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create kg");
            let obs_kg_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_kg_id = obs_kg_resource["id"].as_str().unwrap();

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&obs_lb_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create lb");
            let obs_lb_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_lb_id = obs_lb_resource["id"].as_str().unwrap();

            // Search for kg specifically - should only match the kg observation
            let url = format!(
                "/fhir/Observation?value-quantity=70.5|{}|kg",
                urlencoding::encode(UCUM)
            );
            let (status, _headers, body) = app.request(Method::GET, &url, None).await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], obs_kg_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn quantity_search_greater_than() -> anyhow::Result<()> {
    // Spec: gt prefix - greater than the specified value
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            // Create observations with different glucose values
            let low_glucose = 90.0;
            let high_glucose = 180.0;

            let low_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "15074-8",
                        "display": "Glucose"
                    }]
                },
                "valueQuantity": {
                    "value": low_glucose,
                    "system": UCUM,
                    "code": "mg/dL",
                    "unit": "mg/dL"
                }
            });

            let high_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "15074-8",
                        "display": "Glucose"
                    }]
                },
                "valueQuantity": {
                    "value": high_glucose,
                    "system": UCUM,
                    "code": "mg/dL",
                    "unit": "mg/dL"
                }
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&low_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create low");
            let low_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let low_id = low_resource["id"].as_str().unwrap();

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/Observation",
                    Some(to_json_body(&high_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create high");
            let high_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let high_id = high_resource["id"].as_str().unwrap();

            // Search for glucose > 120 - should only match high glucose
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value-quantity=gt120", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], high_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn quantity_search_less_than() -> anyhow::Result<()> {
    // Spec: lt prefix - less than the specified value
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            // Create observations with different blood pressure values
            let normal_bp = 120.0;
            let high_bp = 160.0;

            let normal_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"coding": [{"system": "http://loinc.org", "code": "8480-6"}]},
                "valueQuantity": { "value": normal_bp, "system": UCUM, "code": "mm[Hg]", "unit": "mmHg" }
            });
            let high_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"coding": [{"system": "http://loinc.org", "code": "8480-6"}]},
                "valueQuantity": { "value": high_bp, "system": UCUM, "code": "mm[Hg]", "unit": "mmHg" }
            });

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&normal_body)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create normal");
            let normal_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let normal_id = normal_resource["id"].as_str().unwrap();

            let (status, _headers, body) = app
                .request(Method::POST, "/fhir/Observation", Some(to_json_body(&high_body)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create high");
            let high_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let high_id = high_resource["id"].as_str().unwrap();

            // Search for BP < 140 - should only match normal BP
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value-quantity=lt140", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], normal_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn quantity_search_greater_or_equal() -> anyhow::Result<()> {
    // Spec: ge prefix - greater than or equal to the specified value
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            let values = vec![
                ("low", 95.0),
                ("target", 100.0),
                ("high", 105.0),
            ];

            let mut ids = Vec::new();

            for (_label, value) in &values {
                let body = json!({
                    "resourceType": "Observation",
                    "status": "final",
                    "code": {"coding": [{"system": "http://loinc.org", "code": "test"}]},
                    "valueQuantity": { "value": value, "system": UCUM, "code": "mg/dL", "unit": "mg/dL" }
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/Observation", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for >= 100 - should match target and high
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value-quantity=ge100", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 2);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn quantity_search_less_or_equal() -> anyhow::Result<()> {
    // Spec: le prefix - less than or equal to the specified value
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            let values = vec![
                ("low", 95.0),
                ("target", 100.0),
                ("high", 105.0),
            ];

            let mut ids = Vec::new();

            for (_label, value) in &values {
                let body = json!({
                    "resourceType": "Observation",
                    "status": "final",
                    "code": {"coding": [{"system": "http://loinc.org", "code": "test"}]},
                    "valueQuantity": { "value": value, "system": UCUM, "code": "mg/dL", "unit": "mg/dL" }
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/Observation", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for <= 100 - should match low and target
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value-quantity=le100", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 2);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn quantity_search_not_equal() -> anyhow::Result<()> {
    // Spec: ne prefix - not equal to the specified value
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            let values = vec![98.0, 100.0, 102.0];
            let mut ids = Vec::new();

            for value in &values {
                let body = json!({
                    "resourceType": "Observation",
                    "status": "final",
                    "code": {"coding": [{"system": "http://loinc.org", "code": "test"}]},
                    "valueQuantity": { "value": value, "system": UCUM, "code": "mg/dL", "unit": "mg/dL" }
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/Observation", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for != 100 - should match 98 and 102
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?value-quantity=ne100", None)
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 2);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn quantity_search_range_with_multiple_params() -> anyhow::Result<()> {
    // Spec: Multiple quantity params can combine to create a range (e.g., value-quantity=ge70&value-quantity=le90)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "value-quantity",
                "Observation",
                "quantity",
                "Observation.value.ofType(Quantity)",
                &[],
            )
            .await?;

            let values = vec![60.0, 75.0, 85.0, 95.0];
            let mut ids = Vec::new();

            for value in &values {
                let body = json!({
                    "resourceType": "Observation",
                    "status": "final",
                    "code": {"coding": [{"system": "http://loinc.org", "code": "test"}]},
                    "valueQuantity": { "value": value, "system": UCUM, "code": "mg/dL", "unit": "mg/dL" }
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/Observation", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for values between 70 and 90 (inclusive) - should match 75 and 85
            let (status, _headers, body) = app
                .request(
                    Method::GET,
                    "/fhir/Observation?value-quantity=ge70&value-quantity=le90",
                    None,
                )
                .await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids.len(), 2);

            Ok(())
        })
    })
    .await
}
