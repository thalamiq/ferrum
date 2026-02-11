//! Metadata Service
//!
//! Generates FHIR CapabilityStatement resources dynamically based on:
//! - Server configuration
//! - Database state (available search parameters)
//! - Loaded StructureDefinitions

use crate::{config::Config, db::metadata::MetadataRepository, Result};
use chrono::Utc;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::str::FromStr;

/// Mode parameter for capabilities interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityMode {
    /// Full capability statement (default)
    Full,
    /// Only normative portions
    Normative,
    /// TerminologyCapabilities resource
    Terminology,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCapabilityModeError;

impl FromStr for CapabilityMode {
    type Err = ParseCapabilityModeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "full" => Ok(Self::Full),
            "normative" => Ok(Self::Normative),
            "terminology" => Ok(Self::Terminology),
            _ => Err(ParseCapabilityModeError),
        }
    }
}

impl CapabilityMode {
    /// Parse from string, returns None for invalid values
    /// Prefer using `FromStr` trait for better error handling
    pub fn try_from_str(s: &str) -> Option<Self> {
        Self::from_str(s).ok()
    }
}

/// Search parameter information
#[derive(Debug, Clone)]
pub struct SearchParameter {
    pub name: String,
    pub r#type: String,
    pub documentation: Option<String>,
    pub target: Option<Vec<String>>,
}

/// Service for generating FHIR CapabilityStatement
pub struct MetadataService {
    config: std::sync::Arc<Config>,
    repo: MetadataRepository,
}

impl MetadataService {
    pub fn new(config: std::sync::Arc<Config>, repo: MetadataRepository) -> Self {
        Self { config, repo }
    }

    /// Generate capability statement
    pub async fn get_capability_statement(
        &self,
        mode: CapabilityMode,
        base_url: &str,
    ) -> Result<JsonValue> {
        if mode == CapabilityMode::Terminology {
            return self.get_terminology_capabilities(base_url).await;
        }

        // Get search parameters from database
        let search_params_by_resource = self.get_search_parameters_by_resource().await?;

        // Get supported resource types
        let supported_resources = self.get_supported_resource_types().await?;

        // Build the capability statement
        let cs_config = &self.config.fhir.capability_statement;
        let now = Utc::now();

        let mut capability_statement = json!({
            "resourceType": "CapabilityStatement",
            "id": cs_config.id,
            "url": format!("{}/metadata", base_url),
            "version": cs_config.software_version,
            "name": cs_config.name,
            "title": cs_config.title,
            "status": "active",
            "experimental": true,
            "date": now.to_rfc3339(),
            "publisher": cs_config.publisher,
            "description": cs_config.description,
            "kind": "instance",
            "software": {
                "name": cs_config.software_name,
                "version": cs_config.software_version,
                "releaseDate": now.format("%Y-%m-%d").to_string(),
            },
            "implementation": {
                "description": format!("FHIR {} Server", self.config.fhir.version),
                "url": base_url,
            },
            "fhirVersion": self.get_fhir_version_code(),
            "format": [
                "application/fhir+json",
                "application/fhir+xml"
            ],
            "patchFormat": [
                "application/json-patch+json"
            ],
            "rest": [{
                "mode": "server",
                "documentation": "FHIR Server REST API",
                "security": {
                    "cors": true,
                    "description": "CORS is enabled for browser-based applications"
                },
                "resource": self.build_resource_capabilities(&search_params_by_resource, &supported_resources),
                "interaction": self.build_system_interactions(),
                "searchParam": self.build_common_search_params()
            }]
        });

        self.add_terminology_operations(&mut capability_statement);

        // Add contact if configured
        if let Some(ref email) = cs_config.contact_email {
            capability_statement["contact"] = json!([{
                "telecom": [{
                    "system": "email",
                    "value": email
                }]
            }]);
        }

        Ok(capability_statement)
    }

    async fn get_terminology_capabilities(&self, base_url: &str) -> Result<JsonValue> {
        let cs_config = &self.config.fhir.capability_statement;
        let now = Utc::now();

        Ok(json!({
            "resourceType": "TerminologyCapabilities",
            "id": "terminology",
            "url": format!("{}/metadata?mode=terminology", base_url),
            "version": cs_config.software_version,
            "name": format!("{}Terminology", cs_config.name),
            "status": "active",
            "date": now.to_rfc3339(),
            "publisher": cs_config.publisher,
            "kind": "instance",
            "software": {
                "name": cs_config.software_name,
                "version": cs_config.software_version
            },
            "implementation": {
                "description": format!("FHIR {} Terminology Server", self.config.fhir.version),
                "url": base_url
            },
            "fhirVersion": self.get_fhir_version_code(),
            "expansion": {
                "paging": true
            },
            "validateCode": {},
            "translation": {},
            "closure": {
                "translation": false
            }
        }))
    }

