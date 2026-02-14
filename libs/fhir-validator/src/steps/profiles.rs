//! Profile validation against constraining StructureDefinitions
//!
//! Validates resources against profiles (StructureDefinitions with derivation=constraint):
//! - Element cardinality constraints
//! - Fixed values and patterns
//! - Slicing (delegated to slicing module)
//! - Type restrictions
//! - Must Support elements

use crate::validator::{IssueCode, ValidationIssue};
use crate::ProfilesPlan;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use ferrum_context::FhirContext;
use ferrum_models::common::element_definition::{
    DiscriminatorType as ModelDiscType, SlicingRules as ModelSlicingRules,
};
use ferrum_snapshot::{ElementDefinition, ExpandedFhirContext};
use ferrum_fhirpath::Engine as FhirPathEngine;

use super::slicing::{validate_slicing, SliceDefinition, SlicingRules};

/// Validates a resource against profiles declared in meta.profile or explicit profiles
pub fn validate_profiles<C: FhirContext>(
    resource: &Value,
    plan: &ProfilesPlan,
    context: &C,
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) {
    // Extract resourceType
    let resource_type = match get_resource_type(resource) {
        Some(rt) => rt,
        None => {
            // Schema validation should have caught this, but be defensive
            return;
        }
    };

    // Resolve profile URLs
    let profile_urls = resolve_profile_urls(resource, plan);

    if profile_urls.is_empty() {
        // No profiles to validate against - this is valid (profiles are optional)
        return;
    }

    // Validate against each profile
    for profile_url in &profile_urls {
        validate_against_profile(
            resource,
            &resource_type,
            profile_url,
            context,
            fhirpath_engine,
            issues,
        );
    }
}

/// Resolves profile URLs from explicit_profiles or meta.profile
fn resolve_profile_urls(resource: &Value, plan: &ProfilesPlan) -> Vec<String> {
    // 1. If explicit_profiles is provided, use those
    if let Some(ref explicit) = plan.explicit_profiles {
        return explicit.clone();
    }

    // 2. Otherwise, extract from meta.profile
    let mut profiles = Vec::new();
    if let Some(meta_profiles) = resource
        .get("meta")
        .and_then(|m| m.get("profile"))
        .and_then(|p| p.as_array())
    {
        for profile_value in meta_profiles {
            if let Some(profile_url) = profile_value.as_str() {
                profiles.push(profile_url.to_string());
            }
        }
    }

    profiles
}

