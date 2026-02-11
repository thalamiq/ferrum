// FHIR R4 URI Search Parameter Tests
//
// Spec: spec/search/03-02-01-05-15-uri.md
//
// URI search tests cover:
// - Exact matching (case and accent sensitive)
// - :above modifier (hierarchical matching - resources where URI starts with the param value)
// - :below modifier (hierarchical matching - resources where param value starts with URI)

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn uri_search_exact_match() -> anyhow::Result<()> {
    // Spec: URI search matches exact URI values (case and accent sensitive)
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "url",
                "ValueSet",
                "uri",
                "ValueSet.url",
                &["above", "below"],
            )
            .await?;

            // Create ValueSets with different URIs
            let vs1_body = json!({
                "resourceType": "ValueSet",
                "status": "active",
                "url": "http://example.org/fhir/ValueSet/example"
            });

            let vs2_body = json!({
                "resourceType": "ValueSet",
                "status": "active",
                "url": "http://example.org/fhir/ValueSet/other"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/ValueSet",
                    Some(to_json_body(&vs1_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create vs1");
            let vs1_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let vs1_id = vs1_resource["id"].as_str().unwrap();

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/ValueSet",
                    Some(to_json_body(&vs2_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create vs2");
            let _vs2_resource: serde_json::Value = serde_json::from_slice(&body)?;

            // Search for exact URI - should only match vs1
            let url = format!(
                "/fhir/ValueSet?url={}",
                urlencoding::encode("http://example.org/fhir/ValueSet/example")
            );
            let (status, _headers, body) = app.request(Method::GET, &url, None).await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "ValueSet")?;
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], vs1_id);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn uri_search_case_sensitive() -> anyhow::Result<()> {
    // Spec: URI matching is case-sensitive
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "url",
                "ValueSet",
                "uri",
                "ValueSet.url",
                &[],
            )
            .await?;

            // Create ValueSet with specific casing
            let vs_body = json!({
                "resourceType": "ValueSet",
                "status": "active",
                "url": "http://Example.Org/FHIR/ValueSet/Test"
            });

            let (status, _headers, body) = app
                .request(
                    Method::POST,
                    "/fhir/ValueSet",
                    Some(to_json_body(&vs_body)?),
                )
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            let vs_resource: serde_json::Value = serde_json::from_slice(&body)?;
            let vs_id = vs_resource["id"].as_str().unwrap();

            // Search with exact casing - should match
            let url = format!(
                "/fhir/ValueSet?url={}",
                urlencoding::encode("http://Example.Org/FHIR/ValueSet/Test")
            );
            let (status, _headers, body) = app.request(Method::GET, &url, None).await?;
            assert_status(status, StatusCode::OK, "exact case search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "ValueSet")?;
            assert_eq!(ids.len(), 1);

            // Search with different casing - should NOT match
            let url_lower = format!(
                "/fhir/ValueSet?url={}",
                urlencoding::encode("http://example.org/fhir/valueset/test")
            );
            let (status, _headers, body) = app.request(Method::GET, &url_lower, None).await?;
            assert_status(status, StatusCode::OK, "different case search");
            let bundle_lower: serde_json::Value = serde_json::from_slice(&body)?;
            let ids_lower = extract_resource_ids(&bundle_lower, "ValueSet")?;
            assert_eq!(ids_lower.len(), 0);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn uri_search_multiple_values_or_logic() -> anyhow::Result<()> {
    // Spec: Multiple URI params use OR logic
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "url",
                "ValueSet",
                "uri",
                "ValueSet.url",
                &[],
            )
            .await?;

            let uris = vec![
                ("vs1", "http://example.org/fhir/ValueSet/one"),
                ("vs2", "http://example.org/fhir/ValueSet/two"),
                ("vs3", "http://example.org/fhir/ValueSet/three"),
            ];

            let mut ids = Vec::new();

            for (_, uri) in &uris {
                let body = json!({
                    "resourceType": "ValueSet",
                    "status": "active",
                    "url": uri
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/ValueSet", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for two specific URIs - should match 2 ValueSets
            // Spec: Multiple values use OR logic with comma separation
            let url = format!(
                "/fhir/ValueSet?url={},{}",
                urlencoding::encode("http://example.org/fhir/ValueSet/one"),
                urlencoding::encode("http://example.org/fhir/ValueSet/three")
            );
            let (status, _headers, body) = app.request(Method::GET, &url, None).await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "ValueSet")?;
            assert_eq!(ids.len(), 2);

            Ok(())
        })
    })
    .await
}

#[tokio::test]
#[ignore = "Server :above modifier for URI search not yet implemented"]
async fn uri_search_above_modifier() -> anyhow::Result<()> {
    // Spec: :above modifier matches URIs that are "above" (ancestors of) the search value
    // url:above=http://example.org/fhir matches http://example.org and http://example.org/fhir
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "url",
                "ValueSet",
                "uri",
                "ValueSet.url",
                &["above"],
            )
            .await?;

            let uris = vec![
                ("base", "http://example.org"),
                ("fhir", "http://example.org/fhir"),
                ("valueset", "http://example.org/fhir/ValueSet/example"),
                ("other", "http://other.org/fhir/ValueSet/example"),
            ];

            let mut ids = Vec::new();

            for (_, uri) in &uris {
                let body = json!({
                    "resourceType": "ValueSet",
                    "status": "active",
                    "url": uri
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/ValueSet", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for URIs above http://example.org/fhir/ValueSet/example
            // Should match: http://example.org and http://example.org/fhir
            let url = format!(
                "/fhir/ValueSet?url:above={}",
                urlencoding::encode("http://example.org/fhir/ValueSet/example")
            );
            let (status, _headers, body) = app.request(Method::GET, &url, None).await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(bundle["total"], 2); // base + fhir

            Ok(())
        })
    })
    .await
}

#[tokio::test]
#[ignore = "Server :below modifier for URI search not yet implemented"]
async fn uri_search_below_modifier() -> anyhow::Result<()> {
    // Spec: :below modifier matches URIs that are "below" (descendants of) the search value
    // url:below=http://example.org/fhir matches http://example.org/fhir/ValueSet/example
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "url",
                "ValueSet",
                "uri",
                "ValueSet.url",
                &["below"],
            )
            .await?;

            let uris = vec![
                ("base", "http://example.org"),
                ("fhir", "http://example.org/fhir"),
                ("valueset", "http://example.org/fhir/ValueSet/example"),
                ("other", "http://other.org/fhir/ValueSet/example"),
            ];

            let mut ids = Vec::new();

            for (_, uri) in &uris {
                let body = json!({
                    "resourceType": "ValueSet",
                    "status": "active",
                    "url": uri
                });
                let (status, _headers, resp_body) = app
                    .request(Method::POST, "/fhir/ValueSet", Some(to_json_body(&body)?))
                    .await?;
                assert_status(status, StatusCode::CREATED, "create");
                let resource: serde_json::Value = serde_json::from_slice(&resp_body)?;
                ids.push(resource["id"].as_str().unwrap().to_string());
            }

            // Search for URIs below http://example.org/fhir
            // Should match: http://example.org/fhir/ValueSet/example
            let url = format!(
                "/fhir/ValueSet?url:below={}",
                urlencoding::encode("http://example.org/fhir")
            );
            let (status, _headers, body) = app.request(Method::GET, &url, None).await?;

            assert_status(status, StatusCode::OK, "search");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            assert_eq!(bundle["total"], 1); // valueset only

            Ok(())
        })
    })
    .await
}
