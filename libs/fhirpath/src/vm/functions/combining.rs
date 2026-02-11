//! Collection combining functions for FHIRPath.
//!
//! This module implements functions that combine collections like `union()` and `combine()`.

use crate::error::{Error, Result};
use crate::value::Collection;

pub fn union_func(collection: Collection, other: Option<&Collection>) -> Result<Collection> {
    use std::collections::HashSet;

    let other =
        other.ok_or_else(|| Error::InvalidOperation("union() requires 1 argument".into()))?;

    // Short-circuit: if either side is empty, return the other
    if collection.is_empty() {
        return Ok(other.clone());
    }
    if other.is_empty() {
        return Ok(collection);
    }

    // Use HashSet for O(1) lookups instead of O(n) iteration
    let mut seen = HashSet::with_capacity(collection.len() + other.len());
    let mut result = Collection::with_capacity(collection.len() + other.len());

    for item in collection.iter().chain(other.iter()) {
        // Try to insert into HashSet first
        // Only add to result if not seen before
        if seen.insert(item.clone()) {
            result.push(item.clone());
        }
    }

    Ok(result)
}

pub fn combine(collection: Collection, other: Option<&Collection>) -> Result<Collection> {
    // combine() merges the input and other collections into a single collection
    // without eliminating duplicate values

    let other =
        other.ok_or_else(|| Error::InvalidOperation("combine() requires 1 argument".into()))?;

    if collection.is_empty() && other.is_empty() {
        return Ok(Collection::empty());
    }

    if collection.is_empty() {
        return Ok(other.clone());
    }

    if other.is_empty() {
        return Ok(collection.clone());
    }

    // Simply concatenate all items from both collections
    let mut result = Collection::empty();

    // Add all items from first collection
    for item in collection.iter() {
        result.push(item.clone());
    }

    // Add all items from second collection
    for item in other.iter() {
        result.push(item.clone());
    }

    Ok(result)
}