/// Validates resource against a single profile StructureDefinition
fn validate_against_profile<C: FhirContext>(
    resource: &Value,
    resource_type: &str,
    profile_url: &str,
    context: &C,
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) {
    // Get StructureDefinition for this profile
    let structure_def = match context.get_structure_definition(profile_url) {
        Ok(Some(sd)) => sd,
        Ok(None) => {
            issues.push(
                ValidationIssue::error(
                    IssueCode::NotFound,
                    format!("Profile StructureDefinition not found: '{}'", profile_url),
                )
                .with_location(format!("{}.meta.profile", resource_type)),
            );
            return;
        }
        Err(e) => {
            issues.push(ValidationIssue::error(
                IssueCode::Exception,
                format!("Error loading profile '{}': {}", profile_url, e),
            ));
            return;
        }
    };

    // Ensure profile matches resourceType
    if structure_def.type_ != resource_type {
        issues.push(
            ValidationIssue::error(
                IssueCode::Invalid,
                format!(
                    "Profile '{}' is for type '{}' but resourceType is '{}'",
                    profile_url, structure_def.type_, resource_type
                ),
            )
            .with_location(format!("{}.resourceType", resource_type)),
        );
        return;
    }

    // Ensure profile is a constraining profile (derivation=constraint)
    use ferrum_models::common::structure_definition::TypeDerivationRule;
    if structure_def.derivation != Some(TypeDerivationRule::Constraint) {
        let derivation_str = structure_def
            .derivation
            .as_ref()
            .map(|d| format!("{:?}", d))
            .unwrap_or_else(|| "none".to_string());
        issues.push(
            ValidationIssue::warning(
                IssueCode::Invalid,
                format!(
                    "Profile '{}' has derivation '{}', expected 'Constraint'",
                    profile_url, derivation_str
                ),
            )
            .with_location(format!("{}.meta.profile", resource_type)),
        );
    }

    // Expand snapshot if needed (reuse logic from schema.rs)
    let structure_def = {
        let needs_expansion = match structure_def.snapshot.as_ref() {
            None => true,
            Some(snapshot) => {
                let index = ProfileElementIndex::new(&snapshot.element);
                snapshot_needs_expansion(resource, resource_type, &index)
            }
        };

        if needs_expansion {
            let expanded_context = ExpandedFhirContext::borrowed(context);
            match expanded_context.get_structure_definition(profile_url) {
                Ok(Some(sd)) => sd,
                Ok(None) => {
                    issues.push(
                        ValidationIssue::error(
                            IssueCode::NotFound,
                            format!("Profile StructureDefinition not found: '{}'", profile_url),
                        )
                        .with_location(format!("{}.meta.profile", resource_type)),
                    );
                    return;
                }
                Err(e) => {
                    issues.push(ValidationIssue::error(
                        IssueCode::Exception,
                        format!("Error expanding profile '{}': {}", profile_url, e),
                    ));
                    return;
                }
            }
        } else {
            structure_def
        }
    };

    let Some(snapshot) = structure_def.snapshot.as_ref() else {
        issues.push(
            ValidationIssue::error(
                IssueCode::Exception,
                format!("Profile '{}' has no snapshot", profile_url),
            )
            .with_location(format!("{}.meta.profile", resource_type)),
        );
        return;
    };

    // Build element index (including slices)
    let index = ProfileElementIndex::new(&snapshot.element);

    // Validate against profile constraints
    validate_profile_object(resource, resource_type, &index, fhirpath_engine, issues);
}

/// Element index for profile validation (includes slices)
struct ProfileElementIndex<'a> {
    by_path: HashMap<&'a str, &'a ElementDefinition>,
    children_by_parent: HashMap<&'a str, Vec<&'a ElementDefinition>>,
    slicing_by_path: HashMap<&'a str, &'a ElementDefinition>, // Elements with slicing
}

impl<'a> ProfileElementIndex<'a> {
    fn new(elements: &'a [ElementDefinition]) -> Self {
        let mut by_path = HashMap::new();
        let mut children_by_parent: HashMap<&'a str, Vec<&'a ElementDefinition>> = HashMap::new();
        let mut slicing_by_path = HashMap::new();

        for element in elements {
            // Store all elements (including slices) by path
            by_path.insert(element.path.as_str(), element);

            // Track elements with slicing
            if element.slicing.is_some() {
                slicing_by_path.insert(element.path.as_str(), element);
            }

            // Build parent-child relationships (skip slices for this)
            if !element.path.contains(':') {
                let Some((parent, _name)) = element.path.rsplit_once('.') else {
                    continue;
                };
                children_by_parent.entry(parent).or_default().push(element);
            }
        }

        Self {
            by_path,
            children_by_parent,
            slicing_by_path,
        }
    }

    fn get_element(&self, path: &str) -> Option<&'a ElementDefinition> {
        self.by_path.get(path).copied()
    }

    fn children_of(&self, parent_path: &str) -> &[&'a ElementDefinition] {
        self.children_by_parent
            .get(parent_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn has_slicing(&self, path: &str) -> bool {
        self.slicing_by_path.contains_key(path)
    }

    fn get_slicing_element(&self, path: &str) -> Option<&'a ElementDefinition> {
        self.slicing_by_path.get(path).copied()
    }
}

