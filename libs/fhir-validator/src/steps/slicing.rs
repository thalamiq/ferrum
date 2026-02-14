//! Slicing validation for repeating elements in FHIR profiles
//!
//! Implements the FHIR slicing mechanism per spec 5.1.0.12-15:
//! - Discriminator-based slice matching (value, exists, type, profile, position)
//! - Slice cardinality validation
//! - Default slice handling for closed slicing

#![allow(dead_code)] // Many functions are stubs for future implementation

use crate::validator::{IssueCode, ValidationIssue};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use ferrum_snapshot::ElementDefinition;
use ferrum_fhirpath::{Context as FhirPathContext, Engine as FhirPathEngine, Value as FhirPathValue};

/// Discriminator type per spec 5.1.0.13
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscriminatorType {
    /// Slices differentiated by value (fixed/pattern/required binding)
    Value,
    /// Slices differentiated by presence/absence of element
    Exists,
    /// Deprecated alias for Value
    Pattern,
    /// Slices differentiated by type of element
    Type,
    /// Slices differentiated by conformance to profile
    Profile,
    /// Slices differentiated by index position
    Position,
}

impl DiscriminatorType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "value" => Some(Self::Value),
            "exists" => Some(Self::Exists),
            "pattern" => Some(Self::Pattern),
            "type" => Some(Self::Type),
            "profile" => Some(Self::Profile),
            "position" => Some(Self::Position),
            _ => None,
        }
    }
}

/// Discriminator definition from ElementDefinition.slicing.discriminator
#[derive(Debug, Clone)]
pub struct Discriminator {
    pub type_: DiscriminatorType,
    /// Restricted FHIRPath expression (element selections, extension(url), resolve(), ofType())
    pub path: String,
}

/// Slicing entry from ElementDefinition.slicing
#[derive(Debug, Clone)]
pub struct SlicingRules {
    pub discriminators: Vec<Discriminator>,
    /// Rules: closed | open | openAtEnd
    pub rules: SlicingRulesKind,
    /// Whether order is significant
    pub ordered: bool,
    /// Description of slicing purpose
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlicingRulesKind {
    /// No additional elements allowed beyond defined slices
    Closed,
    /// Additional elements allowed anywhere
    Open,
    /// Additional elements allowed after defined slices
    OpenAtEnd,
}

impl SlicingRulesKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "closed" => Some(Self::Closed),
            "open" => Some(Self::Open),
            "openAtEnd" => Some(Self::OpenAtEnd),
            _ => None,
        }
    }
}

/// A slice definition with its constraints
#[derive(Debug, Clone)]
pub struct SliceDefinition<'a> {
    /// Slice name from ElementDefinition.sliceName (or "@default" for default slice)
    pub name: String,
    /// Element definition for this slice
    pub element: &'a ElementDefinition,
}

/// Validates slicing for a repeating element
pub fn validate_slicing(
    elements: &[Value],
    element_path: &str,
    slicing_rules: &SlicingRules,
    slices: &[SliceDefinition],
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) {
    // Match each element to slices based on discriminators
    let matches = match_elements_to_slices(
        elements,
        element_path,
        slicing_rules,
        slices,
        fhirpath_engine,
        issues,
    );

    // Validate cardinality for each slice (spec 5.1.0.14)
    validate_slice_cardinalities(&matches, slices, element_path, issues);

    // Validate ordering if required
    if slicing_rules.ordered {
        validate_slice_ordering(&matches, slices, element_path, issues);
    }

    // Check for unmatched elements based on slicing rules
    validate_unmatched_elements(
        &matches,
        elements.len(),
        slicing_rules,
        element_path,
        issues,
    );
}

/// Match each element in the array to slices based on discriminators
fn match_elements_to_slices<'a>(
    elements: &[Value],
    element_path: &str,
    slicing_rules: &SlicingRules,
    slices: &[SliceDefinition<'a>],
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) -> SliceMatches {
    let mut matches = SliceMatches::new();

    for (idx, element) in elements.iter().enumerate() {
        let mut matched = false;

        // Try to match against each slice
        for slice in slices {
            // Skip default slice in initial matching
            if slice.name == "@default" {
                continue;
            }

            if element_matches_slice(
                element,
                slice,
                slicing_rules,
                element_path,
                idx,
                fhirpath_engine,
                issues,
            ) {
                matches.add_match(slice.name.clone(), idx);
                matched = true;
                break; // Per spec: element matches at most one slice (mutually exclusive)
            }
        }

        // If no match and default slice exists, assign to default
        if !matched {
            if let Some(_default_slice) = slices.iter().find(|s| s.name == "@default") {
                matches.add_match("@default".to_string(), idx);
            } else {
                matches.add_unmatched(idx);
            }
        }
    }

    matches
}

