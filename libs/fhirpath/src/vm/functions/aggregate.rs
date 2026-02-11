//! Aggregate function implementation for FHIRPath.
//!
//! The aggregate function applies an aggregator expression to each item in a collection,
//! accumulating results into a single value.

use crate::context::Context;
use crate::engine::Engine;
use crate::error::{Error, Result};
use crate::value::Collection;
use crate::vm::Plan;

/// Aggregate function implementation.
///
/// Note: The FHIRPath aggregate function is a higher-order construct that is normally
/// compiled to the dedicated `Aggregate` opcode. Direct invocation via `CallFunction`
/// does not carry the aggregator subplan, so this path intentionally reports an
/// explicit error to guide callers toward the compiled form.
pub fn aggregate(
    _collection: Collection,
    _aggregator_arg: Option<&Collection>,
    _init_arg: Option<&Collection>,
) -> Result<Collection> {
    Err(Error::InvalidOperation(
        "aggregate() is a higher-order function and should be compiled to the Aggregate opcode"
            .into(),
    ))
}

/// Execute an aggregate using compiled subplans (used by the VM Aggregate opcode).
///
/// This closely follows the FHIRPath specification:
/// - `$total` starts as the supplied `init` expression or `{}` when not provided.
/// - For each element in the input collection, the aggregator expression is evaluated
///   with `$this`, `$index`, and `$total` in scope.
/// - After each iteration, `$total` is replaced with the result of the aggregator.
pub fn aggregate_with_subplans(
    collection: Collection,
    aggregator_plan: &Plan,
    init_value_plan: Option<&Plan>,
    ctx: &Context,
    engine: &Engine,
) -> Result<Collection> {
    // When the input is empty, return the init expression (or empty) per spec.
    if collection.is_empty() {
        if let Some(init_plan) = init_value_plan {
            let mut init_vm = crate::vm::Vm::new(ctx, engine);
            return init_vm.execute(init_plan);
        }
        return Ok(Collection::empty());
    }

    // Evaluate the init expression once to seed $total (or start with empty).
    let mut total = if let Some(init_plan) = init_value_plan {
        let mut init_vm = crate::vm::Vm::new(ctx, engine);
        init_vm.execute(init_plan)?
    } else {
        Collection::empty()
    };

    // Iterate through each element, evaluating the aggregator with $this/$index/$total.
    for (index, item) in collection.iter().enumerate() {
        let item_context = Context {
            this: Some(item.clone()),
            index: Some(index),
            strict: ctx.strict,
            variables: ctx.variables.clone(),
            resource: ctx.resource.clone(),
            root: ctx.root.clone(),
        };

        let mut item_vm = crate::vm::Vm::new_for_predicate(&item_context, engine);
        item_vm.set_total(total.clone());

        let aggregator_result = item_vm.execute(aggregator_plan)?;
        total = aggregator_result;
    }

    Ok(total)
}
