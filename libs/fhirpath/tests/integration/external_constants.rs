use serde_json::json;
use ferrum_fhirpath::{Context, Engine, Value};

#[path = "../test_support/mod.rs"]
mod test_support;

fn get_test_engine() -> &'static Engine {
    test_support::engine_r5()
}

#[test]
fn resolves_external_context_constants() {
    let resource = Value::from_json(json!({"resourceType": "Patient", "id": "p1"}));
    let ctx = Context::new(resource.clone());
    let engine = get_test_engine();

    let result = engine
        .evaluate_expr("%resource", &ctx, None)
        .expect("evaluation failed");
    assert_eq!(result.len(), 1);
    assert_eq!(result.iter().next().unwrap(), &resource);

    let ctx_result = engine
        .evaluate_expr("%context", &ctx, None)
        .expect("evaluation failed");
    assert_eq!(ctx_result.len(), 1);
    assert_eq!(ctx_result.iter().next().unwrap(), &resource);
}

#[test]
fn resolves_root_resource_and_profile() {
    let root_resource = Value::from_json(json!({"resourceType": "Patient", "id": "root"}));
    let contained_resource =
        Value::from_json(json!({"resourceType": "Observation", "id": "contained"}));

    let mut ctx =
        Context::new_with_root_resource(contained_resource.clone(), root_resource.clone());
    ctx.set_variable(
        "%profile",
        Value::string("http://example.org/StructureDefinition/test"),
    );

    let engine = get_test_engine();

    let resource_result = engine
        .evaluate_expr("%resource", &ctx, None)
        .expect("evaluation failed");
    assert_eq!(resource_result.len(), 1);
    assert_eq!(resource_result.iter().next().unwrap(), &contained_resource);

    let root_result = engine
        .evaluate_expr("%rootResource", &ctx, None)
        .expect("evaluation failed");
    assert_eq!(root_result.len(), 1);
    assert_eq!(root_result.iter().next().unwrap(), &root_resource);

    // Backwards-compat alias.
    let legacy_root_result = engine
        .evaluate_expr("%root", &ctx, None)
        .expect("evaluation failed");
    assert_eq!(legacy_root_result.len(), 1);
    assert_eq!(legacy_root_result.iter().next().unwrap(), &root_resource);

    let profile_result = engine
        .evaluate_expr("%profile", &ctx, None)
        .expect("evaluation failed");
    assert_eq!(profile_result.len(), 1);
    assert_eq!(
        profile_result.iter().next().unwrap(),
        &Value::string("http://example.org/StructureDefinition/test")
    );
}
