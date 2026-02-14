#[path = "../test_support/mod.rs"]
mod test_support;

#[test]
fn test_date_equality() {
    use serde_json::json;
    use ferrum_fhirpath::{Context, Value};

    let patient = json!({
      "resourceType": "Patient",
      "birthDate": "1974-12-25"
    });

    let engine = test_support::engine_r5();
    let resource = Value::from_json(patient);
    let ctx = Context::new(resource);

    let r1 = engine
        .evaluate_expr("Patient.birthDate", &ctx, None)
        .unwrap();
    println!(
        "birthDate: {} items, type: {:?}",
        r1.len(),
        r1.iter().next().map(|v| format!("{:?}", v.data()))
    );

    let r2 = engine.evaluate_expr("@1974-12-25", &ctx, None).unwrap();
    println!(
        "@1974-12-25: {} items, type: {:?}",
        r2.len(),
        r2.iter().next().map(|v| format!("{:?}", v.data()))
    );

    let r3 = engine
        .evaluate_expr("Patient.birthDate = @1974-12-25", &ctx, None)
        .unwrap();
    println!("birthDate = @1974-12-25: {} items", r3.len());
    if !r3.is_empty() {
        println!("Result: {:?}", r3.iter().next().map(|v| v.data()));
    }
}
