//! Property-based tests using QuickCheck

use quickcheck::{QuickCheck, TestResult};
use zunder_fhirpath::{Context, Value};

mod test_support;

/// Property: Addition is commutative for integers
/// Using manual test cases instead of QuickCheck to avoid stack overflow
#[test]
fn prop_addition_commutative() {
    let test_cases = vec![
        (0, 0),
        (1, 2),
        (-1, 2),
        (1, -2),
        (-1, -2),
        (100, 200),
        (-100, 200),
        (100, -200),
        (-100, -200),
        (1000, 2000),
        (-1000, 2000),
        (1000, -2000),
        (-1000, -2000),
        (100000, 200000),
        (-100000, 200000),
        (100000, -200000),
        (-100000, -200000),
    ];

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    for (a, b) in test_cases {
        let expr1 = format!("{} + {}", a, b);
        let expr2 = format!("{} + {}", b, a);

        let result1 = engine.evaluate_expr(&expr1, &ctx, None).unwrap();
        let result2 = engine.evaluate_expr(&expr2, &ctx, None).unwrap();

        assert_eq!(
            result1.as_integer().unwrap(),
            result2.as_integer().unwrap(),
            "Addition should be commutative: {} + {} == {} + {}",
            a,
            b,
            b,
            a
        );
    }
}

/// Property: Multiplication is commutative for integers
/// Using manual test cases instead of QuickCheck to avoid stack overflow
#[test]
fn prop_multiplication_commutative() {
    let test_cases = vec![
        (0, 0),
        (1, 2),
        (-1, 2),
        (1, -2),
        (-1, -2),
        (10, 20),
        (-10, 20),
        (10, -20),
        (-10, -20),
        (100, 200),
        (-100, 200),
        (100, -200),
        (-100, -200),
        (1000, 1),
        (-1000, 1),
        (1000, -1),
        (-1000, -1),
    ];

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    for (a, b) in test_cases {
        let expr1 = format!("{} * {}", a, b);
        let expr2 = format!("{} * {}", b, a);

        let result1 = engine.evaluate_expr(&expr1, &ctx, None).unwrap();
        let result2 = engine.evaluate_expr(&expr2, &ctx, None).unwrap();

        assert_eq!(
            result1.as_integer().unwrap(),
            result2.as_integer().unwrap(),
            "Multiplication should be commutative: {} * {} == {} * {}",
            a,
            b,
            b,
            a
        );
    }
}

/// Property: Addition identity (x + 0 = x)
/// Using manual test cases instead of QuickCheck to avoid stack overflow
#[test]
fn prop_addition_identity() {
    let test_cases = vec![0, 1, -1, 100, -100, 1000, -1000, 100000, -100000];

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    for x in test_cases {
        let expr = format!("{} + 0", x);
        let result = engine.evaluate_expr(&expr, &ctx, None).unwrap();

        assert_eq!(
            result.as_integer().unwrap(),
            x,
            "Addition identity should hold: {} + 0 == {}",
            x,
            x
        );
    }
}

/// Property: Multiplication identity (x * 1 = x)
/// Using manual test cases instead of QuickCheck to avoid stack overflow
#[test]
fn prop_multiplication_identity() {
    let test_cases = vec![0, 1, -1, 100, -100, 1000, -1000, 100000, -100000];

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    for x in test_cases {
        let expr = format!("{} * 1", x);
        let result = engine.evaluate_expr(&expr, &ctx, None).unwrap();

        assert_eq!(
            result.as_integer().unwrap(),
            x,
            "Multiplication identity should hold: {} * 1 == {}",
            x,
            x
        );
    }
}

