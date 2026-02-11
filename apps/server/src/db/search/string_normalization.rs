use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// Normalize a FHIR string search value per FHIR search rules:
/// - case-insensitive
/// - accent/diacritic-insensitive (strip combining marks)
/// - ignore punctuation and non-significant whitespace
///
/// This implementation lowercases, decomposes (NFKD), removes combining marks,
/// and retains only alphanumeric characters.
pub fn normalize_string_for_search(input: &str) -> String {
    input
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Normalize for case-insensitive, combining-character insensitive substring search
/// while preserving non-combining characters (used for `uri:contains`).
pub fn normalize_casefold_strip_combining(input: &str) -> String {
    input
        .trim()
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_case_diacritics_punctuation_and_whitespace() {
        assert_eq!(normalize_string_for_search("Eve"), "eve");
        assert_eq!(normalize_string_for_search("Évê"), "eve");
        assert_eq!(normalize_string_for_search("  E\tv e  "), "eve");
        assert_eq!(
            normalize_string_for_search("Carreno Quinones"),
            "carrenoquinones"
        );
        assert_eq!(
            normalize_string_for_search("Carreno-Quinones"),
            "carrenoquinones"
        );
    }

    #[test]
    fn normalize_casefold_strip_combining_preserves_punctuation() {
        assert_eq!(
            normalize_casefold_strip_combining("Éxample.Org/FHÍR"),
            "example.org/fhir"
        );
        assert_eq!(
            normalize_casefold_strip_combining("urn:oid:1.2.3.4"),
            "urn:oid:1.2.3.4"
        );
    }
}
