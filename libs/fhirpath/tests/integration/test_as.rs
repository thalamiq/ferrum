#[path = "../test_support/mod.rs"]
mod test_support;

#[test]
fn test_as_quantity() {
    use serde_json::json;
    use zunder_fhirpath::{Context, Value};

    let obs = json!({
      "resourceType": "Observation",
      "valueQuantity": {
        "value": 185,
        "unit": "lbs",
        "system": "http://unitsofmeasure.org",
        "code": "[lb_av]"
      }
    });

    let engine = test_support::engine_r5();
    let resource = Value::from_json(obs);
    let ctx = Context::new(resource);

    // Test parts
    let r1 = engine
        .evaluate_expr("Observation.valueQuantity", &ctx, None)
        .unwrap();
    println!("valueQuantity: {} items", r1.len());

    let r2 = engine
        .evaluate_expr("Observation.value", &ctx, None)
        .unwrap();
    println!("value: {} items", r2.len());

    let r3 = engine
        .evaluate_expr("Observation.value is Quantity", &ctx, None)
        .unwrap();
    println!("value is Quantity: {:?}", r3.as_boolean().ok());

    let r4 = engine
        .evaluate_expr("Observation.value.as(Quantity)", &ctx, None)
        .unwrap();
    println!("value.as(Quantity): {} items", r4.len());

    if !r4.is_empty() {
        let r5 = engine
            .evaluate_expr("Observation.value.as(Quantity).unit", &ctx, None)
            .unwrap();
        if !r5.is_empty() {
            println!("unit: {:?}", r5.as_string().ok());
        } else {
            println!("unit: empty");
        }
    }

    assert!(!r4.is_empty(), "as(Quantity) should return the item");
}

#[test]
fn test_as_with_multi_item_collection() {
    use serde_json::json;
    use zunder_fhirpath::{Context, Value};

    // Test case for FHIR R4 search parameter compatibility
    // Expression like: (Observation.component.value as Quantity)
    // This should filter multi-item collections, not throw an error
    let obs = json!({
        "resourceType": "Observation",
        "component": [
            {
                "code": {"text": "Systolic BP"},
                "valueQuantity": {
                    "value": 120,
                    "unit": "mmHg",
                    "system": "http://unitsofmeasure.org",
                    "code": "mm[Hg]"
                }
            },
            {
                "code": {"text": "Diastolic BP"},
                "valueQuantity": {
                    "value": 80,
                    "unit": "mmHg",
                    "system": "http://unitsofmeasure.org",
                    "code": "mm[Hg]"
                }
            },
            {
                "code": {"text": "Comment"},
                "valueString": "Normal reading"
            }
        ]
    });

    let engine = test_support::engine_r5();
    let resource = Value::from_json(obs);
    let ctx = Context::new(resource);

    // Test FHIR R4 search parameter expression pattern
    // This should NOT error on multi-item collection, but filter to matching types
    let result = engine
        .evaluate_expr("Observation.component.value.as(Quantity)", &ctx, None)
        .unwrap();

    // Should return 2 Quantity values (systolic and diastolic), filtering out the string
    assert_eq!(
        result.len(),
        2,
        "as(Quantity) should filter and return 2 Quantity items"
    );

    // Verify we can chain operations on the filtered result
    let units = engine
        .evaluate_expr("Observation.component.value.as(Quantity).unit", &ctx, None)
        .unwrap();
    assert_eq!(units.len(), 2, "Should get units from both quantities");

    // Test with string type - should filter to only string values
    let strings = engine
        .evaluate_expr("Observation.component.value.as(string)", &ctx, None)
        .unwrap();
    assert!(
        strings.len() >= 1,
        "as(string) should return at least the string value"
    );

    // Test combined expression like in FHIR R4 search parameters
    let combo = engine
        .evaluate_expr(
            "(Observation.component.value as Quantity) | (Observation.component.value as string)",
            &ctx,
            None,
        )
        .unwrap();
    assert_eq!(
        combo.len(),
        3,
        "Combined expression should return all 3 component values"
    );
}
