use super::builders::{ConditionBuilder, ObservationBuilder, PatientBuilder};
use serde_json::Value;

/// Common test constants
pub mod constants {
    pub const MRN_SYSTEM: &str = "http://example.org/fhir/mrn";
    pub const SNOMED_SYSTEM: &str = "http://snomed.info/sct";
    pub const LOINC_SYSTEM: &str = "http://loinc.org";
    pub const V2_0203_SYSTEM: &str = "http://terminology.hl7.org/CodeSystem/v2-0203";
    pub const MR_CODE: &str = "MR"; // Medical Record Number
}

/// Creates a minimal valid Patient resource
pub fn minimal_patient() -> Value {
    PatientBuilder::new().family("Doe").build()
}

/// Creates a Patient with common test data
pub fn example_patient(family: &str, given: &str) -> Value {
    PatientBuilder::new()
        .active(true)
        .family(family)
        .given(given)
        .build()
}

/// Creates a Patient with MRN identifier
pub fn patient_with_mrn(family: &str, mrn: &str) -> Value {
    PatientBuilder::new()
        .active(true)
        .family(family)
        .identifier(constants::MRN_SYSTEM, mrn)
        .build()
}

/// Creates a Patient with typed identifier
pub fn patient_with_typed_identifier(
    family: &str,
    type_code: &str,
    id_system: &str,
    id_value: &str,
) -> Value {
    PatientBuilder::new()
        .active(true)
        .family(family)
        .identifier_with_type(constants::V2_0203_SYSTEM, type_code, id_system, id_value)
        .build()
}

/// Creates a minimal Observation
pub fn minimal_observation(patient_id: &str) -> Value {
    ObservationBuilder::new()
        .code_text("test")
        .subject(format!("Patient/{}", patient_id))
        .build()
}

/// Creates an Observation with LOINC code
pub fn observation_with_loinc(patient_id: &str, loinc_code: &str, display: &str) -> Value {
    ObservationBuilder::new()
        .code_coding(
            constants::LOINC_SYSTEM,
            loinc_code,
            Some(display.to_string()),
        )
        .subject(format!("Patient/{}", patient_id))
        .build()
}

/// Creates an Observation with subject.identifier
pub fn observation_with_subject_identifier(
    patient_id: &str,
    identifier_system: &str,
    identifier_value: &str,
) -> Value {
    ObservationBuilder::new()
        .code_text("test")
        .subject_with_identifier(
            format!("Patient/{}", patient_id),
            identifier_system,
            identifier_value,
        )
        .build()
}

/// Creates an Observation with subject.display
pub fn observation_with_subject_display(patient_id: &str, display: &str) -> Value {
    ObservationBuilder::new()
        .code_text("test")
        .subject_with_display(format!("Patient/{}", patient_id), display)
        .build()
}

/// Creates a Condition with SNOMED code
pub fn condition_with_snomed(patient_id: &str, code: &str, display: &str) -> Value {
    ConditionBuilder::new()
        .subject(format!("Patient/{}", patient_id))
        .code_coding(constants::SNOMED_SYSTEM, code, display)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_patient() {
        let patient = minimal_patient();
        assert_eq!(patient["resourceType"], "Patient");
        assert_eq!(patient["name"][0]["family"], "Doe");
    }

    #[test]
    fn test_patient_with_mrn() {
        let patient = patient_with_mrn("Smith", "12345");
        assert_eq!(patient["identifier"][0]["system"], constants::MRN_SYSTEM);
        assert_eq!(patient["identifier"][0]["value"], "12345");
    }

    #[test]
    fn test_observation_with_loinc() {
        let obs = observation_with_loinc("123", "8480-6", "Systolic blood pressure");
        assert_eq!(obs["code"]["coding"][0]["system"], constants::LOINC_SYSTEM);
        assert_eq!(obs["code"]["coding"][0]["code"], "8480-6");
    }
}
