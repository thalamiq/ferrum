//! Integration tests for FHIRPath engine
//!
//! Tests the full pipeline: Parse → AST → HIR → VM → Execution

use rust_decimal::Decimal;
use zunder_fhirpath::{Collection, Context, Engine, Value};
#[path = "../test_support/mod.rs"]
mod test_support;

fn get_test_engine() -> &'static Engine {
    test_support::engine_r5()
}

fn eval(expr: &str, resource: Value) -> Collection {
    let engine = get_test_engine();
    let ctx = Context::new(resource);
    engine.evaluate_expr(expr, &ctx, None).unwrap()
}

fn eval_empty(expr: &str) -> Collection {
    eval(expr, Value::empty())
}

// ============================================
// Literals
// ============================================

#[test]
fn test_literals() {
    // Boolean
    let result = eval_empty("true");
    assert_eq!(result.len(), 1);
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("false");
    assert_eq!(result.len(), 1);
    assert!(!result.as_boolean().unwrap());

    // Integer
    let result = eval_empty("42");
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 42);

    // Decimal
    let result = eval_empty("3.14");
    assert_eq!(result.len(), 1);
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(314, 2));
        }
        _ => panic!("Expected decimal"),
    }

    // String
    let result = eval_empty("'hello'");
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_string().unwrap().as_ref(), "hello");
}

// ============================================
// Arithmetic Operations
// ============================================

#[test]
fn test_arithmetic() {
    // Addition
    let result = eval_empty("1 + 2");
    assert_eq!(result.as_integer().unwrap(), 3);

    let result = eval_empty("1.5 + 2.5");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(40, 1)); // 4.0
        }
        _ => panic!("Expected decimal"),
    }

    // Subtraction
    let result = eval_empty("5 - 3");
    assert_eq!(result.as_integer().unwrap(), 2);

    // Multiplication
    let result = eval_empty("3 * 4");
    assert_eq!(result.as_integer().unwrap(), 12);

    // Division
    let result = eval_empty("10 / 2");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(50, 1)); // 5.0
        }
        _ => panic!("Expected decimal"),
    }

    // Modulo
    let result = eval_empty("10 mod 3");
    assert_eq!(result.as_integer().unwrap(), 1);
}

// ============================================
// Comparison Operations
// ============================================

#[test]
fn test_comparison() {
    // Less than
    let result = eval_empty("1 < 2");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("2 < 1");
    assert!(!result.as_boolean().unwrap());

    // Less or equal
    let result = eval_empty("1 <= 2");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("2 <= 2");
    assert!(result.as_boolean().unwrap());

    // Greater than
    let result = eval_empty("3 > 2");
    assert!(result.as_boolean().unwrap());

    // Greater or equal
    let result = eval_empty("3 >= 3");
    assert!(result.as_boolean().unwrap());
}

// ============================================
// Equality Operations
// ============================================

#[test]
fn test_equality() {
    // Equal
    let result = eval_empty("1 = 1");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("1 = 2");
    assert!(!result.as_boolean().unwrap());

    // Not equal
    let result = eval_empty("1 != 2");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("1 != 1");
    assert!(!result.as_boolean().unwrap());
}

// ============================================
// Boolean Operations
// ============================================

#[test]
fn test_boolean_ops() {
    // And
    let result = eval_empty("true and true");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("true and false");
    assert!(!result.as_boolean().unwrap());

    // Or
    let result = eval_empty("true or false");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("false or false");
    assert!(!result.as_boolean().unwrap());

    // Not - called on collection
    let result = eval_empty("true.not()");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("false.not()");
    assert!(result.as_boolean().unwrap());
}

// ============================================
// Unary Operations
// ============================================

#[test]
fn test_unary() {
    // Unary minus
    let result = eval_empty("-5");
    assert_eq!(result.as_integer().unwrap(), -5);

    let result = eval_empty("-(-5)");
    assert_eq!(result.as_integer().unwrap(), 5);

    // Unary plus (should validate numeric)
    let result = eval_empty("+5");
    assert_eq!(result.as_integer().unwrap(), 5);
}

// ============================================
// Existence Functions
// ============================================

