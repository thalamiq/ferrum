//! Evaluation context for FHIRPath expressions
//!
//! Context provides access to variables, the current item ($this), and iteration state.

use crate::value::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Evaluation context containing variables and iteration state
#[derive(Clone)]
pub struct Context {
    /// Current item in iteration ($this)
    pub this: Option<Value>,
    /// Current index in iteration ($index)
    pub index: Option<usize>,
    /// Whether to enforce strict semantic validation (invalid paths produce errors)
    pub strict: bool,
    /// Environment variables (%resource, %context, etc.). Note: the lexer drops the leading `%`
    /// when parsing external constants, so runtime lookups typically use the un-prefixed names.
    pub variables: Arc<HashMap<Arc<str>, Value>>,
    /// The root resource being evaluated
    pub resource: Value,
    /// Root container resource (usually same as `resource`)
    pub root: Value,
}

impl Context {
    pub fn new(resource: Value) -> Self {
        let resource_clone = resource.clone();
        Self::new_with_root_resource(resource_clone.clone(), resource_clone)
    }

    /// Create a new context where `%resource` (and `%context`) are evaluated within
    /// a potentially different `%rootResource` container.
    ///
    /// This is primarily used when evaluating expressions against a contained resource:
    /// `%resource` refers to the contained resource, while `%rootResource` refers to the
    /// containing `DomainResource`.
    pub fn new_with_root_resource(resource: Value, root_resource: Value) -> Self {
        let mut variables: HashMap<Arc<str>, Value> = HashMap::new();

        // FHIR-specific variables (FHIRPath): %resource, %rootResource (and %profile in invariants).
        // Note: this engine stores both prefixed and un-prefixed variants.
        Self::insert_variable_pair(&mut variables, "resource", resource.clone());
        Self::insert_variable_pair(&mut variables, "context", resource.clone());
        // Backwards compat: keep the historical %root variable name as an alias for %rootResource.
        Self::insert_variable_pair(&mut variables, "root", root_resource.clone());
        Self::insert_variable_pair(&mut variables, "rootResource", root_resource.clone());

        // Common external constants used by the HL7 test suite
        variables.insert(Arc::from("%sct"), Value::string("http://snomed.info/sct"));
        variables.insert(Arc::from("%loinc"), Value::string("http://loinc.org"));
        variables.insert(
            Arc::from("%ucum"),
            Value::string("http://unitsofmeasure.org"),
        );
        variables.insert(
            Arc::from("%vs-administrative-gender"),
            Value::string("http://hl7.org/fhir/ValueSet/administrative-gender"),
        );
        variables.insert(
            Arc::from("%ext-patient-birthTime"),
            Value::string("http://hl7.org/fhir/StructureDefinition/patient-birthTime"),
        );
        // Allow access without '%' prefix as well
        variables.insert(Arc::from("sct"), Value::string("http://snomed.info/sct"));
        variables.insert(Arc::from("loinc"), Value::string("http://loinc.org"));
        variables.insert(
            Arc::from("ucum"),
            Value::string("http://unitsofmeasure.org"),
        );
        variables.insert(
            Arc::from("vs-administrative-gender"),
            Value::string("http://hl7.org/fhir/ValueSet/administrative-gender"),
        );
        variables.insert(
            Arc::from("ext-patient-birthTime"),
            Value::string("http://hl7.org/fhir/StructureDefinition/patient-birthTime"),
        );

        Self {
            this: None,
            index: None,
            strict: false,
            variables: Arc::new(variables),
            resource,
            root: root_resource,
        }
    }

    fn insert_variable_pair(variables: &mut HashMap<Arc<str>, Value>, name: &str, value: Value) {
        variables.insert(Arc::from(name), value.clone());
        variables.insert(Arc::from(format!("%{}", name)), value);
    }

    /// Enable strict semantic validation (invalid paths produce errors instead of empty collections)
    pub fn with_strict_semantics(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Push a new iteration context with $this and $index
    pub fn push_this(mut self, this: Value) -> Self {
        self.this = Some(this.clone());
        self
    }

    /// Push a new iteration context with $this and $index
    pub fn push_iteration(mut self, this: Value, index: usize) -> Self {
        self.this = Some(this.clone());
        self.index = Some(index);
        self
    }

    /// Get a variable value
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        // Handle special variables
        if name == "$this" {
            return self.this.as_ref();
        }
        if name == "$index" {
            // Return index as integer value
            // TODO: Return index as Value - will be handled in evaluator for now
            return None;
        }

        self.variables.get(name)
    }

    /// Set a variable
    pub fn set_variable(&mut self, name: impl Into<Arc<str>>, value: Value) {
        let name: Arc<str> = name.into();
        let variables = Arc::make_mut(&mut self.variables);
        variables.insert(name.clone(), value.clone());

        // Mirror both naming styles to match how the lexer/parser represent external constants.
        let raw = name.as_ref();
        if let Some(stripped) = raw.strip_prefix('%') {
            variables.insert(Arc::from(stripped), value);
        } else {
            variables.insert(Arc::from(format!("%{}", raw)), value);
        }
    }

    /// Convenience setter for the FHIRPath `%profile` variable (used in profile invariants).
    pub fn with_profile(mut self, canonical_url: impl Into<Arc<str>>) -> Self {
        self.set_variable("profile", Value::string(canonical_url));
        self
    }
}