/// Check if snapshot needs expansion (reused from schema.rs)
fn snapshot_needs_expansion(
    resource: &Value,
    root_path: &str,
    index: &ProfileElementIndex<'_>,
) -> bool {
    fn has_non_special_keys(obj: &serde_json::Map<String, Value>) -> bool {
        obj.keys().any(|k| {
            !is_special_element_key(k)
                && !k.starts_with('_')
                && k.as_str() != "extension"
                && k.as_str() != "modifierExtension"
        })
    }

    fn visit(value: &Value, path: &str, index: &ProfileElementIndex<'_>) -> bool {
        match value {
            Value::Object(obj) => {
                for (key, child) in obj {
                    if is_special_element_key(key) || key.starts_with('_') {
                        continue;
                    }

                    let child_path = format!("{}.{}", path, key);

                    // Complex child present, but snapshot has no children (likely not deep-expanded).
                    if child.is_object() {
                        if index.get_element(&child_path).is_some()
                            && index.children_of(&child_path).is_empty()
                            && has_non_special_keys(child.as_object().unwrap())
                        {
                            return true;
                        }
                        if visit(child, &child_path, index) {
                            return true;
                        }
                    } else if let Some(arr) = child.as_array() {
                        let has_object_items = arr.iter().any(|v| v.is_object());
                        if has_object_items
                            && index.get_element(&child_path).is_some()
                            && index.children_of(&child_path).is_empty()
                        {
                            return true;
                        }
                        for item in arr {
                            if visit(item, &child_path, index) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            Value::Array(arr) => arr.iter().any(|v| visit(v, path, index)),
            _ => false,
        }
    }

    visit(resource, root_path, index)
}

fn is_special_element_key(key: &str) -> bool {
    matches!(
        key,
        "resourceType" | "id" | "meta" | "extension" | "modifierExtension"
    )
}

/// Validates resource against profile constraints
fn validate_profile_object(
    value: &Value,
    path: &str,
    index: &ProfileElementIndex<'_>,
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(obj) = value.as_object() else {
        return;
    };

    // Validate each child element
    for child_def in index.children_of(path) {
        let Some(name) = child_def.path.split('.').next_back() else {
            continue;
        };

        // Skip choice base elements ([x])
        if name.ends_with("[x]") {
            continue;
        }

        let child_path = format!("{}.{}", path, name);
        let child_value = obj.get(name);

        // Check for slicing
        if index.has_slicing(&child_path) {
            if let Some(slicing_element) = index.get_slicing_element(&child_path) {
                if let Some(arr) = child_value.and_then(|v| v.as_array()) {
                    validate_sliced_element(
                        arr,
                        &child_path,
                        slicing_element,
                        index,
                        fhirpath_engine,
                        issues,
                    );
                }
            }
        }

        // Validate cardinality (profile may have stricter constraints than base)
        if let Some(v) = child_value {
            let count = match v {
                Value::Array(arr) => arr.len() as u64,
                Value::Null => 0,
                _ => 1,
            };
            let min = child_def.min.unwrap_or(0) as u64;
            let max = child_def.max.as_deref().unwrap_or("*");
            validate_cardinality(count, name, &child_path, min, max, issues);
        } else {
            let min = child_def.min.unwrap_or(0) as u64;
            if min > 0 {
                validate_cardinality(0, name, &child_path, min, "*", issues);
            }
        }

        // Validate fixed value
        if let Some(fixed_value) = &child_def.fixed {
            if let Some(v) = child_value {
                if !values_match(v, fixed_value) {
                    let child_path_clone = child_path.clone();
                    issues.push(
                        ValidationIssue::error(
                            IssueCode::Value,
                            format!(
                                "Element '{}' must have fixed value: {}",
                                name,
                                serde_json::to_string(fixed_value).unwrap_or_default()
                            ),
                        )
                        .with_location(child_path_clone.clone())
                        .with_expression(vec![child_path_clone]),
                    );
                }
            }
        }

        // Validate pattern
        if let Some(pattern_value) = &child_def.pattern {
            if let Some(v) = child_value {
                if !value_matches_pattern(v, pattern_value) {
                    let child_path_clone = child_path.clone();
                    issues.push(
                        ValidationIssue::error(
                            IssueCode::Value,
                            format!("Element '{}' does not match required pattern", name),
                        )
                        .with_location(child_path_clone.clone())
                        .with_expression(vec![child_path_clone]),
                    );
                }
            }
        }

        // Validate type restrictions (profile may restrict types)
        if let Some(v) = child_value {
            if !v.is_null() {
                validate_type_restrictions(v, child_def, &child_path, issues);
            }
        }

        // Check mustSupport
        if child_def.must_support == Some(true)
            && (child_value.is_none() || child_value.map(|v| v.is_null()).unwrap_or(false))
        {
            let child_path_clone = child_path.clone();
            issues.push(
                ValidationIssue::warning(
                    IssueCode::Required,
                    format!("Element '{}' is marked as mustSupport but is missing", name),
                )
                .with_location(child_path_clone.clone())
                .with_expression(vec![child_path_clone]),
            );
        }

        // Recursively validate nested objects
        if let Some(v) = child_value {
            if v.is_object() {
                validate_profile_object(v, &child_path, index, fhirpath_engine, issues);
            } else if let Some(arr) = v.as_array() {
                for item in arr {
                    if item.is_object() {
                        validate_profile_object(item, &child_path, index, fhirpath_engine, issues);
                    }
                }
            }
        }
    }
}

/// Validates a sliced element
fn validate_sliced_element(
    elements: &[Value],
    element_path: &str,
    slicing_element: &ElementDefinition,
    index: &ProfileElementIndex<'_>,
    fhirpath_engine: &Arc<FhirPathEngine>,
    issues: &mut Vec<ValidationIssue>,
) {
    // Parse slicing rules
    let Some(slicing_rules) = parse_slicing_rules(slicing_element) else {
        return;
    };

    // Collect slice definitions
    let slices = collect_slice_definitions_from_index(index, element_path);

    // Delegate to slicing validation
    validate_slicing(
        elements,
        element_path,
        &slicing_rules,
        &slices,
        fhirpath_engine,
        issues,
    );
}

/// Collect slice definitions from element index
fn collect_slice_definitions_from_index<'a>(
    index: &'a ProfileElementIndex<'a>,
    base_path: &str,
) -> Vec<SliceDefinition<'a>> {
    let mut slices = Vec::new();

    // Find all elements that are slices of this path
    for (path, element) in index.by_path.iter() {
        if path.starts_with(base_path) && path.contains(':') {
            if let Some(slice_name) = path.split(':').nth(1) {
                // Only include the direct slice, not nested elements
                if path == &format!("{}:{}", base_path, slice_name) {
                    slices.push(SliceDefinition {
                        name: slice_name.to_string(),
                        element,
                    });
                }
            }
        }
    }

    // Check for default slice
    if let Some(default_element) = index.get_element(&format!("{}:@default", base_path)) {
        slices.push(SliceDefinition {
            name: "@default".to_string(),
            element: default_element,
        });
    }

    slices
}

/// Extract slicing information from a slicing entry element
pub fn parse_slicing_rules(slicing_element: &ElementDefinition) -> Option<SlicingRules> {
    let slicing = slicing_element.slicing.as_ref()?;

    let mut discriminators = Vec::new();
    if let Some(discriminator_list) = &slicing.discriminator {
        for disc in discriminator_list {
            let disc_type = match disc.discriminator_type {
                ModelDiscType::Value => super::slicing::DiscriminatorType::Value,
                ModelDiscType::Exists => super::slicing::DiscriminatorType::Exists,
                ModelDiscType::Pattern => super::slicing::DiscriminatorType::Pattern,
                ModelDiscType::Type => super::slicing::DiscriminatorType::Type,
                ModelDiscType::Profile => super::slicing::DiscriminatorType::Profile,
            };
            discriminators.push(super::slicing::Discriminator {
                type_: disc_type,
                path: disc.path.clone(),
            });
        }
    }

    let rules = match slicing.rules {
        ModelSlicingRules::Closed => super::slicing::SlicingRulesKind::Closed,
        ModelSlicingRules::Open => super::slicing::SlicingRulesKind::Open,
        ModelSlicingRules::OpenAtEnd => super::slicing::SlicingRulesKind::OpenAtEnd,
    };

    let ordered = slicing.ordered.unwrap_or(false);

    Some(SlicingRules {
        discriminators,
        rules,
        ordered,
        description: slicing.description.clone(),
    })
}

/// Collect all slice definitions for a sliced element (public API)
#[allow(dead_code)]
pub fn collect_slice_definitions<'a>(
    snapshot_elements: &'a [ElementDefinition],
    base_path: &str,
) -> Vec<SliceDefinition<'a>> {
    let mut slices = Vec::new();

    for element in snapshot_elements {
        // Slices have path like "Observation.component:systolic"
        if element.path.starts_with(base_path) && element.path.contains(':') {
            if let Some(slice_name) = element.path.split(':').nth(1) {
                // Only include the direct slice, not nested elements
                if element.path == format!("{}:{}", base_path, slice_name) {
                    slices.push(SliceDefinition {
                        name: slice_name.to_string(),
                        element,
                    });
                }
            }
        }
    }

    // Check for default slice (spec 5.1.0.15)
    for element in snapshot_elements {
        if element.path == format!("{}:@default", base_path) {
            slices.push(SliceDefinition {
                name: "@default".to_string(),
                element,
            });
        }
    }

    slices
}