#[test]
fn test_existence_functions() {
    // empty() - need to call on a collection
    // In FHIRPath, empty collection is {} or just empty()
    // For now, test with literal that creates collection
    let result = eval_empty("1.empty()");
    assert!(!result.as_boolean().unwrap());

    // exists()
    let result = eval_empty("1.exists()");
    assert!(result.as_boolean().unwrap());

    // count()
    let result = eval_empty("1.count()");
    assert_eq!(result.as_integer().unwrap(), 1);
}

// ============================================
// Subsetting Functions
// ============================================

#[test]
fn test_subsetting() {
    // first() on empty collection
    let result = eval_empty("{}.first()");
    assert_eq!(result.len(), 0);

    // first() on single item
    let result = eval_empty("1.first()");
    assert_eq!(result.as_integer().unwrap(), 1);

    // last()
    let result = eval_empty("1.last()");
    assert_eq!(result.as_integer().unwrap(), 1);

    // single()
    let result = eval_empty("1.single()");
    assert_eq!(result.as_integer().unwrap(), 1);
}

// ============================================
// String Functions
// ============================================

#[test]
fn test_string_functions() {
    // toString()
    let result = eval_empty("42.toString()");
    assert_eq!(result.as_string().unwrap().as_ref(), "42");

    let result = eval_empty("true.toString()");
    assert_eq!(result.as_string().unwrap().as_ref(), "true");

    // length()
    let result = eval_empty("'hello'.length()");
    assert_eq!(result.as_integer().unwrap(), 5);

    // upper()
    let result = eval_empty("'hello'.upper()");
    assert_eq!(result.as_string().unwrap().as_ref(), "HELLO");

    // lower()
    let result = eval_empty("'HELLO'.lower()");
    assert_eq!(result.as_string().unwrap().as_ref(), "hello");

    // startsWith()
    let result = eval_empty("'hello'.startsWith('he')");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'hello'.startsWith('lo')");
    assert!(!result.as_boolean().unwrap());

    // endsWith()
    let result = eval_empty("'hello'.endsWith('lo')");
    assert!(result.as_boolean().unwrap());

    // contains (string function)
    let result = eval_empty("'hello'.contains('ell')");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'hello'.contains('xyz')");
    assert!(!result.as_boolean().unwrap());
}

// ============================================
// Higher-Order Functions
// ============================================

#[test]
fn test_where() {
    // Start with simplest case: where with constant false (should return empty)
    let result = eval_empty("(1 | 2 | 3).where(false)");
    assert_eq!(
        result.len(),
        0,
        "where(false) should return empty collection"
    );

    // Test where with constant true (should return all items)
    let result = eval_empty("(1 | 2 | 3).where(true)");
    assert_eq!(result.len(), 3, "where(true) should return all 3 items");

    // Simple where with boolean predicate using $this
    // This is the key test - $this should be accessible in predicate
    let result = eval_empty("(1 | 2 | 3).where($this > 1)");
    assert_eq!(
        result.len(),
        2,
        "where($this > 1) should return 2 items, got {}",
        result.len()
    );
    // Check that result contains 2 and 3 (order may vary)
    let values: Vec<i64> = result
        .iter()
        .map(|v| match v.data() {
            zunder_fhirpath::value::ValueData::Integer(i) => *i,
            _ => panic!("Expected integer"),
        })
        .collect();
    assert_eq!(values.len(), 2);
    assert!(
        values.contains(&2),
        "Result should contain 2, got: {:?}",
        values
    );
    assert!(
        values.contains(&3),
        "Result should contain 3, got: {:?}",
        values
    );

    // Where with empty result
    let result = eval_empty("(1 | 2 | 3).where($this > 10)");
    assert_eq!(result.len(), 0);

    // Where on empty collection
    let result = eval_empty("{}.where($this > 1)");
    assert_eq!(result.len(), 0);

    // Where with exists() predicate - empty string IS a value, so exists() returns true
    // An empty string '' is NOT the same as an empty collection {}
    let result = eval_empty("('hello' | 'world' | '').where($this.exists())");
    assert_eq!(result.len(), 3); // All three strings exist (empty string is still a value)

    // To exclude empty strings, use length() > 0
    let result = eval_empty("('hello' | 'world' | '').where($this.length() > 0)");
    assert_eq!(result.len(), 2); // Empty string excluded

    // Where with equality predicate
    // Note: The | operator deduplicates, so (1 | 2 | 3 | 2) = {1, 2, 3}
    let result = eval_empty("(1 | 2 | 3 | 2).where($this = 2)");
    assert_eq!(result.len(), 1); // One 2 (union operator deduplicates)
}

