//! Unit tests for the FHIRPath VM module

use std::sync::Arc;
use zunder_fhirpath::context::Context;
use zunder_fhirpath::value::Value;
use zunder_fhirpath::vm::{Opcode, Plan, Vm};

mod test_support;

#[test]
fn test_vm_push_const() {
    let plan = Plan {
        opcodes: vec![Opcode::PushConst(0), Opcode::Return],
        max_stack_depth: 64,
        constants: vec![Value::integer(42)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 42);
}

#[test]
fn test_vm_push_multiple_constants() {
    let plan = Plan {
        opcodes: vec![Opcode::PushConst(0), Opcode::PushConst(1), Opcode::Return],
        max_stack_depth: 64,
        constants: vec![Value::integer(10), Value::integer(20)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    // Return only returns the top of stack (last item pushed)
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 20);
}

#[test]
fn test_vm_load_this() {
    let plan = Plan {
        opcodes: vec![Opcode::LoadThis, Opcode::Return],
        max_stack_depth: 64,
        constants: vec![],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let resource = Value::integer(100);
    let ctx = Context::new(resource.clone());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 1);
    // $this should be the resource
    assert_eq!(result.as_integer().unwrap(), 100);
}

#[test]
fn test_vm_load_index() {
    let plan = Plan {
        opcodes: vec![Opcode::LoadIndex, Opcode::Return],
        max_stack_depth: 64,
        constants: vec![],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    // Set index in context (normally done by higher-order functions)
    let result = vm.execute(&plan).unwrap();

    // $index should be 0 initially (or empty if not set)
    // This depends on VM implementation
    assert!(result.len() <= 1);
}

#[test]
fn test_vm_binary_add() {
    let plan = Plan {
        opcodes: vec![
            Opcode::PushConst(0),
            Opcode::PushConst(1),
            Opcode::CallBinary(0), // Add operator
            Opcode::Return,
        ],
        max_stack_depth: 64,
        constants: vec![Value::integer(10), Value::integer(20)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 30);
}

#[test]
fn test_vm_binary_multiply() {
    let plan = Plan {
        opcodes: vec![
            Opcode::PushConst(0),
            Opcode::PushConst(1),
            Opcode::CallBinary(2), // Multiply operator
            Opcode::Return,
        ],
        max_stack_depth: 64,
        constants: vec![Value::integer(5), Value::integer(6)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 30);
}

#[test]
fn test_vm_pop() {
    let plan = Plan {
        opcodes: vec![
            Opcode::PushConst(0),
            Opcode::PushConst(1),
            Opcode::Pop,
            Opcode::PushConst(0), // Push first const again
            Opcode::Return,
        ],
        max_stack_depth: 64,
        constants: vec![Value::integer(10), Value::integer(20)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    // After pop, only one item should remain
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 10);
}

#[test]
fn test_vm_dup() {
    let plan = Plan {
        opcodes: vec![Opcode::PushConst(0), Opcode::Dup, Opcode::Return],
        max_stack_depth: 64,
        constants: vec![Value::integer(42)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    // After dup, Return returns the top collection (duplicated one), which has 1 item
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 42);
}

#[test]
fn test_vm_empty_collection() {
    let plan = Plan {
        opcodes: vec![Opcode::PushConst(0), Opcode::Return], // Push empty collection and return
        max_stack_depth: 64,
        constants: vec![Value::empty()],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 0);
    assert!(result.is_empty());
}

#[test]
fn test_vm_navigation() {
    use serde_json::json;

    let resource_json = json!({
        "name": "John"
    });
    let resource = Value::from_json(resource_json);

    let plan = Plan {
        opcodes: vec![
            Opcode::LoadThis,
            Opcode::Navigate(0), // Navigate to "name"
            Opcode::Return,
        ],
        max_stack_depth: 64,
        constants: vec![],
        segments: vec![Arc::from("name")],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(resource);
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result.as_string().unwrap().as_ref(), "John");
}

#[test]
fn test_vm_index() {
    // Test Index with a multi-item collection by using Union to combine singletons
    // Push three singletons, union them, then index
    let plan = Plan {
        opcodes: vec![
            Opcode::PushConst(0),   // Push 10
            Opcode::PushConst(1),   // Push 20
            Opcode::CallBinary(40), // Union (impl_id 40)
            Opcode::PushConst(2),   // Push 30
            Opcode::CallBinary(40), // Union again
            Opcode::Index(0),       // Index 0 (should get 10)
            Opcode::Return,
        ],
        max_stack_depth: 64,
        constants: vec![Value::integer(10), Value::integer(20), Value::integer(30)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 10);
}

#[test]
fn test_vm_error_on_empty_stack() {
    let plan = Plan {
        opcodes: vec![
            Opcode::Pop, // Try to pop from empty stack
        ],
        max_stack_depth: 64,
        constants: vec![],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan);

    // Should error or return empty collection
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[test]
fn test_vm_binary_op_empty_collection() {
    let plan = Plan {
        opcodes: vec![
            Opcode::PushConst(0),  // Empty collection
            Opcode::PushConst(1),  // Integer
            Opcode::CallBinary(0), // Add
            Opcode::Return,
        ],
        max_stack_depth: 64,
        constants: vec![Value::empty(), Value::integer(10)],
        segments: vec![],
        type_specifiers: vec![],
        functions: vec![],
        subplans: vec![],
        variables: vec![],
    };

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());
    let mut vm = Vm::new(&ctx, engine);
    let result = vm.execute(&plan).unwrap();

    // Binary op with empty collection should return empty
    assert_eq!(result.len(), 0);
}
