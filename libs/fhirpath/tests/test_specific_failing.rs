//! Test specific failing cases from HL7 suite

use serde_json::Value as JsonValue;
use ferrum_fhirpath::{Context, Value};

mod test_support;

#[test]
fn test_polymorphism_is_b() {
    let engine = test_support::engine_r5();

    // Load observation-example.json
    let json_str = std::fs::read_to_string("tests/examples/observation-example.json").unwrap();
    let json: JsonValue = serde_json::from_str(&json_str).unwrap();
    let resource = Value::from_json(json.clone());

    // Test: Observation.value.is(Period).not()
    let ctx = Context::new(resource.clone());

    // First check what Observation.value is
    let value_result = engine
        .evaluate_json("Observation.value", json.clone(), None)
        .unwrap();
    println!("Observation.value = {:?}", value_result);

    // Then check is(Period)
    let is_result = engine
        .evaluate_json("Observation.value.is(Period)", json.clone(), None)
        .unwrap();
    println!("Observation.value.is(Period) = {:?}", is_result);

    // Also check is(Quantity) - should be true
    let is_quantity = engine
        .evaluate_json("Observation.value.is(Quantity)", json, None)
        .unwrap();
    println!("Observation.value.is(Quantity) = {:?}", is_quantity);
    assert!(is_quantity.as_boolean().unwrap(), "Should be Quantity");

    match engine.evaluate_expr("Observation.value.is(Period).not()", &ctx, None) {
        Ok(result) => {
            println!("Result: {:?}", result);
            println!("Length: {}", result.len());
            println!("Can convert to boolean: {}", result.as_boolean().is_ok());
            if let Ok(b) = result.as_boolean() {
                println!("Boolean value: {}", b);
                assert!(b, "Expected true");
            } else {
                println!("Items in result:");
                for (i, item) in result.iter().enumerate() {
                    println!("  [{}]: {:?}", i, item);
                }
                panic!("Cannot convert to boolean");
            }
        }
        Err(e) => {
            panic!("Error: {:?}", e);
        }
    }
}
