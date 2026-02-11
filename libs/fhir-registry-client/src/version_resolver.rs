//! Version resolution logic for FHIR package dependencies
//!
//! Implements FHIR package specification version resolution:
//! - https://confluence.hl7.org/spaces/FHIR/pages/35718629/NPM+Package+Specification
//! - https://build.fhir.org/ig/FHIR/ig-guidance/versions.html
//!
//! This logic is shared with services/package-registry/app/dependency_resolver.py
//! to ensure consistent behavior across the codebase.

use std::cmp::Ordering;

/// Select the best matching version per FHIR package specification.
///
/// Args:
/// - versions: List of available version strings
/// - version_range: Version reference (e.g., "1.5.x", "1.2", "current", "dev", or exact "1.2.3")
///
/// Returns:
/// - Selected version string or None if no match found
///
/// Spec compliance:
/// - Only patch-level wildcards allowed: "1.5.x"
/// - Major.minor format resolves to latest patch: "1.2" -> "1.2.x"
/// - Exact versions: prefer unlabeled (1.2.3) over labeled (1.2.3-ballot)
/// - Special keywords: "current" (CI build), "dev" (local build)
/// - NO caret (^) or asterisk (*) ranges
pub fn select_version(versions: &[String], version_range: Option<&str>) -> Option<String> {
    if versions.is_empty() {
        return None;
    }

    let version_range = version_range.unwrap_or("");

    // Handle missing/empty or "current" keyword
    if version_range.is_empty() || version_range == "current" {
        return select_most_recent_milestone(versions);
    }

    // Handle "dev" keyword (local build fallback to current)
    if version_range == "dev" {
        return select_most_recent_milestone(versions);
    }

    // Handle "latest" keyword
    if version_range == "latest" {
        return select_most_recent_milestone(versions);
    }

    // Handle patch-level x-range (e.g., "1.5.x")
    if version_range.ends_with(".x") {
        return select_x_range(versions, version_range);
    }

    // Handle major.minor format (e.g., "1.2" -> "1.2.x")
    if is_major_minor_only(version_range) {
        return select_x_range(versions, &format!("{}.x", version_range));
    }

    // Handle exact version with label preference
    select_exact_version(versions, version_range)
}

fn select_most_recent_milestone(versions: &[String]) -> Option<String> {
    let sorted = sort_versions_desc(versions);
    // Prefer versions without labels
    let non_labeled: Vec<&String> = sorted.iter().filter(|v| !v.contains('-')).collect();
    non_labeled
        .first()
        .copied()
        .cloned()
        .or_else(|| sorted.first().cloned())
}

fn select_x_range(versions: &[String], version_range: &str) -> Option<String> {
    let prefix = version_range.trim_end_matches(".x");
    let prefix_parts: std::result::Result<Vec<u32>, _> =
        prefix.split('.').map(|n| n.parse()).collect();
    let prefix_parts = match prefix_parts {
        Ok(parts) => parts,
        Err(_) => return None,
    };

    let mut matching_unlabeled = Vec::new();
    let mut matching_labeled = Vec::new();

    for version in versions {
        if !version.starts_with(&format!("{}.", prefix)) {
            continue;
        }

        // Parse version parts
        let base_version = version.split('-').next().unwrap_or(version);
        let version_parts: std::result::Result<Vec<u32>, _> =
            base_version.split('.').map(|n| n.parse()).collect();
        let version_parts = match version_parts {
            Ok(parts) => parts,
            Err(_) => continue,
        };

        // Check prefix matches and has exactly one more part (patch)
        if version_parts.len() != prefix_parts.len() + 1 {
            continue;
        }

        let matches = version_parts
            .iter()
            .zip(prefix_parts.iter())
            .all(|(a, b)| a == b);

        if matches {
            if version.contains('-') {
                matching_labeled.push(version.clone());
            } else {
                matching_unlabeled.push(version.clone());
            }
        }
    }

    // Prefer unlabeled, fallback to labeled
    let candidates = if !matching_unlabeled.is_empty() {
        matching_unlabeled
    } else {
        matching_labeled
    };

    if candidates.is_empty() {
        return None;
    }

    let sorted = sort_versions_asc(&candidates);
    sorted.last().cloned()
}