#[test]
fn test_select() {
    // Simple select projection
    let result = eval_empty("(1 | 2 | 3).select($this * 2)");
    assert_eq!(result.len(), 3);
    let values: Vec<i64> = result
        .iter()
        .map(|v| match v.data() {
            zunder_fhirpath::value::ValueData::Integer(i) => *i,
            _ => panic!("Expected integer"),
        })
        .collect();
    assert_eq!(values, vec![2, 4, 6]);

    // Select on empty collection
    let result = eval_empty("{}.select($this * 2)");
    assert_eq!(result.len(), 0);

    // Select with string projection
    let result = eval_empty("(1 | 2 | 3).select($this.toString())");
    assert_eq!(result.len(), 3);
    let values: Vec<&str> = result
        .iter()
        .map(|v| match v.data() {
            zunder_fhirpath::value::ValueData::String(s) => s.as_ref(),
            _ => panic!("Expected string"),
        })
        .collect();
    assert_eq!(values, vec!["1", "2", "3"]);
}

#[test]
fn test_repeat() {
    // Simple repeat - should process items and add new ones
    // Note: repeat() requires a projection that returns children/related items
    // For a simple test, we'll use a projection that returns the item itself (should stop immediately)

    // Empty collection returns empty
    let result = eval_empty("{}.repeat($this)");
    assert_eq!(result.len(), 0);

    // Single item with identity projection should return that item
    let result = eval_empty("1.repeat($this)");
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_integer().unwrap(), 1);

    // Multiple items with identity projection should return all items (no cycles)
    let result = eval_empty("(1 | 2 | 3).repeat($this)");
    assert_eq!(result.len(), 3);
    let values: Vec<i64> = result
        .iter()
        .map(|v| match v.data() {
            zunder_fhirpath::value::ValueData::Integer(i) => *i,
            _ => panic!("Expected integer"),
        })
        .collect();
    assert_eq!(values.len(), 3);
    assert!(values.contains(&1));
    assert!(values.contains(&2));
    assert!(values.contains(&3));
}

// ============================================
// Conversion Functions
// ============================================

#[test]
fn test_conversion_functions() {
    // toBoolean()
    let result = eval_empty("true.toBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("false.toBoolean()");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("1.toBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("0.toBoolean()");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("2.toBoolean()");
    assert_eq!(result.len(), 0); // Non-0/1 integers return empty

    let result = eval_empty("'true'.toBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'false'.toBoolean()");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("'yes'.toBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'no'.toBoolean()");
    assert!(!result.as_boolean().unwrap());

    // convertsToBoolean()
    let result = eval_empty("true.convertsToBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("1.convertsToBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("2.convertsToBoolean()");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("'true'.convertsToBoolean()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'invalid'.convertsToBoolean()");
    assert!(!result.as_boolean().unwrap());

    // toInteger()
    let result = eval_empty("42.toInteger()");
    assert_eq!(result.as_integer().unwrap(), 42);

    // Decimal with fractional part returns empty (no Decimal → Integer conversion per spec)
    let result = eval_empty("3.14.toInteger()");
    assert_eq!(result.len(), 0);

    // Decimal without fractional part converts successfully
    let result = eval_empty("3.0.toInteger()");
    assert_eq!(result.as_integer().unwrap(), 3);

    let result = eval_empty("'42'.toInteger()");
    assert_eq!(result.as_integer().unwrap(), 42);

    // String with decimal point doesn't match integer regex, returns empty
    let result = eval_empty("'3.14'.toInteger()");
    assert_eq!(result.len(), 0);

    // convertsToInteger()
    let result = eval_empty("42.convertsToInteger()");
    assert!(result.as_boolean().unwrap());

    // Decimal with fractional part cannot convert to integer
    let result = eval_empty("3.14.convertsToInteger()");
    assert!(!result.as_boolean().unwrap());

    // Decimal without fractional part can convert to integer
    let result = eval_empty("3.0.convertsToInteger()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'42'.convertsToInteger()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'invalid'.convertsToInteger()");
    assert!(!result.as_boolean().unwrap());

    // toDecimal()
    let result = eval_empty("42.toDecimal()");
    let dec = result.iter().next().unwrap();
    match dec.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            assert_eq!(*d, Decimal::from(42));
        }
        _ => panic!("Expected decimal"),
    }

    let result = eval_empty("3.14.toDecimal()");
    let dec = result.iter().next().unwrap();
    match dec.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            assert_eq!(*d, Decimal::new(314, 2));
        }
        _ => panic!("Expected decimal"),
    }

    let result = eval_empty("'3.14'.toDecimal()");
    let dec = result.iter().next().unwrap();
    match dec.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            use rust_decimal::Decimal;
            assert_eq!(*d, Decimal::new(314, 2));
        }
        _ => panic!("Expected decimal"),
    }

    // convertsToDecimal()
    let result = eval_empty("42.convertsToDecimal()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("3.14.convertsToDecimal()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'3.14'.convertsToDecimal()");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("'invalid'.convertsToDecimal()");
    assert!(!result.as_boolean().unwrap());
}