/// Check if an element matches a slice based on discriminators
fn element_matches_slice(
    element: &Value,
    slice: &SliceDefinition,
    slicing_rules: &SlicingRules,
    element_path: &str,
    element_idx: usize,
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) -> bool {
    // Position discriminator is special - check index first
    if let Some(_disc) = slicing_rules
        .discriminators
        .iter()
        .find(|d| d.type_ == DiscriminatorType::Position)
    {
        return matches_position_discriminator(element_idx, slice);
    }

    // All discriminators must match (composite match)
    for discriminator in &slicing_rules.discriminators {
        if !matches_discriminator(
            element,
            slice,
            discriminator,
            element_path,
            fhirpath_engine,
            issues,
        ) {
            return false;
        }
    }

    true
}

/// Check position-based discriminator (spec 5.1.0.13)
fn matches_position_discriminator(_element_idx: usize, _slice: &SliceDefinition) -> bool {
    // TODO: Implement position discriminator matching
    // Requires tracking slice order and fixed cardinalities
    // For now, stub returns false
    false
}

/// Check if element matches a discriminator
fn matches_discriminator(
    element: &Value,
    slice: &SliceDefinition,
    discriminator: &Discriminator,
    _element_path: &str,
    fhirpath_engine: &Arc<FhirPathEngine>,
    _issues: &mut Vec<ValidationIssue>,
) -> bool {
    match discriminator.type_ {
        DiscriminatorType::Value | DiscriminatorType::Pattern => {
            matches_value_discriminator(element, slice, &discriminator.path, fhirpath_engine)
        }
        DiscriminatorType::Exists => {
            matches_exists_discriminator(element, slice, &discriminator.path, fhirpath_engine)
        }
        DiscriminatorType::Type => {
            matches_type_discriminator(element, slice, &discriminator.path, fhirpath_engine)
        }
        DiscriminatorType::Profile => {
            matches_profile_discriminator(element, slice, &discriminator.path, fhirpath_engine)
        }
        DiscriminatorType::Position => {
            // Handled separately in element_matches_slice
            true
        }
    }
}

/// Value discriminator: slice must have fixed/pattern value or required binding (spec 5.1.0.13)
fn matches_value_discriminator(
    element: &Value,
    slice: &SliceDefinition,
    discriminator_path: &str,
    fhirpath_engine: &Arc<FhirPathEngine>,
) -> bool {
    // Extract value from element using FHIRPath engine
    let element_value = extract_value_by_fhirpath(element, discriminator_path, fhirpath_engine);

    // Check against slice's fixed value, pattern, or required binding
    if let Some(fixed) = get_fixed_value(slice, discriminator_path) {
        return values_match(&element_value, &fixed);
    }

    if let Some(pattern) = get_pattern_value(slice, discriminator_path) {
        return value_matches_pattern(&element_value, &pattern);
    }

    // TODO: Check required binding enumeration
    // Requires context access to resolve ValueSet
    false
}

/// Exists discriminator: presence/absence of element (spec 5.1.0.13)
fn matches_exists_discriminator(
    element: &Value,
    slice: &SliceDefinition,
    discriminator_path: &str,
    fhirpath_engine: &Arc<FhirPathEngine>,
) -> bool {
    let element_value = extract_value_by_fhirpath(element, discriminator_path, fhirpath_engine);
    let exists = !element_value.is_null();

    // Slice must have min=1+ (exists) or max=0 (not exists)
    let slice_requires_exists = slice.element.min.unwrap_or(0) >= 1;
    let slice_requires_not_exists = slice.element.max.as_deref() == Some("0");

    if slice_requires_exists {
        exists
    } else if slice_requires_not_exists {
        !exists
    } else {
        // Slice doesn't specify clear exists constraint
        true
    }
}

/// Type discriminator: element type matches slice type constraint (spec 5.1.0.13)
fn matches_type_discriminator(
    _element: &Value,
    _slice: &SliceDefinition,
    _discriminator_path: &str,
    _fhirpath_engine: &Arc<FhirPathEngine>,
) -> bool {
    // TODO: Implement type discriminator
    // Requires extracting resourceType or checking polymorphic element type
    // Common for Reference.resolve() and choice types like value[x]
    false
}

/// Profile discriminator: element conforms to specified profile (spec 5.1.0.13)
fn matches_profile_discriminator(
    _element: &Value,
    _slice: &SliceDefinition,
    _discriminator_path: &str,
    _fhirpath_engine: &Arc<FhirPathEngine>,
) -> bool {
    // TODO: Implement profile discriminator
    // Requires full validation against target profile
    // Most powerful but most expensive (>1000x vs value discriminator)
    false
}