fn select_exact_version(versions: &[String], version_range: &str) -> Option<String> {
    let mut exact_match = None;
    let mut labeled_match = None;

    for v in versions {
        if v == version_range {
            return Some(v.clone()); // Exact match including label
        }

        // Check if this is a labeled version of the requested version
        if v.contains('-') && v.starts_with(&format!("{}-", version_range)) {
            labeled_match = Some(v.clone());
        }

        // Check if requested version has label but we have unlabeled
        if version_range.contains('-') {
            let unlabeled_request = version_range.split('-').next().unwrap_or(version_range);
            if v == unlabeled_request {
                exact_match = Some(v.clone());
            }
        }
    }

    // Prefer exact unlabeled match, fallback to labeled
    exact_match.or(labeled_match)
}

fn is_major_minor_only(version_str: &str) -> bool {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() != 2 {
        return false;
    }
    parts[0].parse::<u32>().is_ok() && parts[1].parse::<u32>().is_ok()
}

fn sort_versions_asc(versions: &[String]) -> Vec<String> {
    let mut sorted = versions.to_vec();
    sorted.sort_by_key(|a| version_key(a));
    sorted
}

fn sort_versions_desc(versions: &[String]) -> Vec<String> {
    let mut sorted = versions.to_vec();
    sorted.sort_by_key(|a| std::cmp::Reverse(version_key(a)));
    sorted
}

fn version_key(version: &str) -> Vec<VersionPart> {
    version
        .split('.')
        .flat_map(|part| {
            // Split on '-' to separate label
            let (num_part, _label) = if let Some((num, label)) = part.split_once('-') {
                (num, Some(label))
            } else {
                (part, None)
            };

            // Try to parse as number
            if let Ok(num) = num_part.parse::<u32>() {
                vec![VersionPart::Number(num)]
            } else {
                // If not a number, treat as string
                num_part.chars().map(VersionPart::Char).collect::<Vec<_>>()
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionPart {
    Number(u32),
    Char(char),
}

impl Ord for VersionPart {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (VersionPart::Number(a), VersionPart::Number(b)) => a.cmp(b),
            (VersionPart::Number(_), VersionPart::Char(_)) => Ordering::Less,
            (VersionPart::Char(_), VersionPart::Number(_)) => Ordering::Greater,
            (VersionPart::Char(a), VersionPart::Char(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for VersionPart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_most_recent_milestone() {
        let versions = vec![
            "1.0.0".to_string(),
            "1.0.1".to_string(),
            "1.1.0".to_string(),
            "1.1.0-ballot".to_string(),
        ];
        assert_eq!(
            select_most_recent_milestone(&versions),
            Some("1.1.0".to_string())
        );
    }

    #[test]
    fn test_select_x_range() {
        // Should prefer unlabeled versions
        let versions = vec![
            "1.0.0".to_string(),
            "1.0.1".to_string(),
            "1.1.0".to_string(),
            "1.1.1".to_string(),
            "1.1.2-ballot".to_string(),
        ];
        assert_eq!(
            select_version(&versions, Some("1.1.x")),
            Some("1.1.1".to_string()) // Prefers unlabeled over labeled
        );
        // If only labeled versions available, use the highest
        let versions = vec![
            "1.1.0-ballot".to_string(),
            "1.1.1-ballot".to_string(),
            "1.1.2-ballot".to_string(),
        ];
        assert_eq!(
            select_version(&versions, Some("1.1.x")),
            Some("1.1.2-ballot".to_string())
        );
    }

    #[test]
    fn test_select_major_minor() {
        let versions = vec![
            "1.0.0".to_string(),
            "1.0.1".to_string(),
            "1.0.2".to_string(),
        ];
        assert_eq!(
            select_version(&versions, Some("1.0")),
            Some("1.0.2".to_string())
        );
    }

    #[test]
    fn test_select_exact_version() {
        let versions = vec![
            "1.0.0".to_string(),
            "1.0.1".to_string(),
            "1.0.1-ballot".to_string(),
        ];
        assert_eq!(
            select_version(&versions, Some("1.0.1")),
            Some("1.0.1".to_string())
        );
        assert_eq!(
            select_version(&versions, Some("1.0.1-ballot")),
            Some("1.0.1-ballot".to_string())
        );
    }

    #[test]
    fn test_select_current_or_latest() {
        let versions = vec!["1.0.0".to_string(), "1.0.1".to_string()];
        assert_eq!(
            select_version(&versions, Some("current")),
            Some("1.0.1".to_string())
        );
        assert_eq!(
            select_version(&versions, Some("latest")),
            Some("1.0.1".to_string())
        );
        assert_eq!(select_version(&versions, None), Some("1.0.1".to_string()));
    }
}