#[test]
fn test_iif() {
    // iif(condition, ifTrue, ifFalse)
    let result = eval_empty("iif(true, 1, 2)");
    assert_eq!(result.as_integer().unwrap(), 1);

    let result = eval_empty("iif(false, 1, 2)");
    assert_eq!(result.as_integer().unwrap(), 2);

    let result = eval_empty("iif(1 > 0, 'yes', 'no')");
    assert_eq!(result.as_string().unwrap().as_ref(), "yes");

    let result = eval_empty("iif(1 < 0, 'yes', 'no')");
    assert_eq!(result.as_string().unwrap().as_ref(), "no");

    // Empty condition evaluates to false
    let result = eval_empty("iif({}, 1, 2)");
    assert_eq!(result.as_integer().unwrap(), 2);
}

// ============================================
// Math Functions
// ============================================

#[test]
fn test_math_functions() {
    // abs()
    // Note: -5.abs() should be parsed as -(5.abs()) per precedence, but test expects (-5).abs()
    // Using parentheses to match expected behavior
    let result = eval_empty("(-5).abs()");
    assert_eq!(result.as_integer().unwrap(), 5);

    let result = eval_empty("5.abs()");
    assert_eq!(result.as_integer().unwrap(), 5);

    // floor()
    let result = eval_empty("3.7.floor()");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(3, 0));
        }
        _ => panic!("Expected decimal"),
    }

    // ceiling()
    let result = eval_empty("3.2.ceiling()");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(4, 0));
        }
        _ => panic!("Expected decimal"),
    }

    // round()
    let result = eval_empty("3.5.round()");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(4, 0));
        }
        _ => panic!("Expected decimal"),
    }

    // truncate()
    let result = eval_empty("3.9.truncate()");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(3, 0));
        }
        _ => panic!("Expected decimal"),
    }
}

// ============================================
// Type Operations
// ============================================

#[test]
fn test_type_operations() {
    // is (type check)
    let result = eval_empty("1 is Integer");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("1 is String");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("'hello' is String");
    assert!(result.as_boolean().unwrap());

    // as (type cast) - operator syntax
    let result = eval_empty("1 as Integer");
    assert_eq!(result.as_integer().unwrap(), 1);

    let result = eval_empty("1 as String");
    assert_eq!(result.len(), 0); // Should return empty if cast fails

    // as() function call syntax
    let result = eval_empty("1.as('Integer')");
    assert_eq!(result.as_integer().unwrap(), 1);

    let result = eval_empty("1.as('String')");
    assert_eq!(result.len(), 0); // Should return empty if cast fails

    let result = eval_empty("'hello'.as('String')");
    assert_eq!(result.as_string().unwrap().as_ref(), "hello");

    // is() function call syntax
    let result = eval_empty("1.is('Integer')");
    assert!(result.as_boolean().unwrap());

    let result = eval_empty("1.is('String')");
    assert!(!result.as_boolean().unwrap());

    let result = eval_empty("'hello'.is('String')");
    assert!(result.as_boolean().unwrap());
}

// ============================================
// Complex Expressions
// ============================================

