//! Shared utility functions for FHIRPath function implementations.
//!
//! This module contains helper functions used across multiple function modules,
//! such as value comparison utilities.

use crate::hir::HirBinaryOperator;
use crate::value::{Collection, Value};
use crate::vm::operations::execute_binary_op;

/// Check if two values are equal according to FHIRPath semantics.
///
/// This function handles type coercion between Integer and Decimal types,
/// which is part of the FHIRPath specification.
pub fn items_equal(left: &Value, right: &Value) -> bool {
    let left_col = Collection::singleton(left.clone());
    let right_col = Collection::singleton(right.clone());
    match execute_binary_op(HirBinaryOperator::Eq, left_col, right_col) {
        Ok(result) => result.as_boolean().unwrap_or(false),
        Err(_) => false,
    }
}
