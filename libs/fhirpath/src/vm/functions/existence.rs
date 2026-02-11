//! Existence and collection query functions for FHIRPath.
//!
//! This module implements functions that check collection properties like `empty()`,
//! `exists()`, `all()`, `distinct()`, etc.

use crate::error::{Error, Result};
use crate::value::{Collection, Value, ValueData};

pub fn empty(collection: Collection) -> Result<Collection> {
    Ok(Collection::singleton(Value::boolean(collection.is_empty())))
}

pub fn exists(collection: Collection, criteria: Option<&Collection>) -> Result<Collection> {
    // exists() with no arguments: returns true if collection is not empty
    if criteria.is_none() {
        return Ok(Collection::singleton(Value::boolean(
            !collection.is_empty(),
        )));
    }

    // With a criteria collection, treat non-empty/true as match (used only in fallback paths)
    let predicate = criteria.unwrap();
    let matches = if predicate.is_empty() {
        false
    } else {
        predicate
            .as_boolean()
            .unwrap_or_else(|_| !predicate.is_empty())
    };
    Ok(Collection::singleton(Value::boolean(matches)))
}

pub fn all(collection: Collection, _criteria: Option<&Collection>) -> Result<Collection> {
    // TODO: Implement criteria evaluation
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }
    // For now, return true if collection is not empty
    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn all_true(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Boolean(b) => {
                if !b {
                    return Ok(Collection::singleton(Value::boolean(false)));
                }
            }
            _ => {
                return Err(Error::TypeError(
                    "allTrue() requires a collection of booleans".into(),
                ));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn any_true(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(false)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Boolean(b) => {
                if *b {
                    return Ok(Collection::singleton(Value::boolean(true)));
                }
            }
            _ => {
                return Err(Error::TypeError(
                    "anyTrue() requires a collection of booleans".into(),
                ));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(false)))
}

pub fn all_false(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Boolean(b) => {
                if *b {
                    return Ok(Collection::singleton(Value::boolean(false)));
                }
            }
            _ => {
                return Err(Error::TypeError(
                    "allFalse() requires a collection of booleans".into(),
                ));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(true)))
}

pub fn any_false(collection: Collection) -> Result<Collection> {
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(false)));
    }

    for item in collection.iter() {
        match item.data() {
            ValueData::Boolean(b) => {
                if !b {
                    return Ok(Collection::singleton(Value::boolean(true)));
                }
            }
            _ => {
                return Err(Error::TypeError(
                    "anyFalse() requires a collection of booleans".into(),
                ));
            }
        }
    }

    Ok(Collection::singleton(Value::boolean(false)))
}

pub fn subset_of(collection: Collection, other: Option<&Collection>) -> Result<Collection> {
    use std::collections::HashSet;

    let other =
        other.ok_or_else(|| Error::InvalidOperation("subsetOf requires 1 argument".into()))?;

    // Empty set is subset of any set
    if collection.is_empty() {
        return Ok(Collection::singleton(Value::boolean(true)));
    }

    // Build HashSet from other for O(1) lookups
    let other_set: HashSet<&Value> = other.iter().collect();

    // Check if all items in collection are in other
    let is_subset = collection.iter().all(|item| other_set.contains(item));

    Ok(Collection::singleton(Value::boolean(is_subset)))
}

pub fn superset_of(collection: Collection, other: Option<&Collection>) -> Result<Collection> {
    // Superset is the reverse of subset
    let other =
        other.ok_or_else(|| Error::InvalidOperation("supersetOf requires 1 argument".into()))?;
    subset_of(other.clone(), Some(&collection))
}

pub fn count(collection: Collection) -> Result<Collection> {
    Ok(Collection::singleton(Value::integer(
        collection.len() as i64
    )))
}

pub fn distinct(collection: Collection) -> Result<Collection> {
    use std::collections::HashSet;

    if collection.is_empty() {
        return Ok(collection);
    }

    let mut seen = HashSet::with_capacity(collection.len());
    let mut result = Collection::with_capacity(collection.len());

    for item in collection.iter() {
        if seen.insert(item.clone()) {
            result.push(item.clone());
        }
    }

    Ok(result)
}

pub fn is_distinct(collection: Collection) -> Result<Collection> {
    let original_len = collection.len();
    let distinct_collection = distinct(collection)?;
    let distinct_len = distinct_collection.len();
    Ok(Collection::singleton(Value::boolean(
        original_len == distinct_len,
    )))
}