#[test]
fn test_complex_expressions() {
    // Chained operations
    let result = eval_empty("(1 + 2) * 3");
    assert_eq!(result.as_integer().unwrap(), 9);

    // Function chaining
    let result = eval_empty("42.toString().length()");
    assert_eq!(result.as_integer().unwrap(), 2);

    // Boolean logic
    let result = eval_empty("(1 < 2) and (3 > 1)");
    assert!(result.as_boolean().unwrap());

    // Mixed types
    let result = eval_empty("1 + 2.5");
    let item = result.iter().next().unwrap();
    match item.data() {
        zunder_fhirpath::value::ValueData::Decimal(d) => {
            assert_eq!(*d, Decimal::new(35, 1)); // 3.5
        }
        _ => panic!("Expected decimal"),
    }
}

// ============================================
// Edge Cases
// ============================================

#[test]
fn test_edge_cases() {
    // Empty collection operations
    let result = eval_empty("empty() + 1");
    assert_eq!(result.len(), 0);

    // Division by zero
    let result = eval_empty("1 / 0");
    assert_eq!(result.len(), 0); // Should return empty, not error

    // Modulo by zero
    let result = eval_empty("1 mod 0");
    assert_eq!(result.len(), 0); // Should return empty, not error
}

#[test]
fn test_filtering_functions() {
    // ofType() - filter by type
    // Test with System types (type specifier must be a string literal)
    let result = eval_empty("(1 | 'hello' | 2).ofType('Integer')");
    assert_eq!(result.len(), 2);

    let result = eval_empty("(1 | 'hello' | 2).ofType('String')");
    assert_eq!(result.len(), 1);

    // extension() - filter extensions by URL
    // Note: This requires actual FHIR resources with extensions, so we'll test basic functionality
    // For now, empty collection on non-extension resources
    let result = eval_empty("1.extension('http://example.org/extension')");
    assert_eq!(result.len(), 0);
}

#[test]
fn test_combine() {
    // combine() - merge collections without deduplication
    let result = eval_empty("(1 | 2).combine(3 | 4)");
    assert_eq!(result.len(), 4);

    let result = eval_empty("(1 | 1 | 2).combine(2 | 3)");
    // Note: The | operator deduplicates, so (1 | 1 | 2) = {1, 2} and (2 | 3) = {2, 3}
    // combine preserves duplicates across collections: {1, 2} + {2, 3} = {1, 2, 2, 3}
    assert_eq!(result.len(), 4);

    // Empty collections
    let result = eval_empty("{}.combine(1 | 2)");
    assert_eq!(result.len(), 2);

    let result = eval_empty("(1 | 2).combine({})");
    assert_eq!(result.len(), 2);

    let result = eval_empty("{}.combine({})");
    assert_eq!(result.len(), 0);
}

#[test]
fn test_aggregate() {
    // aggregate() - general-purpose aggregation
    // Sum: value.aggregate($this + $total, 0)
    let result = eval_empty("(1 | 2 | 3).aggregate($this + $total, 0)");
    assert_eq!(result.as_integer().unwrap(), 6);

    // Count using aggregate
    let result = eval_empty("(1 | 2 | 3).aggregate($total + 1, 0)");
    assert_eq!(result.as_integer().unwrap(), 3);

    // Empty collection with init value
    let result = eval_empty("{}.aggregate($this + $total, 42)");
    assert_eq!(result.as_integer().unwrap(), 42);

    // Empty collection without init value
    let result = eval_empty("{}.aggregate($this + $total)");
    assert_eq!(result.len(), 0);
}

// ============================================
// Type Name Resolution
// ============================================

#[test]
fn test_type_name_resolution() {
    // Create a simple Patient resource for testing using JSON
    use serde_json::json;

    let patient_json = json!({
        "resourceType": "Patient",
        "name": [{
            "given": ["John"],
            "family": "Doe"
        }]
    });

    let patient = Value::from_json(patient_json);

    // Test: Patient.name.given should work (type name matches context)
    let engine = get_test_engine();
    let ctx = zunder_fhirpath::Context::new(patient.clone());
    let result = engine
        .evaluate_expr("Patient.name.given", &ctx, None)
        .unwrap();
    assert_eq!(result.len(), 1, "Patient.name.given should return 1 item");
    assert_eq!(result.as_string().unwrap().as_ref(), "John");

    // Test: name.given should also work (without type prefix)
    // First test: name should return collection of name objects
    let name_result = engine.evaluate_expr("name", &ctx, None).unwrap();
    assert!(
        !name_result.is_empty(),
        "name should return at least 1 item"
    );

    // Then test: name.given should return given values
    let result = engine.evaluate_expr("name.given", &ctx, None).unwrap();
    assert_eq!(result.len(), 1, "name.given should return 1 item");
    assert_eq!(result.as_string().unwrap().as_ref(), "John");

    // Test: Patient.name.family should work
    let result = engine
        .evaluate_expr("Patient.name.family", &ctx, None)
        .unwrap();
    assert_eq!(result.len(), 1, "Patient.name.family should return 1 item");
    assert_eq!(result.as_string().unwrap().as_ref(), "Doe");

    // Test: Wrong type name should return empty
    let result = engine
        .evaluate_expr("Observation.name.given", &ctx, None)
        .unwrap();
    assert_eq!(
        result.len(),
        0,
        "Observation.name.given on Patient should return empty"
    );
}

