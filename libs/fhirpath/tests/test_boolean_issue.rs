//! Test to investigate boolean conversion issues

use zunder_fhirpath::{Context, Value};

mod test_support;

#[test]
fn test_boolean_results() {
    let engine = test_support::engine_r5();

    // Test 1: Simple boolean from .not()
    let result = engine
        .evaluate_expr("true.not()", &Context::new(Value::empty()), None)
        .unwrap();
    println!(
        "true.not() = {:?}, len={}, is_boolean={}",
        result,
        result.len(),
        result.as_boolean().is_ok()
    );
    if let Ok(b) = result.as_boolean() {
        println!("  Boolean value: {}", b);
        assert!(!b);
    } else {
        panic!("Expected boolean result");
    }

    // Test 2: all() function
    let result = engine
        .evaluate_expr(
            "(1|2|3).all($this > 0)",
            &Context::new(Value::empty()),
            None,
        )
        .unwrap();
    println!(
        "(1|2|3).all($this > 0) = {:?}, len={}, is_boolean={}",
        result,
        result.len(),
        result.as_boolean().is_ok()
    );
    if let Ok(b) = result.as_boolean() {
        println!("  Boolean value: {}", b);
    } else {
        println!("  ERROR: Cannot convert to boolean");
        println!("  Result items:");
        for (i, item) in result.iter().enumerate() {
            println!("    [{}]: {:?}", i, item);
        }
    }

    // Test 3: subsetOf
    let result = engine
        .evaluate_expr("(1|2).subsetOf(1|2|3)", &Context::new(Value::empty()), None)
        .unwrap();
    println!(
        "(1|2).subsetOf(1|2|3) = {:?}, len={}, is_boolean={}",
        result,
        result.len(),
        result.as_boolean().is_ok()
    );
    if let Ok(b) = result.as_boolean() {
        println!("  Boolean value: {}", b);
    } else {
        println!("  ERROR: Cannot convert to boolean");
        println!("  Result items:");
        for (i, item) in result.iter().enumerate() {
            println!("    [{}]: {:?}", i, item);
        }
    }
}
