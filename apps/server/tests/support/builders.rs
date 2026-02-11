use axum::body::Bytes;
use serde_json::json;

/// Converts a JSON value to request body bytes
pub fn to_json_body(value: &serde_json::Value) -> anyhow::Result<Bytes> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}

/// Builder for Patient resources
pub struct PatientBuilder {
    id: Option<String>,
    active: Option<bool>,
    family: Option<String>,
    given: Vec<String>,
    gender: Option<String>,
    identifiers: Vec<serde_json::Value>,
}

impl PatientBuilder {
    pub fn new() -> Self {
        Self {
            id: None,
            active: None,
            family: None,
            given: Vec::new(),
            gender: None,
            identifiers: Vec::new(),
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = Some(active);
        self
    }

    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    pub fn given(mut self, given: impl Into<String>) -> Self {
        self.given.push(given.into());
        self
    }

    pub fn gender(mut self, gender: impl Into<String>) -> Self {
        self.gender = Some(gender.into());
        self
    }

    pub fn identifier(mut self, system: impl Into<String>, value: impl Into<String>) -> Self {
        self.identifiers.push(json!({
            "system": system.into(),
            "value": value.into()
        }));
        self
    }

    pub fn identifier_with_type(
        mut self,
        type_system: impl Into<String>,
        type_code: impl Into<String>,
        system: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.identifiers.push(json!({
            "type": {
                "coding": [{
                    "system": type_system.into(),
                    "code": type_code.into()
                }]
            },
            "system": system.into(),
            "value": value.into()
        }));
        self
    }

    pub fn build(self) -> serde_json::Value {
        let mut patient = json!({
            "resourceType": "Patient"
        });

        if let Some(id) = self.id {
            patient["id"] = json!(id);
        }

        if let Some(active) = self.active {
            patient["active"] = json!(active);
        }

        if self.family.is_some() || !self.given.is_empty() {
            let mut name = json!({});
            if let Some(family) = self.family {
                name["family"] = json!(family);
            }
            if !self.given.is_empty() {
                name["given"] = json!(self.given);
            }
            patient["name"] = json!([name]);
        }

        if let Some(gender) = self.gender {
            patient["gender"] = json!(gender);
        }

        if !self.identifiers.is_empty() {
            patient["identifier"] = json!(self.identifiers);
        }

        patient
    }
}

impl Default for PatientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for Observation resources
pub struct ObservationBuilder {
    id: Option<String>,
    status: String,
    code_text: Option<String>,
    code_coding: Option<serde_json::Value>,
    subject_ref: Option<String>,
    subject_identifier: Option<serde_json::Value>,
    subject_display: Option<String>,
}

impl ObservationBuilder {
    pub fn new() -> Self {
        Self {
            id: None,
            status: "final".to_string(),
            code_text: None,
            code_coding: None,
            subject_ref: None,
            subject_identifier: None,
            subject_display: None,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    pub fn code_text(mut self, text: impl Into<String>) -> Self {
        self.code_text = Some(text.into());
        self
    }

    pub fn code_coding(
        mut self,
        system: impl Into<String>,
        code: impl Into<String>,
        display: Option<impl Into<String>>,
    ) -> Self {
        let mut coding = json!({
            "system": system.into(),
            "code": code.into()
        });
        if let Some(display) = display {
            coding["display"] = json!(display.into());
        }
        self.code_coding = Some(coding);
        self
    }

    pub fn subject(mut self, reference: impl Into<String>) -> Self {
        self.subject_ref = Some(reference.into());
        self
    }

    pub fn subject_with_identifier(
        mut self,
        reference: impl Into<String>,
        system: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.subject_ref = Some(reference.into());
        self.subject_identifier = Some(json!({
            "system": system.into(),
            "value": value.into()
        }));
        self
    }

    pub fn subject_with_display(
        mut self,
        reference: impl Into<String>,
        display: impl Into<String>,
    ) -> Self {
        self.subject_ref = Some(reference.into());
        self.subject_display = Some(display.into());
        self
    }

    pub fn build(self) -> serde_json::Value {
        let mut obs = json!({
            "resourceType": "Observation",
            "status": self.status
        });

        if let Some(id) = self.id {
            obs["id"] = json!(id);
        }

        let mut code = json!({});
        if let Some(text) = self.code_text {
            code["text"] = json!(text);
        }
        if let Some(coding) = self.code_coding {
            code["coding"] = json!([coding]);
        }
        obs["code"] = code;

        if let Some(subject_ref) = self.subject_ref {
            let mut subject = json!({
                "reference": subject_ref
            });
            if let Some(identifier) = self.subject_identifier {
                subject["identifier"] = identifier;
            }
            if let Some(display) = self.subject_display {
                subject["display"] = json!(display);
            }
            obs["subject"] = subject;
        }

        obs
    }
}

impl Default for ObservationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for Condition resources
pub struct ConditionBuilder {
    id: Option<String>,
    subject_ref: Option<String>,
    code_text: Option<String>,
    code_system: Option<String>,
    code_code: Option<String>,
    code_display: Option<String>,
}

impl ConditionBuilder {
    pub fn new() -> Self {
        Self {
            id: None,
            subject_ref: None,
            code_text: None,
            code_system: None,
            code_code: None,
            code_display: None,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn subject(mut self, reference: impl Into<String>) -> Self {
        self.subject_ref = Some(reference.into());
        self
    }

    pub fn code_text(mut self, text: impl Into<String>) -> Self {
        self.code_text = Some(text.into());
        self
    }

    pub fn code_coding(
        mut self,
        system: impl Into<String>,
        code: impl Into<String>,
        display: impl Into<String>,
    ) -> Self {
        self.code_system = Some(system.into());
        self.code_code = Some(code.into());
        self.code_display = Some(display.into());
        self
    }

    pub fn build(self) -> serde_json::Value {
        let mut condition = json!({
            "resourceType": "Condition"
        });

        if let Some(id) = self.id {
            condition["id"] = json!(id);
        }

        if let Some(subject_ref) = self.subject_ref {
            condition["subject"] = json!({
                "reference": subject_ref
            });
        }

        let mut code = json!({});
        if let Some(text) = self.code_text {
            code["text"] = json!(text);
        }
        if let (Some(system), Some(code_val), Some(display)) =
            (self.code_system, self.code_code, self.code_display)
        {
            code["coding"] = json!([{
                "system": system,
                "code": code_val,
                "display": display
            }]);
        }
        condition["code"] = code;

        condition
    }
}

impl Default for ConditionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patient_builder_minimal() {
        let patient = PatientBuilder::new().family("Smith").build();
        assert_eq!(patient["resourceType"], "Patient");
        assert_eq!(patient["name"][0]["family"], "Smith");
    }

    #[test]
    fn patient_builder_full() {
        let patient = PatientBuilder::new()
            .id("123")
            .active(true)
            .family("Smith")
            .given("John")
            .given("Adam")
            .gender("male")
            .identifier("http://example.org/mrn", "12345")
            .build();

        assert_eq!(patient["id"], "123");
        assert_eq!(patient["active"], true);
        assert_eq!(patient["name"][0]["family"], "Smith");
        assert_eq!(patient["name"][0]["given"][0], "John");
        assert_eq!(patient["name"][0]["given"][1], "Adam");
        assert_eq!(patient["gender"], "male");
        assert_eq!(patient["identifier"][0]["system"], "http://example.org/mrn");
        assert_eq!(patient["identifier"][0]["value"], "12345");
    }

    #[test]
    fn observation_builder_basic() {
        let obs = ObservationBuilder::new()
            .code_text("Weight")
            .subject("Patient/123")
            .build();

        assert_eq!(obs["resourceType"], "Observation");
        assert_eq!(obs["status"], "final");
        assert_eq!(obs["code"]["text"], "Weight");
        assert_eq!(obs["subject"]["reference"], "Patient/123");
    }
}
