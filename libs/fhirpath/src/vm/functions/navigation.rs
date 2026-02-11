//! Navigation functions for FHIRPath.
//!
//! This module implements tree navigation functions like `children()` and `descendants()`.

use crate::error::{Error, Result};
use crate::value::{Collection, Value, ValueData};
use serde_json::Value as JsonValue;
use std::sync::Arc;

use super::helpers::items_equal;

pub fn children(collection: Collection, name_arg: Option<&Collection>) -> Result<Collection> {
    // children() returns immediate child nodes of all items
    // Optional name argument filters to specific property name

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    let mut result = Collection::empty();

    // If name argument provided, filter to that property
    let filter_name =
        if let Some(name_arg) = name_arg {
            if name_arg.is_empty() {
                return Ok(Collection::empty());
            }
            Some(name_arg.as_string().map_err(|_| {
                Error::TypeError("children() name argument must be a string".into())
            })?)
        } else {
            None
        };

    for item in collection.iter() {
        match item.data() {
            ValueData::Object(obj_map) => {
                // For objects, children are all property values
                if let Some(ref filter) = filter_name {
                    // Filter to specific property
                    if let Some(prop_collection) = obj_map.get(filter.as_ref()) {
                        // Add all items from the property collection
                        for prop_item in prop_collection.iter() {
                            result.push(prop_item.clone());
                        }
                    }
                } else {
                    // Add all property values
                    for prop_collection in obj_map.values() {
                        for prop_item in prop_collection.iter() {
                            result.push(prop_item.clone());
                        }
                    }
                }
            }
            ValueData::LazyJson { root, path } => {
                let Some(JsonValue::Object(obj)) = item.data().resolved_json() else {
                    continue;
                };

                if let Some(ref filter) = filter_name {
                    if let Some(field_value) = obj.get(filter.as_ref()) {
                        let mut base_path = path.clone();
                        base_path.push(crate::value::JsonPathToken::Key(filter.clone()));
                        match field_value {
                            JsonValue::Array(arr) => {
                                for (idx, child) in arr.iter().enumerate() {
                                    let mut child_path = base_path.clone();
                                    child_path.push(crate::value::JsonPathToken::Index(idx));
                                    result.push(Value::from_json_node(
                                        root.clone(),
                                        child_path,
                                        child,
                                    ));
                                }
                            }
                            other => {
                                result.push(Value::from_json_node(root.clone(), base_path, other));
                            }
                        }
                    }
                } else {
                    for (key, field_value) in obj.iter() {
                        let key: Arc<str> = Arc::from(key.as_str());
                        let mut base_path = path.clone();
                        base_path.push(crate::value::JsonPathToken::Key(key.clone()));
                        match field_value {
                            JsonValue::Array(arr) => {
                                for (idx, child) in arr.iter().enumerate() {
                                    let mut child_path = base_path.clone();
                                    child_path.push(crate::value::JsonPathToken::Index(idx));
                                    result.push(Value::from_json_node(
                                        root.clone(),
                                        child_path,
                                        child,
                                    ));
                                }
                            }
                            other => {
                                result.push(Value::from_json_node(root.clone(), base_path, other));
                            }
                        }
                    }
                }
            }
            // For arrays/collections, children would be elements, but we don't have array type yet
            // For primitives (Boolean, Integer, Decimal, String, etc.), there are no children
            _ => {
                // Primitives have no children - don't add anything
            }
        }
    }

    Ok(result)
}

pub fn descendants(collection: Collection, name_arg: Option<&Collection>) -> Result<Collection> {
    // descendants() returns all descendant nodes (recursive children)
    // This is equivalent to repeat(children())
    // Uses cycle detection to prevent infinite loops

    if collection.is_empty() {
        return Ok(Collection::empty());
    }

    // Input queue: items to process
    let mut input_queue: Vec<Value> = collection.iter().cloned().collect();
    // Output collection: unique descendant items found
    let mut result = Collection::empty();
    // Seen items: track items already processed (cycle detection)
    let mut seen_items = Vec::new();

    // Helper to check if item is already seen
    let is_seen = |item: &Value, seen: &[Value]| -> bool {
        for seen_item in seen {
            if items_equal(item, seen_item) {
                return true;
            }
        }
        false
    };

    // Helper to get children of an item
    let get_children = |item: &Value, filter_name: Option<&Arc<str>>| -> Collection {
        let mut children = Collection::empty();
        match item.data() {
            ValueData::Object(obj_map) => {
                if let Some(filter) = filter_name {
                    if let Some(prop_collection) = obj_map.get(filter.as_ref()) {
                        for prop_item in prop_collection.iter() {
                            children.push(prop_item.clone());
                        }
                    }
                } else {
                    for prop_collection in obj_map.values() {
                        for prop_item in prop_collection.iter() {
                            children.push(prop_item.clone());
                        }
                    }
                }
            }
            ValueData::LazyJson { root, path } => {
                let Some(JsonValue::Object(obj)) = item.data().resolved_json() else {
                    return children;
                };

                if let Some(filter) = filter_name {
                    if let Some(field_value) = obj.get(filter.as_ref()) {
                        let mut base_path = path.clone();
                        base_path.push(crate::value::JsonPathToken::Key(filter.clone()));
                        match field_value {
                            JsonValue::Array(arr) => {
                                for (idx, child) in arr.iter().enumerate() {
                                    let mut child_path = base_path.clone();
                                    child_path.push(crate::value::JsonPathToken::Index(idx));
                                    children.push(Value::from_json_node(
                                        root.clone(),
                                        child_path,
                                        child,
                                    ));
                                }
                            }
                            other => {
                                children.push(Value::from_json_node(
                                    root.clone(),
                                    base_path,
                                    other,
                                ));
                            }
                        }
                    }
                } else {
                    for (key, field_value) in obj.iter() {
                        let key: Arc<str> = Arc::from(key.as_str());
                        let mut base_path = path.clone();
                        base_path.push(crate::value::JsonPathToken::Key(key.clone()));
                        match field_value {
                            JsonValue::Array(arr) => {
                                for (idx, child) in arr.iter().enumerate() {
                                    let mut child_path = base_path.clone();
                                    child_path.push(crate::value::JsonPathToken::Index(idx));
                                    children.push(Value::from_json_node(
                                        root.clone(),
                                        child_path,
                                        child,
                                    ));
                                }
                            }
                            other => {
                                children.push(Value::from_json_node(
                                    root.clone(),
                                    base_path,
                                    other,
                                ));
                            }
                        }
                    }
                }
            }
            _ => {
                // Primitives have no children
            }
        }
        children
    };

    // Extract filter name if provided
    let filter_name =
        if let Some(name_arg) = name_arg {
            if name_arg.is_empty() {
                return Ok(Collection::empty());
            }
            Some(name_arg.as_string().map_err(|_| {
                Error::TypeError("descendants() name argument must be a string".into())
            })?)
        } else {
            None
        };

    // Process queue until empty
    while let Some(current_item) = input_queue.pop() {
        // Get children of current item
        let children_collection = get_children(&current_item, filter_name.as_ref());

        // Process all children
        for child in children_collection.iter() {
            // Check if child is already seen (cycle detection)
            if !is_seen(child, &seen_items) {
                // Add to output and queue for further processing
                result.push(child.clone());
                seen_items.push(child.clone());
                input_queue.push(child.clone());
            }
        }
    }

    Ok(result)
}
