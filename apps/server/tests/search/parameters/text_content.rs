// FHIR R4 _text and _content Search Parameter Tests
//
// Spec:
// - spec/search/03-02-01-08-01-13-_text.md
// - spec/search/03-02-01-08-01-01-_content.md
//
// These parameters are defined on base types (DomainResource / Resource) and are expected
// to work across resource types via fallback resolution.

use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn text_search_indexes_narrative_only() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let obs_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "text": {
                    "status": "generated",
                    "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">Narrative summary</div>"
                },
                "note": [{
                    "text": "Patient was anxious"
                }],
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "8310-5",
                        "display": "Body temperature"
                    }]
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
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = created["id"].as_str().unwrap();

            // Index inline (workers are disabled in tests).
            let stored = app
                .state
                .crud_service
                .read_resource("Observation", obs_id)
                .await?;
            app.state.indexing_service.index_resource(&stored).await?;

            // _text searches narrative only.
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?_text=summary", None)
                .await?;
            assert_status(status, StatusCode::OK, "search narrative");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert_eq!(ids, vec![obs_id.to_string()]);

            // Note text is not part of narrative; should not match _text.
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Observation?_text=anxious", None)
                .await?;
            assert_status(status, StatusCode::OK, "search note");
            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Observation")?;
            assert!(ids.is_empty());

            Ok(())
        })
    })
    .await
}

#[tokio::test]
async fn content_search_indexes_well_known_text_fields() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let obs_body = json!({
                "resourceType": "Observation",
                "status": "final",
                "note": [{
                    "text": "Patient was anxious"
                }],
                "code": {
                    "coding": [{
                        "system": "http://loinc.org",
                        "code": "85354-9",
                        "display": "Blood pressure panel"
                    }],
                    "text": "BP panel"
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
            let created: serde_json::Value = serde_json::from_slice(&body)?;
            let obs_id = created["id"].as_str().unwrap();

            // Index inline (workers are disabled in tests).
            let stored = app
                .state
                .crud_service
                .read_resource("Observation", obs_id)
                .await?;
            app.state.indexing_service.index_resource(&stored).await?;

            // _content searches non-narrative text (e.g. note.text) and display/text fields.
            for query in ["anxious", "Blood", "BP"] {
                let url = format!("/fhir/Observation?_content={}", urlencoding::encode(query));
                let (status, _headers, body) = app.request(Method::GET, &url, None).await?;
                assert_status(status, StatusCode::OK, "search content");
                let bundle: serde_json::Value = serde_json::from_slice(&body)?;
                let ids = extract_resource_ids(&bundle, "Observation")?;
                assert_eq!(ids, vec![obs_id.to_string()]);
            }

            Ok(())
        })
    })
    .await
}