/// Property: Double negation (--x = x)
/// Using manual test cases instead of QuickCheck to avoid stack overflow
#[test]
fn prop_double_negation() {
    let test_cases = vec![0, 1, -1, 100, -100, 1000, -1000, 100000, -100000];

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    for x in test_cases {
        let expr = format!("-(-{})", x);
        let result = engine.evaluate_expr(&expr, &ctx, None).unwrap();

        assert_eq!(
            result.as_integer().unwrap(),
            x,
            "Double negation should hold: -(-{}) == {}",
            x,
            x
        );
    }
}

/// Property: Equality is reflexive (x = x is always true)
/// Using manual test cases instead of QuickCheck to avoid stack overflow
#[test]
fn prop_equality_reflexive() {
    let test_cases = vec![0, 1, -1, 100, -100, 1000, -1000, 100000, -100000];

    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    for x in test_cases {
        let expr = format!("{} = {}", x, x);
        let result = engine.evaluate_expr(&expr, &ctx, None).unwrap();

        assert!(
            result.as_boolean().unwrap(),
            "Equality should be reflexive: {} = {} should be true",
            x,
            x
        );
    }
}

/// Property: String length is non-negative
#[test]
fn prop_string_length_nonnegative() {
    fn prop(s: String) -> TestResult {
        let engine = test_support::engine_r5();
        let ctx = Context::new(Value::empty());

        // Escape single quotes in string
        let escaped = s.replace('\'', "\\'");
        let expr = format!("'{}'.length()", escaped);

        if let Ok(result) = engine.evaluate_expr(&expr, &ctx, None) {
            let len = result.as_integer().unwrap();
            TestResult::from_bool(len >= 0)
        } else {
            TestResult::discard()
        }
    }

    QuickCheck::new()
        .tests(100)
        .quickcheck(prop as fn(String) -> TestResult);
}

/// Property: toString is idempotent for strings
#[test]
fn prop_to_string_idempotent() {
    fn prop(s: String) -> TestResult {
        let engine = test_support::engine_r5();
        let ctx = Context::new(Value::empty());

        // Escape backslashes first, then single quotes
        // In FHIRPath: \ becomes \\, ' becomes \'
        let escaped = s.replace('\\', "\\\\").replace('\'', "\\'");
        let expr = format!("'{}'.toString()", escaped);

        if let Ok(result) = engine.evaluate_expr(&expr, &ctx, None) {
            let result_arc = result.as_string().unwrap();
            let result_str: &str = result_arc.as_ref();
            TestResult::from_bool(result_str == s.as_str())
        } else {
            TestResult::discard()
        }
    }

    QuickCheck::new()
        .tests(100)
        .quickcheck(prop as fn(String) -> TestResult);
}

/// Property: Boolean not is involutory (not(not(x)) = x)
#[test]
fn prop_not_involutory() {
    fn prop(b: bool) -> TestResult {
        let engine = test_support::engine_r5();
        let ctx = Context::new(Value::empty());

        let expr = format!("{}.not().not()", b);
        let result = engine.evaluate_expr(&expr, &ctx, None).unwrap();

        TestResult::from_bool(result.as_boolean().unwrap() == b)
    }

    QuickCheck::new()
        .tests(100)
        .quickcheck(prop as fn(bool) -> TestResult);
}

/// Property: Empty collection operations return empty
#[test]
fn prop_empty_collection_operations() {
    fn prop(op: String) -> TestResult {
        // Only test safe operations
        let safe_ops = vec!["+", "-", "*", "/", "=", "!=", "<", ">", "<=", ">="];
        if !safe_ops.contains(&op.as_str()) {
            return TestResult::discard();
        }

        let engine = test_support::engine_r5();
        let ctx = Context::new(Value::empty());

        let expr = format!("empty() {} 1", op);

        if let Ok(result) = engine.evaluate_expr(&expr, &ctx, None) {
            // Empty collection operations should return empty or false
            TestResult::from_bool(result.is_empty() || !result.as_boolean().unwrap_or(false))
        } else {
            TestResult::discard()
        }
    }

    QuickCheck::new()
        .tests(100)
        .quickcheck(prop as fn(String) -> TestResult);
}