/// Extract value from element using FHIRPath engine (spec 5.1.0.13)
///
/// Discriminator paths are restricted FHIRPath expressions supporting:
/// - Element selections (a.b.c)
/// - extension(url)
/// - resolve()
/// - ofType()
fn extract_value_by_fhirpath(element: &Value, path: &str, engine: &Arc<FhirPathEngine>) -> Value {
    if path.is_empty() || path == "$this" {
        return element.clone();
    }

    // Convert serde_json::Value to FHIRPath Value
    let fhirpath_value = FhirPathValue::from_json(element.clone());
    let ctx = FhirPathContext::new(fhirpath_value);

    // Evaluate the FHIRPath expression
    // Note: The engine caches compiled plans internally, so repeated paths are fast
    match engine.evaluate_expr(path, &ctx, None) {
        Ok(collection) => {
            // Convert result collection back to serde_json::Value
            // Take first value from collection (discriminators should return single value)
            if let Some(first) = collection.iter().next() {
                convert_fhirpath_value_to_json(first)
            } else {
                Value::Null
            }
        }
        Err(_) => {
            // If FHIRPath evaluation fails, return null
            // This can happen for invalid paths or complex expressions
            Value::Null
        }
    }
}

/// Convert FHIRPath Value to serde_json::Value for comparison
fn convert_fhirpath_value_to_json(value: &FhirPathValue) -> Value {
    use ferrum_fhirpath::value::ValueData;

    match value.data() {
        ValueData::Boolean(b) => Value::Bool(*b),
        ValueData::Integer(i) => serde_json::json!(i),
        ValueData::Decimal(d) => serde_json::json!(d.to_string()),
        ValueData::String(s) => Value::String(s.to_string()),
        ValueData::Object(map) => {
            let mut obj = serde_json::Map::new();
            for (key, collection) in map.as_ref() {
                // For object fields, take first value from collection
                if let Some(first) = collection.iter().next() {
                    obj.insert(key.to_string(), convert_fhirpath_value_to_json(first));
                }
            }
            Value::Object(obj)
        }
        _ => Value::Null,
    }
}

/// Get fixed value from slice element definition
fn get_fixed_value(_slice: &SliceDefinition, _path: &str) -> Option<Value> {
    // TODO: Look up ElementDefinition for discriminator path within slice
    // Check ElementDefinition.fixed[x] fields
    None
}

/// Get pattern value from slice element definition
fn get_pattern_value(_slice: &SliceDefinition, _path: &str) -> Option<Value> {
    // TODO: Look up ElementDefinition for discriminator path within slice
    // Check ElementDefinition.pattern[x] fields
    None
}

/// Check if two values match exactly
fn values_match(a: &Value, b: &Value) -> bool {
    a == b
}

/// Check if value matches pattern (partial match for objects)
fn value_matches_pattern(value: &Value, pattern: &Value) -> bool {
    match (value, pattern) {
        (Value::Object(val_obj), Value::Object(pat_obj)) => {
            // Pattern match: all pattern fields must match
            for (key, pat_val) in pat_obj {
                match val_obj.get(key) {
                    Some(val_val) if value_matches_pattern(val_val, pat_val) => continue,
                    _ => return false,
                }
            }
            true
        }
        (Value::Array(val_arr), Value::Array(pat_arr)) => {
            // Arrays must match element-wise
            if val_arr.len() != pat_arr.len() {
                return false;
            }
            val_arr
                .iter()
                .zip(pat_arr.iter())
                .all(|(v, p)| value_matches_pattern(v, p))
        }
        _ => value == pattern,
    }
}

