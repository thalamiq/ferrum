pub(super) fn is_untyped_logical_id_reference(raw: &str) -> bool {
    let s = raw.trim();
    if s.is_empty() {
        return false;
    }
    if s.starts_with('#') {
        return false;
    }
    if s.contains("://") || s.starts_with("urn:") {
        return false;
    }
    if s.contains('|') {
        return false;
    }
    !s.contains('/')
}

pub(super) fn is_valid_fhir_logical_id(value: &str) -> bool {
    // FHIR id: [A-Za-z0-9\\-\\.]{1,64}
    let len = value.len();
    if len == 0 || len > 64 {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}