/// Validates cardinality constraints
fn validate_cardinality(
    count: u64,
    element_name: &str,
    element_path: &str,
    min: u64,
    max: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if count < min {
        issues.push(
            ValidationIssue::error(
                IssueCode::Required,
                format!(
                    "Element '{}' has cardinality {}..{}, but found {} occurrence(s)",
                    element_name, min, max, count
                ),
            )
            .with_location(element_path.to_string())
            .with_expression(vec![element_path.to_string()]),
        );
    }

    if max != "*" {
        if let Ok(max_num) = max.parse::<u64>() {
            if count > max_num {
                issues.push(
                    ValidationIssue::error(
                        IssueCode::Structure,
                        format!(
                            "Element '{}' has cardinality {}..{}, but found {} occurrence(s)",
                            element_name, min, max, count
                        ),
                    )
                    .with_location(element_path.to_string())
                    .with_expression(vec![element_path.to_string()]),
                );
            }
        }
    }
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

/// Validates type restrictions (profile may restrict allowed types)
fn validate_type_restrictions(
    value: &Value,
    element_def: &ElementDefinition,
    element_path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(types) = element_def.types.as_ref() else {
        return;
    };

    if types.is_empty() {
        return;
    }

    // For arrays, validate each element
    let values: Vec<&Value> = match value {
        Value::Array(arr) => arr.iter().collect(),
        _ => vec![value],
    };

    for val in values {
        if val.is_null() {
            continue;
        }

        // Check if value matches any of the allowed types
        let mut matches_type = false;
        for type_def in types {
            if value_matches_type(val, &type_def.code) {
                matches_type = true;
                break;
            }
        }

        if !matches_type {
            let allowed_types: Vec<String> = types.iter().map(|t| t.code.clone()).collect();
            issues.push(
                ValidationIssue::error(
                    IssueCode::Value,
                    format!(
                        "Element has incorrect type. Allowed types: {}",
                        allowed_types.join(", ")
                    ),
                )
                .with_location(element_path.to_string())
                .with_expression(vec![element_path.to_string()]),
            );
            return;
        }
    }
}

/// Check if value matches a FHIR type
fn value_matches_type(value: &Value, type_code: &str) -> bool {
    match type_code {
        "string" | "uri" | "url" | "canonical" | "code" | "oid" | "id" | "uuid" | "markdown"
        | "xhtml" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" | "unsignedInt" | "positiveInt" => value.is_number(),
        "decimal" => value.is_number() || value.is_string(),
        "date" | "dateTime" | "instant" | "time" => value.is_string(),
        "base64Binary" => value.is_string(),
        // Complex types - check for object
        "CodeableConcept" | "Coding" | "Identifier" | "Reference" | "Quantity" | "Period"
        | "Range" | "Ratio" | "HumanName" | "Address" | "ContactPoint" => value.is_object(),
        "BackboneElement" | "Element" => value.is_object(),
        _ => value.is_object() || value.is_string(),
    }
}

/// Helper to extract resourceType from resource
fn get_resource_type(resource: &Value) -> Option<String> {
    resource
        .get("resourceType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Profile validation tests will be added as implementation progresses
    }
}