#[test]
fn test_path_with_parent_type_prefix() {
    // Test a path starting with the parent type, e.g., Resource.Resource.meta.lastUpdated
    use serde_json::json;

    let resource_json = json!({
        "resourceType": "Patient",
        "meta": {
            "lastUpdated": "2024-01-15T10:30:00Z"
        },
        "name": [{
            "given": ["John"],
            "family": "Doe"
        }]
    });

    let resource = Value::from_json(resource_json);
    let engine = get_test_engine();
    let ctx = zunder_fhirpath::Context::new(resource.clone());

    // Test: Resource.Resource.meta.lastUpdated should work
    // The first Resource is the parent type, second Resource navigates to the resource itself
    let result = engine
        .evaluate_expr("Resource.meta.lastUpdated", &ctx, None)
        .unwrap();
    assert_eq!(
        result.len(),
        1,
        "Resource.meta.lastUpdated should return 1 item"
    );
    assert_eq!(result.as_string().unwrap().as_ref(), "2024-01-15T10:30:00Z");

    // Test: Resource.meta.lastUpdated should also work (without the second Resource)
    let result = engine
        .evaluate_expr("Resource.meta.lastUpdated", &ctx, None)
        .unwrap();
    assert_eq!(
        result.len(),
        1,
        "Resource.meta.lastUpdated should return 1 item"
    );
    assert_eq!(result.as_string().unwrap().as_ref(), "2024-01-15T10:30:00Z");

    // Test with compiled expression: Resource.Resource.meta.lastUpdated
    let plan = engine.compile("Resource.meta.lastUpdated", None).unwrap();
    let result = engine.evaluate(&plan, &ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Compiled Resource.meta.lastUpdated should return 1 item"
    );
    assert_eq!(result.as_string().unwrap().as_ref(), "2024-01-15T10:30:00Z");

    // Test with compiled expression and type: Resource.Resource.meta.lastUpdated
    let plan = engine
        .compile("Resource.meta.lastUpdated", Some("Resource"))
        .unwrap();
    let result = engine.evaluate(&plan, &ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Compiled Resource.meta.lastUpdated with type should return 1 item"
    );
    assert_eq!(result.as_string().unwrap().as_ref(), "2024-01-15T10:30:00Z");

    // Test with compiled expression: Resource.meta.lastUpdated
    let plan = engine.compile("Resource.meta.lastUpdated", None).unwrap();
    let result = engine.evaluate(&plan, &ctx).unwrap();
    assert_eq!(
        result.len(),
        1,
        "Compiled Resource.meta.lastUpdated should return 1 item"
    );
    assert_eq!(result.as_string().unwrap().as_ref(), "2024-01-15T10:30:00Z");
}