    fn add_terminology_operations(&self, capability_statement: &mut JsonValue) {
        let Some(rest) = capability_statement
            .get_mut("rest")
            .and_then(|v| v.as_array_mut())
        else {
            return;
        };
        let Some(server) = rest.get_mut(0) else {
            return;
        };

        // System-level operations
        server["operation"] = json!([
            {
                "name": "closure",
                "definition": "http://hl7.org/fhir/OperationDefinition/ConceptMap-closure"
            }
        ]);

        let Some(resources) = server.get_mut("resource").and_then(|v| v.as_array_mut()) else {
            return;
        };

        for res in resources {
            let Some(rt) = res.get("type").and_then(|v| v.as_str()) else {
                continue;
            };
            match rt {
                "ValueSet" => {
                    res["operation"] = json!([
                        {
                            "name": "expand",
                            "definition": "http://hl7.org/fhir/OperationDefinition/ValueSet-expand"
                        },
                        {
                            "name": "validate-code",
                            "definition": "http://hl7.org/fhir/OperationDefinition/ValueSet-validate-code"
                        }
                    ]);
                }
                "CodeSystem" => {
                    res["operation"] = json!([
                        {
                            "name": "lookup",
                            "definition": "http://hl7.org/fhir/OperationDefinition/CodeSystem-lookup"
                        },
                        {
                            "name": "validate-code",
                            "definition": "http://hl7.org/fhir/OperationDefinition/CodeSystem-validate-code"
                        },
                        {
                            "name": "subsumes",
                            "definition": "http://hl7.org/fhir/OperationDefinition/CodeSystem-subsumes"
                        }
                    ]);
                }
                "ConceptMap" => {
                    res["operation"] = json!([
                        {
                            "name": "translate",
                            "definition": "http://hl7.org/fhir/OperationDefinition/ConceptMap-translate"
                        }
                    ]);
                }
                _ => {}
            }
        }
    }

    /// Get search parameters grouped by resource type from database
    async fn get_search_parameters_by_resource(
        &self,
    ) -> Result<HashMap<String, Vec<SearchParameter>>> {
        let params_by_resource_info = self.repo.get_search_parameters_by_resource().await?;

        // Convert from repository type to service type
        let mut params_by_resource: HashMap<String, Vec<SearchParameter>> = HashMap::new();

        for (resource_type, param_infos) in params_by_resource_info {
            let params: Vec<SearchParameter> = param_infos
                .into_iter()
                .map(|info| SearchParameter {
                    name: info.code.clone(),
                    r#type: info.param_type.clone(),
                    documentation: info
                        .description
                        .or_else(|| Some(format!("Search parameter {}", info.code))),
                    target: if info.param_type == "reference" {
                        info.targets
                    } else {
                        None
                    },
                })
                .collect();

            params_by_resource.insert(resource_type, params);
        }

        Ok(params_by_resource)
    }

