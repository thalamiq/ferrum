//! Integration tests for XML support
//!
//! Run with: cargo test --test test_xml_support --features xml-support

mod test_support;

#[cfg(feature = "xml-support")]
#[test]
fn test_evaluate_xml_basic() {
    use zunder_context::DefaultFhirContext;
    use zunder_fhirpath::Engine;

    let context = test_support::context_r4().clone();
    let engine = Engine::new(context, None);

    let xml = r#"<Patient xmlns="http://hl7.org/fhir">
        <id value="pat-1"/>
        <active value="true"/>
        <birthDate value="1974-12-25"/>
    </Patient>"#;

    // Test basic field access
    let result = engine.evaluate_xml("Patient.id", xml, None).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_string().unwrap().as_ref(), "pat-1");

    // Test boolean field
    let result = engine.evaluate_xml("Patient.active", xml, None).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result.as_boolean().unwrap());

    // Test date field
    let result = engine.evaluate_xml("Patient.birthDate", xml, None).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_string().unwrap().as_ref(), "1974-12-25");
}

#[cfg(feature = "xml-support")]
#[test]
fn test_evaluate_xml_with_type() {
    use zunder_context::DefaultFhirContext;
    use zunder_fhirpath::Engine;

    let context = test_support::context_r4().clone();
    let engine = Engine::new(context, None);

    let xml = r#"<Patient xmlns="http://hl7.org/fhir">
        <name>
            <family value="Doe"/>
            <given value="John"/>
        </name>
    </Patient>"#;

    // Test with type validation
    let result = engine
        .evaluate_xml("name.family", xml, Some("Patient"))
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_string().unwrap().as_ref(), "Doe");

    let result = engine
        .evaluate_xml("name.given", xml, Some("Patient"))
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.as_string().unwrap().as_ref(), "John");
}

#[cfg(feature = "xml-support")]
#[test]
fn test_evaluate_xml_array() {
    use zunder_context::DefaultFhirContext;
    use zunder_fhirpath::Engine;

    let context = test_support::context_r4().clone();
    let engine = Engine::new(context, None);

    let xml = r#"<Patient xmlns="http://hl7.org/fhir">
        <telecom>
            <system value="phone"/>
            <value value="555-1234"/>
        </telecom>
        <telecom>
            <system value="email"/>
            <value value="john@example.com"/>
        </telecom>
    </Patient>"#;

    // Test array access
    let result = engine
        .evaluate_xml("Patient.telecom.system", xml, None)
        .unwrap();
    assert_eq!(result.len(), 2);
}

#[cfg(feature = "xml-support")]
#[test]
fn test_evaluate_xml_invalid() {
    use zunder_context::DefaultFhirContext;
    use zunder_fhirpath::Engine;

    let context = test_support::context_r4().clone();
    let engine = Engine::new(context, None);

    let invalid_xml = r#"<Patient xmlns="http://hl7.org/fhir">
        <id value="pat-1"
    </Patient>"#;

    // Should return an error for invalid XML
    let result = engine.evaluate_xml("Patient.id", invalid_xml, None);
    assert!(result.is_err());
}

#[cfg(not(feature = "xml-support"))]
#[test]
fn test_xml_support_disabled() {
    // When xml-support feature is not enabled, the evaluate_xml methods
    // should not be available. This test just ensures compilation succeeds
    // without the feature.
    assert!(true);
}
