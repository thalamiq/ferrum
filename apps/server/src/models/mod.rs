//! Domain models for the FHIR server

pub mod fhir;
pub mod operations;
pub mod resource_types;

pub use fhir::{
    ConditionalParams, CreateParams, HistoryEntry, HistoryMethod, HistoryResult, Resource,
    ResourceOperation, ResourceResult, UpdateParams,
};
pub use operations::*;
pub use resource_types::{is_known_resource_type, RESOURCE_TYPES};