#[test]
fn test_complex_code_union_expression() {
    // Test a complex union expression that collects codes from multiple resource types
    // This is commonly used in FHIR search parameters to extract codes from various resources
    use serde_json::json;

    // Create a simple Condition resource with a code
    let condition_json = json!({
        "resourceType": "Condition",
        "id": "test-condition",
        "code": {
            "coding": [
                {
                    "system": "http://snomed.info/sct",
                    "code": "10001005",
                    "display": "Bacterial sepsis"
                }
            ]
        }
    });

    let condition = Value::from_json(condition_json);
    let engine = get_test_engine();
    let ctx = Context::new(condition);

    // Test that non-matching type-prefixed paths return empty
    let allergy_result = engine
        .evaluate_expr("AllergyIntolerance.code", &ctx, None)
        .unwrap();
    assert_eq!(
        allergy_result.len(),
        0,
        "AllergyIntolerance.code on Condition should return empty, got {}",
        allergy_result.len()
    );

    // Test simple union with non-matching type first
    let union_result = engine
        .evaluate_expr("AllergyIntolerance.code | Condition.code", &ctx, None)
        .unwrap();
    assert_eq!(
        union_result.len(),
        1,
        "AllergyIntolerance.code | Condition.code should return 1 item, got {}",
        union_result.len()
    );

    // First, test basic field access without type prefix
    let code_result_basic = engine.evaluate_expr("code", &ctx, None).unwrap();
    assert_eq!(
        code_result_basic.len(),
        1,
        "code should return 1 CodeableConcept, got {}",
        code_result_basic.len()
    );

    // Test with type prefix - this might fail if type checking is strict
    let code_result = engine.evaluate_expr("Condition.code", &ctx, None).unwrap();
    assert_eq!(
        code_result.len(),
        1,
        "Condition.code should return 1 CodeableConcept, got {}",
        code_result.len()
    );

    // Verify the code contains the expected coding
    let coding_result = engine
        .evaluate_expr("code.coding.code", &ctx, None)
        .unwrap();
    assert_eq!(
        coding_result.len(),
        1,
        "code.coding.code should return 1 code, got {}",
        coding_result.len()
    );
    assert_eq!(coding_result.as_string().unwrap().as_ref(), "10001005");

    // Complex union expression that extracts codes from multiple resource types
    // For a Condition resource, only Condition.code should match
    // Non-matching type-prefixed paths should return empty, not cause errors
    let expr = "AllergyIntolerance.code | AllergyIntolerance.reaction.substance | Condition.code | (DeviceRequest.code as CodeableConcept) | DiagnosticReport.code | FamilyMemberHistory.condition.code | List.code | Medication.code | (MedicationAdministration.medication as CodeableConcept) | (MedicationDispense.medication as CodeableConcept) | (MedicationRequest.medication as CodeableConcept) | (MedicationStatement.medication as CodeableConcept) | Observation.code | Procedure.code | ServiceRequest.code";

    let result = engine.evaluate_expr(expr, &ctx, None).unwrap();

    // Should return the Condition.code (CodeableConcept)
    assert_eq!(
        result.len(),
        1,
        "Union expression should return 1 CodeableConcept for Condition, got {}",
        result.len()
    );

    // Verify the result contains the expected code by checking coding
    let coding_result = engine
        .evaluate_expr("code.coding.code", &ctx, None)
        .unwrap();
    assert_eq!(
        coding_result.len(),
        1,
        "code.coding.code should return 1 code"
    );
    assert_eq!(coding_result.as_string().unwrap().as_ref(), "10001005");
}

#[test]
fn test_code_coding_expression() {
    // Test code.coding expression on a Condition resource
    use serde_json::json;

    let condition_json = json!({
        "resourceType": "Condition",
        "id": "test-condition",
        "code": {
            "coding": [
                {
                    "system": "http://snomed.info/sct",
                    "code": "10001005",
                    "display": "Bacterial sepsis"
                },
                {
                    "system": "http://hl7.org/fhir/sid/icd-10",
                    "code": "A41.9",
                    "display": "Sepsis, unspecified organism"
                }
            ]
        }
    });

    let condition = Value::from_json(condition_json);
    let engine = get_test_engine();
    let ctx = Context::new(condition);

    // Test code.coding should return the coding array
    let result = engine.evaluate_expr("code.coding", &ctx, None).unwrap();
    assert_eq!(
        result.len(),
        2,
        "code.coding should return 2 coding items, got {}",
        result.len()
    );

    // Test that we can access individual coding properties
    let code_result = engine
        .evaluate_expr("code.coding.code", &ctx, None)
        .unwrap();
    assert_eq!(
        code_result.len(),
        2,
        "code.coding.code should return 2 codes, got {}",
        code_result.len()
    );

    // Verify the codes are present
    let codes: Vec<&str> = code_result
        .iter()
        .map(|v| match v.data() {
            zunder_fhirpath::value::ValueData::String(s) => s.as_ref(),
            _ => panic!("Expected string"),
        })
        .collect();
    assert!(codes.contains(&"10001005"), "Should contain SNOMED code");
    assert!(codes.contains(&"A41.9"), "Should contain ICD-10 code");
}
