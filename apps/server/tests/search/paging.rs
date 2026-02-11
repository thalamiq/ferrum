use crate::support::*;
use anyhow::Context as _;
use axum::http::{Method, StatusCode};
use serde_json::Value;
use url::Url;

fn link_url(bundle: &Value, relation: &str) -> Option<String> {
    bundle
        .get("link")
        .and_then(|v| v.as_array())
        .and_then(|links| {
            links.iter().find_map(|link| {
                let rel = link.get("relation").and_then(|v| v.as_str())?;
                if rel == relation {
                    link.get("url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
}

fn path_and_query(url: &str) -> anyhow::Result<String> {
    let parsed = Url::parse(url).context("parse paging link URL")?;
    let mut out = parsed.path().to_string();
    if let Some(query) = parsed.query() {
        out.push('?');
        out.push_str(query);
    }
    Ok(out)
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.to_string())
}

async fn create_patient(app: &TestApp, family: &str) -> anyhow::Result<String> {
    let patient = PatientBuilder::new().family(family).build();
    let (status, _headers, body) = app
        .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
        .await?;
    assert_status(status, StatusCode::CREATED, "create patient");
    let created: Value = serde_json::from_slice(&body)?;
    created["id"]
        .as_str()
        .map(|s| s.to_string())
        .context("created patient id")
}

#[tokio::test]
async fn paging_links_include_prev_first_last() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            create_patient(app, "Alpha").await?;
            create_patient(app, "Beta").await?;
            create_patient(app, "Gamma").await?;

            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?_count=1", None)
                .await?;
            assert_status(status, StatusCode::OK, "initial page");

            let bundle: Value = serde_json::from_slice(&body)?;
            assert_bundle_type(&bundle, "searchset")?;

            let initial_ids = extract_resource_ids_by_mode(&bundle, "Patient", "match")?;
            let initial_id = initial_ids.first().cloned().context("initial page id")?;

            let next_url = link_url(&bundle, "next").context("initial next link")?;
            let last_url = link_url(&bundle, "last").context("initial last link")?;
            assert!(link_url(&bundle, "prev").is_none(), "initial prev link");
            assert!(link_url(&bundle, "first").is_none(), "initial first link");
            assert_eq!(
                query_param(&last_url, "_cursor_direction").as_deref(),
                Some("last"),
                "last link direction"
            );

            let next_path = path_and_query(&next_url)?;
            let (status, _headers, body) = app.request(Method::GET, &next_path, None).await?;
            assert_status(status, StatusCode::OK, "next page");
            let bundle: Value = serde_json::from_slice(&body)?;

            let prev_url = link_url(&bundle, "prev").context("next prev link")?;
            let first_url = link_url(&bundle, "first").context("next first link")?;
            let last_url = link_url(&bundle, "last").context("next last link")?;
            assert!(
                link_url(&bundle, "next").is_some(),
                "next link on middle page"
            );
            assert_eq!(
                query_param(&prev_url, "_cursor_direction").as_deref(),
                Some("prev"),
                "prev link direction"
            );
            assert!(query_param(&first_url, "_cursor_direction").is_none());
            assert_eq!(
                query_param(&last_url, "_cursor_direction").as_deref(),
                Some("last"),
                "last link direction"
            );

            let prev_path = path_and_query(&prev_url)?;
            let (status, _headers, body) = app.request(Method::GET, &prev_path, None).await?;
            assert_status(status, StatusCode::OK, "prev page");
            let bundle: Value = serde_json::from_slice(&body)?;
            let prev_ids = extract_resource_ids_by_mode(&bundle, "Patient", "match")?;
            assert_eq!(
                prev_ids.first().map(String::as_str),
                Some(initial_id.as_str()),
                "prev returns initial page"
            );

            let last_path = path_and_query(&last_url)?;
            let (status, _headers, body) = app.request(Method::GET, &last_path, None).await?;
            assert_status(status, StatusCode::OK, "last page");
            let bundle: Value = serde_json::from_slice(&body)?;
            assert!(link_url(&bundle, "next").is_none(), "last page next link");
            assert!(link_url(&bundle, "last").is_none(), "last page last link");
            assert!(link_url(&bundle, "prev").is_some(), "last page prev link");
            assert!(link_url(&bundle, "first").is_some(), "last page first link");

            Ok(())
        })
    })
    .await
}