    /// Get list of supported resource types
    async fn get_supported_resource_types(&self) -> Result<Vec<String>> {
        // If configured explicitly, use that list
        if !self
            .config
            .fhir
            .capability_statement
            .supported_resources
            .is_empty()
        {
            return Ok(self
                .config
                .fhir
                .capability_statement
                .supported_resources
                .clone());
        }

        // Otherwise, query from database to get all resource types that have StructureDefinitions
        let mut resource_types = self
            .repo
            .get_resource_types_from_structure_definitions()
            .await?;

        // If no StructureDefinitions found, use a default list of common resources
        if resource_types.is_empty() {
            resource_types = vec![
                "Patient",
                "Observation",
                "Condition",
                "Procedure",
                "MedicationRequest",
                "Encounter",
                "Organization",
                "Practitioner",
                "DiagnosticReport",
                "Immunization",
                "AllergyIntolerance",
                "CarePlan",
                "CareTeam",
                "Goal",
                "Device",
                "Medication",
                "Location",
                "HealthcareService",
                "PractitionerRole",
                "Specimen",
                "ServiceRequest",
                "DocumentReference",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
        }

        Ok(resource_types)
    }

    /// Build resource capabilities for each supported resource type
    fn build_resource_capabilities(
        &self,
        search_params: &HashMap<String, Vec<SearchParameter>>,
        resource_types: &[String],
    ) -> Vec<JsonValue> {
        let mut resources = Vec::new();

        for resource_type in resource_types {
            let params = search_params
                .get(resource_type)
                .cloned()
                .unwrap_or_default();

            let mut interactions = Vec::new();
            if self.config.fhir.interactions.instance.read {
                interactions.push(json!({
                    "code": "read",
                    "documentation": format!("Read {} by ID", resource_type)
                }));
            }
            if self.config.fhir.interactions.instance.vread {
                interactions.push(json!({
                    "code": "vread",
                    "documentation": format!("Read specific version of {}", resource_type)
                }));
            }
            if self.config.fhir.interactions.instance.update {
                interactions.push(json!({
                    "code": "update",
                    "documentation": format!("Update {} resource", resource_type)
                }));
            }
            if self.config.fhir.interactions.instance.patch {
                interactions.push(json!({
                    "code": "patch",
                    "documentation": format!("Patch {} resource", resource_type)
                }));
            }
            if self.config.fhir.interactions.instance.delete {
                interactions.push(json!({
                    "code": "delete",
                    "documentation": format!("Delete {} resource", resource_type)
                }));
            }
            if self.config.fhir.interactions.instance.history {
                interactions.push(json!({
                    "code": "history-instance",
                    "documentation": format!("History for {} instance", resource_type)
                }));
            }
            if self.config.fhir.interactions.type_level.history {
                interactions.push(json!({
                    "code": "history-type",
                    "documentation": format!("History for {} type", resource_type)
                }));
            }
            if self.config.fhir.interactions.type_level.create {
                interactions.push(json!({
                    "code": "create",
                    "documentation": format!("Create new {} resource", resource_type)
                }));
            }
            if self.config.fhir.interactions.type_level.search {
                interactions.push(json!({
                    "code": "search-type",
                    "documentation": format!("Search {} resources", resource_type)
                }));
            }

            let update_create_enabled = self.config.fhir.allow_update_create
                && self.config.fhir.interactions.instance.update;
            let read_history_enabled = self.config.fhir.interactions.instance.history
                || self.config.fhir.interactions.type_level.history;
            let conditional_delete_mode =
                if self.config.fhir.interactions.type_level.conditional_delete {
                    "single"
                } else {
                    "not-supported"
                };

            let resource_capability = json!({
                "type": resource_type,
                "profile": format!("http://hl7.org/fhir/StructureDefinition/{}", resource_type),
                "documentation": format!("CRUD operations and search for {} resources", resource_type),
                "interaction": interactions,
                "versioning": "versioned",
                "readHistory": read_history_enabled,
                "updateCreate": update_create_enabled,
                "conditionalCreate": self.config.fhir.interactions.type_level.conditional_create,
                "conditionalRead": "full-support",
                "conditionalUpdate": self.config.fhir.interactions.type_level.conditional_update,
                "conditionalDelete": conditional_delete_mode,
                "searchInclude": ["*"],
                "searchRevInclude": ["*"],
                "searchParam": self.format_search_params(&params)
            });

            resources.push(resource_capability);
        }

        resources
    }

    /// Format search parameters for capability statement
    fn format_search_params(&self, params: &[SearchParameter]) -> Vec<JsonValue> {
        params
            .iter()
            .map(|p| {
                let mut param = json!({
                    "name": p.name,
                    "type": p.r#type,
                    "documentation": p.documentation.clone().unwrap_or_default()
                });

                // Add target types for reference parameters
                if let Some(ref targets) = p.target {
                    param["target"] = json!(targets);
                }

                param
            })
            .collect()
    }

    /// Build common search parameters (available for all resource types)
    fn build_common_search_params(&self) -> Vec<JsonValue> {
        let mut params = vec![
            json!({
                "name": "_id",
                "type": "token",
                "documentation": "Logical id of this artifact"
            }),
            json!({
                "name": "_lastUpdated",
                "type": "date",
                "documentation": "When the resource version last changed"
            }),
            json!({
                "name": "_language",
                "type": "token",
                "documentation": "Language of the resource"
            }),
            json!({
                "name": "_type",
                "type": "special",
                "documentation": "Filter types in system-level searches"
            }),
            json!({
                "name": "_profile",
                "type": "reference",
                "documentation": "Profiles this resource claims to conform to"
            }),
            json!({
                "name": "_security",
                "type": "token",
                "documentation": "Security Labels applied to this resource"
            }),
            json!({
                "name": "_source",
                "type": "uri",
                "documentation": "Identifies where the resource comes from"
            }),
            json!({
                "name": "_tag",
                "type": "token",
                "documentation": "Tags applied to this resource"
            }),
        ];

        if self.config.fhir.search.enable_text {
            params.push(json!({
                "name": "_text",
                "type": "special",
                "documentation": "Search on the narrative of the resource"
            }));
        }
        if self.config.fhir.search.enable_content {
            params.push(json!({
                "name": "_content",
                "type": "special",
                "documentation": "Search on the entire content of the resource"
            }));
        }

        params
    }

    fn build_system_interactions(&self) -> Vec<JsonValue> {
        let mut interactions = Vec::new();

        if self.config.fhir.interactions.system.batch {
            interactions.push(json!({
                "code": "batch",
                "documentation": "Support for batch operations"
            }));
        }
        if self.config.fhir.interactions.system.transaction {
            interactions.push(json!({
                "code": "transaction",
                "documentation": "Support for transaction operations"
            }));
        }
        if self.config.fhir.interactions.system.search {
            interactions.push(json!({
                "code": "search-system",
                "documentation": "Support for server-wide search"
            }));
        }
        if self.config.fhir.interactions.system.history {
            interactions.push(json!({
                "code": "history-system",
                "documentation": "Support for system-wide history"
            }));
        }
        if self.config.fhir.interactions.system.delete {
            interactions.push(json!({
                "code": "delete-system",
                "documentation": "Support for conditional delete across all resource types"
            }));
        }

        interactions
    }

    /// Get FHIR version code
    fn get_fhir_version_code(&self) -> String {
        match self.config.fhir.version.as_str() {
            "R4" => "4.0.1".to_string(),
            "R4B" => "4.3.0".to_string(),
            "R5" => "5.0.0".to_string(),
            other => other.to_string(),
        }
    }
}