/// Validate cardinality constraints for each slice (spec 5.1.0.14)
fn validate_slice_cardinalities(
    matches: &SliceMatches,
    slices: &[SliceDefinition],
    element_path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    // Get base element cardinality (sum of minimums constraint from spec 5.1.0.14)
    let base_min = slices.first().and_then(|s| s.element.min).unwrap_or(0) as u64;
    let base_max = slices
        .first()
        .and_then(|s| s.element.max.as_deref())
        .unwrap_or("*");

    let total_elements = matches.total_matched() + matches.unmatched.len();

    // Check base cardinality
    if (total_elements as u64) < base_min {
        issues.push(
            ValidationIssue::error(
                IssueCode::Required,
                format!(
                    "Element '{}' requires at least {} occurrences, found {}",
                    element_path, base_min, total_elements
                ),
            )
            .with_location(element_path.to_string()),
        );
    }

    if base_max != "*" {
        if let Ok(max_num) = base_max.parse::<u64>() {
            if (total_elements as u64) > max_num {
                issues.push(
                    ValidationIssue::error(
                        IssueCode::Structure,
                        format!(
                            "Element '{}' allows at most {} occurrences, found {}",
                            element_path, max_num, total_elements
                        ),
                    )
                    .with_location(element_path.to_string()),
                );
            }
        }
    }

    // Check each slice's cardinality
    for slice in slices {
        let count = matches.count_for_slice(&slice.name);
        let min = slice.element.min.unwrap_or(0) as u64;
        let max = slice.element.max.as_deref().unwrap_or("*");

        // Per spec 5.1.0.14: individual slice can have min=0 even if base min > 0
        if count < min {
            issues.push(
                ValidationIssue::error(
                    IssueCode::Required,
                    format!(
                        "Slice '{}' requires at least {} occurrences, found {}",
                        slice.name, min, count
                    ),
                )
                .with_location(format!("{}:{}", element_path, slice.name)),
            );
        }

        // Per spec 5.1.0.14: slice max cannot exceed base max
        if max != "*" {
            if let Ok(max_num) = max.parse::<u64>() {
                if count > max_num {
                    issues.push(
                        ValidationIssue::error(
                            IssueCode::Structure,
                            format!(
                                "Slice '{}' allows at most {} occurrences, found {}",
                                slice.name, max_num, count
                            ),
                        )
                        .with_location(format!("{}:{}", element_path, slice.name)),
                    );
                }
            }
        }
    }

    // Validate sum of minimums constraint (spec 5.1.0.14)
    let sum_of_mins: u64 = slices
        .iter()
        .filter(|s| s.name != "@default")
        .map(|s| s.element.min.unwrap_or(0) as u64)
        .sum();

    if sum_of_mins > base_min {
        issues.push(
            ValidationIssue::warning(
                IssueCode::Structure,
                format!(
                    "Sum of slice minimums ({}) exceeds base minimum ({}) for '{}'",
                    sum_of_mins, base_min, element_path
                ),
            )
            .with_location(element_path.to_string()),
        );
    }
}

/// Validate slice ordering if required
fn validate_slice_ordering(
    _matches: &SliceMatches,
    _slices: &[SliceDefinition],
    _element_path: &str,
    _issues: &mut Vec<ValidationIssue>,
) {
    // TODO: Implement ordered slicing validation
    // Elements must appear in the order slices are defined
}

/// Validate unmatched elements based on slicing rules
fn validate_unmatched_elements(
    matches: &SliceMatches,
    _total_elements: usize,
    slicing_rules: &SlicingRules,
    element_path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if matches.unmatched.is_empty() {
        return;
    }

    match slicing_rules.rules {
        SlicingRulesKind::Closed => {
            // No unmatched elements allowed (unless caught by default slice)
            for idx in &matches.unmatched {
                issues.push(
                    ValidationIssue::error(
                        IssueCode::Structure,
                        format!(
                            "Element at index {} does not match any defined slice (slicing is closed)",
                            idx
                        ),
                    )
                    .with_location(format!("{}[{}]", element_path, idx)),
                );
            }
        }
        SlicingRulesKind::Open => {
            // Unmatched elements allowed anywhere - no error
        }
        SlicingRulesKind::OpenAtEnd => {
            // TODO: Check unmatched elements only appear after all matched elements
        }
    }
}

/// Tracks which elements matched which slices
#[derive(Debug)]
struct SliceMatches {
    /// Map from slice name to element indices
    matches: HashMap<String, Vec<usize>>,
    /// Indices of elements that didn't match any slice
    unmatched: Vec<usize>,
}

impl SliceMatches {
    fn new() -> Self {
        Self {
            matches: HashMap::new(),
            unmatched: Vec::new(),
        }
    }

    fn add_match(&mut self, slice_name: String, element_idx: usize) {
        self.matches
            .entry(slice_name)
            .or_default()
            .push(element_idx);
    }

    fn add_unmatched(&mut self, element_idx: usize) {
        self.unmatched.push(element_idx);
    }

    fn count_for_slice(&self, slice_name: &str) -> u64 {
        self.matches
            .get(slice_name)
            .map(|v| v.len() as u64)
            .unwrap_or(0)
    }

    fn total_matched(&self) -> usize {
        self.matches.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_matches_pattern() {
        // Exact match
        assert!(values_match(
            &serde_json::json!("test"),
            &serde_json::json!("test")
        ));

        // Object pattern match
        let value = serde_json::json!({"code": "123", "system": "http://test", "extra": "data"});
        let pattern = serde_json::json!({"code": "123", "system": "http://test"});
        assert!(value_matches_pattern(&value, &pattern));

        // Pattern mismatch
        let pattern = serde_json::json!({"code": "456"});
        assert!(!value_matches_pattern(&value, &pattern));
    }
}
