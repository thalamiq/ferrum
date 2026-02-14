//! Filtering functions for FHIRPath.
//!
//! This module implements collection filtering operations like `where()`, `select()`,
//! `ofType()`, and `extension()`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::value::{Collection, Value, ValueData};
use serde_json::Value as JsonValue;
use ferrum_context::FhirContext;

use super::helpers;
use super::type_helpers::{matches_type_specifier_exact, validate_type_specifier};

pub fn where_func(_collection: Collection, _criteria: Option<&Collection>) -> Result<Collection> {
    // Handled by VM opcode Where, but provide fallback
    Err(Error::Unsupported(
        "where() should be handled by VM opcode".into(),
    ))
}

pub fn select_func(
    _collection: Collection,
    _projection: Option<&Collection>,
) -> Result<Collection> {
    // Handled by VM opcode Select, but provide fallback
    Err(Error::Unsupported(
        "select() should be handled by VM opcode".into(),
    ))
}

pub fn repeat(collection: Collection, projection_arg: Option<&Collection>) -> Result<Collection> {
    // Implements FHIRPath repeat() by repeatedly applying the projection and
    // accumulating newly discovered items until no new items are found. Equality
    // for cycle detection follows the = (equals) semantics used elsewhere.
    let projection = projection_arg.ok_or_else(|| {
        Error::InvalidOperation("repeat() requires 1 argument (projection)".into())
    })?;

    if collection.is_empty() || projection.is_empty() {
        return Ok(Collection::empty());
    }

    // Seed queue with initial projection results (deduplicated)
    // Use HashSet for O(1) cycle detection instead of O(n) Vec iteration
    let mut seen: std::collections::HashSet<Value> = std::collections::HashSet::new();
    let mut queue: Vec<Value> = Vec::new();
    let mut result = Collection::empty();

    for projected in projection.iter() {
        if seen.insert(projected.clone()) {
            queue.push(projected.clone());
            result.push(projected.clone());
        }
    }

    // Heuristically determine which fields produced the projection so we can
    // walk the same relationship on subsequent items. This aligns with common
    // repeat() usage such as contains/item traversal.
    let mut projection_fields: HashSet<String> = HashSet::new();
    for parent in collection.iter() {
        if let ValueData::Object(obj_map) = parent.data() {
            for (field, values) in obj_map.iter() {
                if values
                    .iter()
                    .any(|child| projection.iter().any(|proj| values_equal(proj, child)))
                {
                    projection_fields.insert(field.as_ref().to_string());
                }
            }
        }
    }

    // If we could not infer any projection fields, return the first projection.
    if projection_fields.is_empty() {
        return Ok(result);
    }

    // Repeat breadth-first until no new items are produced (or safety limit hit)
    const MAX_ITERATIONS: usize = 10_000;
    let mut iterations = 0;

    while let Some(current) = queue.pop() {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(Error::EvaluationError(format!(
                "repeat() exceeded maximum iterations ({}) - possible infinite loop",
                MAX_ITERATIONS
            )));
        }

        if let ValueData::Object(obj_map) = current.data() {
            for field in projection_fields.iter() {
                if let Some(children) = obj_map.get(field.as_str()) {
                    for child in children.iter() {
                        if seen.insert(child.clone()) {
                            queue.push(child.clone());
                            result.push(child.clone());
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

pub fn of_type(
    collection: Collection,
    type_arg: Option<&Collection>,
    path_hint: Option<&str>,
    fhir_context: Option<&dyn FhirContext>,
    ctx: &Context,
) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let type_spec = type_arg.ok_or_else(|| {
        Error::InvalidOperation("ofType() requires 1 argument (type specifier)".into())
    })?;

    if type_spec.is_empty() {
        return Ok(Collection::empty());
    }

    let type_name = if let Ok(s) = type_spec.as_string() {
        s.to_string()
    } else if type_spec.len() == 1 {
        if let Some(item) = type_spec.iter().next() {
            match item.data() {
                ValueData::String(s) => s.to_string(),
                _ => {
                    return Err(Error::TypeError(
                        "ofType() type specifier must be a string or identifier".into(),
                    ));
                }
            }
        } else {
            return Ok(Collection::empty());
        }
    } else {
        return Err(Error::TypeError(
            "ofType() type specifier must be a singleton string".into(),
        ));
    };

    validate_type_specifier(&type_name, fhir_context)?;

    let mut result = Collection::empty();
    for item in collection.iter() {
        // Match the given type exactly (no inheritance), per HL7 suite expectations.
        if matches_type_specifier_exact(item, &type_name, path_hint, fhir_context, ctx) {
            result.push(item.clone());
        }
    }

    Ok(result)
}

pub fn extension(
    collection: Collection,
    url_arg: Option<&Collection>,
    path_hint: Option<&str>,
    ctx: &Context,
) -> Result<Collection> {
    // extension() filters the input collection for items named "extension" with the given url
    // This is a syntactical shortcut for .extension.where(url = string)

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let url = url_arg
        .ok_or_else(|| Error::InvalidOperation("extension() requires 1 argument (url)".into()))?;

    if url.is_empty() {
        return Ok(Collection::empty());
    }

    let url_str = url
        .as_string()
        .map_err(|_| Error::TypeError("extension() url must be a string".into()))?;

    // Pre-resolve primitive extensions from the root (e.g., _birthDate)
    let primitive_extensions = lookup_primitive_extension(ctx, path_hint);

    let mut result = Collection::empty();

    let matches_url = |ext_item: &Value| -> bool {
        match ext_item.data() {
            ValueData::Object(ext_map) => {
                let Some(url_col) = ext_map.get("url") else {
                    return false;
                };
                let Some(url_val) = url_col.iter().next() else {
                    return false;
                };
                match url_val.data() {
                    ValueData::String(ext_url) => ext_url.as_ref() == url_str.as_ref(),
                    _ => false,
                }
            }
            ValueData::LazyJson { .. } => {
                let Some(JsonValue::Object(obj)) = ext_item.data().resolved_json() else {
                    return false;
                };
                obj.get("url")
                    .and_then(|v| v.as_str())
                    .is_some_and(|u| u == url_str.as_ref())
            }
            _ => false,
        }
    };

    // Navigate to extensions for each item in the collection
    for item in collection.iter() {
        // Get extensions from the item
        if let ValueData::Object(obj_map) = item.data() {
            if let Some(extensions_col) = obj_map.get("extension") {
                for ext_item in extensions_col.iter() {
                    if matches_url(ext_item) {
                        result.push(ext_item.clone());
                    }
                }
            }
        } else if let ValueData::LazyJson { root, path } = item.data() {
            let Some(JsonValue::Object(obj)) = item.data().resolved_json() else {
                continue;
            };
            if let Some(JsonValue::Array(arr)) = obj.get("extension") {
                let mut base_path = path.clone();
                base_path.push(crate::value::JsonPathToken::Key(Arc::from("extension")));
                for (idx, child) in arr.iter().enumerate() {
                    let mut child_path = base_path.clone();
                    child_path.push(crate::value::JsonPathToken::Index(idx));
                    let ext_item = Value::from_json_node(root.clone(), child_path, child);
                    if matches_url(&ext_item) {
                        result.push(ext_item);
                    }
                }
            }
        } else if let Some(prim_ext_container) = &primitive_extensions {
            // Primitive value - try sibling _field extensions from the root resource
            // prim_ext_container is the _birthDate object - need to navigate to its "extension" field
            for container_item in prim_ext_container.iter() {
                match container_item.data() {
                    ValueData::Object(container_map) => {
                        if let Some(extensions_col) = container_map.get("extension") {
                            for ext_item in extensions_col.iter() {
                                if matches_url(ext_item) {
                                    result.push(ext_item.clone());
                                }
                            }
                        }
                    }
                    ValueData::LazyJson { root, path } => {
                        let Some(JsonValue::Object(obj)) = container_item.data().resolved_json()
                        else {
                            continue;
                        };
                        if let Some(JsonValue::Array(arr)) = obj.get("extension") {
                            let mut base_path = path.clone();
                            base_path
                                .push(crate::value::JsonPathToken::Key(Arc::from("extension")));
                            for (idx, child) in arr.iter().enumerate() {
                                let mut child_path = base_path.clone();
                                child_path.push(crate::value::JsonPathToken::Index(idx));
                                let ext_item =
                                    Value::from_json_node(root.clone(), child_path, child);
                                if matches_url(&ext_item) {
                                    result.push(ext_item);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(result)
}

/// Find primitive extensions for the current path (e.g., _birthDate.extension)
fn lookup_primitive_extension(ctx: &Context, path_hint: Option<&str>) -> Option<Collection> {
    let path = path_hint?;
    let mut segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }

    // Drop trailing ".extension" if present
    if segments.last().map(|s| *s == "extension").unwrap_or(false) {
        segments.pop();
    }

    // Remove leading resource type if it matches the current resource
    let resource_type = match ctx.resource.data() {
        ValueData::Object(obj) => obj
            .get("resourceType")
            .and_then(|col| col.iter().next())
            .and_then(|v| match v.data() {
                ValueData::String(s) => Some(s.as_ref().to_string()),
                _ => None,
            }),
        ValueData::LazyJson { .. } => ctx
            .resource
            .data()
            .resolved_json()
            .and_then(|v| v.get("resourceType"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    };
    if let Some(rt) = resource_type.as_deref() {
        if segments.first().map(|s| *s == rt).unwrap_or(false) {
            segments.remove(0);
        }
    }

    if segments.is_empty() {
        return None;
    }

    let mut current = ctx.resource.clone();
    // Walk to the parent object of the target primitive
    for seg in segments.iter().take(segments.len().saturating_sub(1)) {
        let next = match current.data() {
            ValueData::Object(obj) => obj.get(*seg).and_then(|c| c.iter().next()).cloned(),
            ValueData::LazyJson { root, path } => {
                let Some(JsonValue::Object(obj)) = current.data().resolved_json() else {
                    return None;
                };
                let field_value = obj.get(*seg)?;
                let seg: Arc<str> = Arc::from(*seg);
                let mut base_path = path.clone();
                base_path.push(crate::value::JsonPathToken::Key(seg));
                match field_value {
                    JsonValue::Array(arr) => arr.first().map(|child| {
                        let mut child_path = base_path;
                        child_path.push(crate::value::JsonPathToken::Index(0));
                        Value::from_json_node(root.clone(), child_path, child)
                    }),
                    other => Some(Value::from_json_node(root.clone(), base_path, other)),
                }
            }
            _ => None,
        };
        if let Some(val) = next {
            current = val;
        } else {
            return None;
        }
    }

    let last_seg = segments.last()?;
    let underscore = format!("_{}", last_seg);
    match current.data() {
        ValueData::Object(obj) => {
            if let Some(ext_container) = obj.get(underscore.as_str()) {
                return Some(ext_container.clone());
            }
        }
        ValueData::LazyJson { root, path } => {
            let Some(JsonValue::Object(obj)) = current.data().resolved_json() else {
                return None;
            };
            let field_value = obj.get(underscore.as_str())?;
            let mut coll = Collection::empty();
            let mut base_path = path.clone();
            base_path.push(crate::value::JsonPathToken::Key(Arc::from(
                underscore.as_str(),
            )));
            match field_value {
                JsonValue::Array(arr) => {
                    for (idx, child) in arr.iter().enumerate() {
                        let mut child_path = base_path.clone();
                        child_path.push(crate::value::JsonPathToken::Index(idx));
                        coll.push(Value::from_json_node(root.clone(), child_path, child));
                    }
                }
                other => {
                    coll.push(Value::from_json_node(root.clone(), base_path, other));
                }
            }
            return Some(coll);
        }
        _ => {}
    }

    None
}

/// Equality helper for repeat cycle detection using FHIRPath "=" semantics
/// (numeric coercion, string/boolean match, and shallow object identity).
fn values_equal(left: &Value, right: &Value) -> bool {
    if helpers::items_equal(left, right) {
        return true;
    }

    match (left.data(), right.data()) {
        (ValueData::Object(l_map), ValueData::Object(r_map)) => {
            if l_map.len() != r_map.len() {
                return false;
            }
            for (key, l_val) in l_map.iter() {
                if let Some(r_val) = r_map.get(key.as_ref()) {
                    if l_val.len() != r_val.len() {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}
