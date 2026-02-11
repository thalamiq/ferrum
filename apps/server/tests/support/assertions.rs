use anyhow::Context as _;
use axum::http::StatusCode;
use serde_json::Value;

/// Assert that a response is a valid FHIR Bundle
pub fn assert_bundle(value: &Value) -> anyhow::Result<&Value> {
    assert_eq!(
        value.get("resourceType").and_then(|v| v.as_str()),
        Some("Bundle"),
        "expected Bundle resource type"
    );
    Ok(value)
}

/// Assert that a Bundle has a specific type
pub fn assert_bundle_type<'a>(bundle: &'a Value, bundle_type: &str) -> anyhow::Result<&'a Value> {
    assert_eq!(
        bundle.get("type").and_then(|v| v.as_str()),
        Some(bundle_type),
        "expected Bundle.type = {bundle_type}"
    );
    Ok(bundle)
}

/// Get Bundle entries as array
pub fn get_bundle_entries(bundle: &Value) -> anyhow::Result<&Vec<Value>> {
    bundle
        .get("entry")
        .and_then(|v| v.as_array())
        .context("Bundle.entry is array")
}

/// Assert Bundle has a specific number of entries
pub fn assert_bundle_total(bundle: &Value, expected_count: usize) -> anyhow::Result<()> {
    let entries = get_bundle_entries(bundle)?;
    assert_eq!(
        entries.len(),
        expected_count,
        "expected {expected_count} entries, got {}",
        entries.len()
    );
    Ok(())
}

/// Extract resource IDs from Bundle entries
pub fn extract_resource_ids(bundle: &Value, resource_type: &str) -> anyhow::Result<Vec<String>> {
    let entries = get_bundle_entries(bundle)?;
    let ids = entries
        .iter()
        .filter_map(|e| e.get("resource"))
        .filter(|r| r.get("resourceType").and_then(|v| v.as_str()) == Some(resource_type))
        .filter_map(|r| r.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    Ok(ids)
}

/// Extract resource IDs from Bundle entries with search mode filter
pub fn extract_resource_ids_by_mode(
    bundle: &Value,
    resource_type: &str,
    search_mode: &str,
) -> anyhow::Result<Vec<String>> {
    let entries = get_bundle_entries(bundle)?;
    let ids = entries
        .iter()
        .filter(|e| {
            e.get("search")
                .and_then(|s| s.get("mode"))
                .and_then(|m| m.as_str())
                == Some(search_mode)
        })
        .filter_map(|e| e.get("resource"))
        .filter(|r| r.get("resourceType").and_then(|v| v.as_str()) == Some(resource_type))
        .filter_map(|r| r.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    Ok(ids)
}

/// Assert that a resource has a specific ID
pub fn assert_resource_id(resource: &Value, expected_id: &str) -> anyhow::Result<()> {
    let id = resource
        .get("id")
        .and_then(|v| v.as_str())
        .context("resource has id")?;
    assert_eq!(id, expected_id, "expected resource id = {expected_id}");
    Ok(())
}

/// Assert that a resource has a specific version ID
pub fn assert_version_id(resource: &Value, expected_version: &str) -> anyhow::Result<()> {
    let version = resource
        .get("meta")
        .and_then(|m| m.get("versionId"))
        .and_then(|v| v.as_str())
        .context("resource has meta.versionId")?;
    assert_eq!(
        version, expected_version,
        "expected meta.versionId = {expected_version}"
    );
    Ok(())
}

/// Assert status code matches expected
pub fn assert_status(actual: StatusCode, expected: StatusCode, context: &str) {
    assert_eq!(
        actual, expected,
        "{context}: expected status {expected}, got {actual}"
    );
}

/// Assert status is 2xx success
pub fn assert_success(status: StatusCode, context: &str) {
    assert!(
        status.is_success(),
        "{context}: expected success status, got {status}"
    );
}

/// Assert status is 4xx client error
pub fn assert_client_error(status: StatusCode, context: &str) {
    assert!(
        status.is_client_error(),
        "{context}: expected client error status, got {status}"
    );
}

/// Assert that a Bundle contains a resource with a specific ID
pub fn assert_bundle_contains_id(
    bundle: &Value,
    resource_type: &str,
    expected_id: &str,
) -> anyhow::Result<()> {
    let ids = extract_resource_ids(bundle, resource_type)?;
    assert!(
        ids.contains(&expected_id.to_string()),
        "expected Bundle to contain {resource_type}/{expected_id}, found: {ids:?}"
    );
    Ok(())
}

/// Assert that a Bundle does NOT contain a resource with a specific ID
pub fn assert_bundle_not_contains_id(
    bundle: &Value,
    resource_type: &str,
    unexpected_id: &str,
) -> anyhow::Result<()> {
    let ids = extract_resource_ids(bundle, resource_type)?;
    assert!(
        !ids.contains(&unexpected_id.to_string()),
        "expected Bundle to NOT contain {resource_type}/{unexpected_id}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_assert_bundle() {
        let bundle = json!({
            "resourceType": "Bundle",
            "type": "searchset",
            "entry": []
        });
        assert!(assert_bundle(&bundle).is_ok());
    }

    #[test]
    fn test_extract_resource_ids() {
        let bundle = json!({
            "resourceType": "Bundle",
            "entry": [
                { "resource": { "resourceType": "Patient", "id": "123" } },
                { "resource": { "resourceType": "Patient", "id": "456" } },
                { "resource": { "resourceType": "Observation", "id": "789" } }
            ]
        });

        let patient_ids = extract_resource_ids(&bundle, "Patient").unwrap();
        assert_eq!(patient_ids, vec!["123", "456"]);

        let obs_ids = extract_resource_ids(&bundle, "Observation").unwrap();
        assert_eq!(obs_ids, vec!["789"]);
    }
}
