use crate::{
    db::terminology::{ConceptDetails, TerminologyRepository},
    models::{OperationContext, Parameters},
    Error, Result,
};
use chrono::Utc;
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone)]
pub struct TerminologyService {
    repo: TerminologyRepository,
}

impl TerminologyService {
    pub fn new(repo: TerminologyRepository) -> Self {
        Self { repo }
    }

    pub async fn expand(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<JsonValue> {
        let valueset = self.resolve_valueset(context, params).await?;

        // Extract parameters
        let filter = params
            .get_value("filter")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let offset = params
            .get_value("offset")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0) as usize;

        let count = params
            .get_value("count")
            .and_then(|v| v.as_i64())
            .unwrap_or(1000)
            .max(0) as usize;

        let display_language = params
            .get_value("displayLanguage")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let active_only = params
            .get_value("activeOnly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let include_designations = params
            .get_value("includeDesignations")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Check cache
        let vs_url = valueset.get("url").and_then(|v| v.as_str());
        let vs_version = valueset.get("version").and_then(|v| v.as_str());
        let params_hash = self.compute_expansion_params_hash(
            &filter,
            offset,
            count,
            &display_language,
            active_only,
            include_designations,
        );

        if let Some((url, version)) = vs_url.zip(vs_version.or(Some(""))) {
            if let Ok(Some(contains)) = self
                .repo
                .fetch_cached_expansion(url, version, &params_hash)
                .await
            {
                let expansion = json!({
                    "identifier": format!("urn:uuid:{}", Uuid::new_v4()),
                    "timestamp": Utc::now().to_rfc3339(),
                    "contains": contains
                });

                let mut cached_valueset = json!({
                    "resourceType": "ValueSet",
                    "url": url,
                    "expansion": expansion
                });

                if !version.is_empty() {
                    cached_valueset["version"] = JsonValue::String(version.to_string());
                }

                return Ok(cached_valueset);
            }
        }

        // Expand valueset
        let mut concepts = self.expand_valueset(&valueset).await?;

        // Apply activeOnly filter
        if active_only {
            concepts.retain(|c| !c.inactive.unwrap_or(false));
        }

        // Apply text filter
        if let Some(f) = filter.as_deref() {
            let f = f.to_lowercase();
            concepts.retain(|c| {
                c.code.to_lowercase().contains(&f)
                    || c.display
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&f)
            });
        }

        let total = concepts.len();
        let sliced = concepts
            .into_iter()
            .skip(offset)
            .take(count)
            .collect::<Vec<_>>();

        let contains = sliced
            .iter()
            .map(|c| {
                let mut obj = serde_json::Map::new();
                obj.insert("system".to_string(), JsonValue::String(c.system.clone()));
                obj.insert("code".to_string(), JsonValue::String(c.code.clone()));

                // Display (with language preference)
                let display = if let Some(ref lang) = display_language {
                    c.designations
                        .as_ref()
                        .and_then(|d| d.as_array())
                        .and_then(|arr| {
                            arr.iter()
                                .find(|d| {
                                    d.get("language").and_then(|v| v.as_str())
                                        == Some(lang.as_str())
                                })
                                .and_then(|d| d.get("value").and_then(|v| v.as_str()))
                        })
                        .or(c.display.as_deref())
                } else {
                    c.display.as_deref()
                };

                if let Some(d) = display {
                    obj.insert("display".to_string(), JsonValue::String(d.to_string()));
                }

                if c.inactive.unwrap_or(false) {
                    obj.insert("inactive".to_string(), JsonValue::Bool(true));
                }

                // Include designations if requested
                if include_designations {
                    if let Some(ref desig) = c.designations {
                        obj.insert("designation".to_string(), desig.clone());
                    }
                }

                JsonValue::Object(obj)
            })
            .collect::<Vec<_>>();

        let expansion_id = Uuid::new_v4();
        let mut out = valueset.clone();
        out["expansion"] = json!({
            "identifier": format!("urn:uuid:{}", expansion_id),
            "timestamp": Utc::now().to_rfc3339(),
            "total": total,
            "offset": offset,
            "contains": contains
        });

        // Store in cache
        if let Some((url, version)) = vs_url.zip(vs_version.or(Some(""))) {
            let concept_tuples: Vec<_> = sliced
                .iter()
                .map(|c| {
                    (
                        c.system.clone(),
                        c.code.clone(),
                        c.display.clone(),
                        c.inactive,
                        c.designations.clone(),
                    )
                })
                .collect();

            let _ = self
                .repo
                .store_expansion_cache(
                    url,
                    version,
                    &params_hash,
                    expansion_id,
                    total,
                    offset,
                    count,
                    &JsonValue::Array(contains.clone()),
                    &concept_tuples,
                )
                .await;
        }

        Ok(out)
    }

    pub async fn lookup(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<Parameters> {
        // $lookup is defined on CodeSystem (type-level) in R4, but tolerate instance-level
        let (system, code, version) = self
            .resolve_system_code(params, context, "CodeSystem")
            .await?;

        let cs_name = match self
            .repo
            .find_resource_by_canonical_url("CodeSystem", &system, version.as_deref())
            .await
        {
            Ok(Some(cs_resource)) => cs_resource
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| cs_resource.get("title").and_then(|v| v.as_str()))
                .unwrap_or(system.as_str())
                .to_string(),
            _ => system.clone(),
        };

        let cs = self
            .find_concept_in_codesystem(&system, version.as_deref(), &code)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("Unknown code '{}' in system '{}'", code, system))
            })?;

        // Get requested properties (if specified)
        let requested_properties: Vec<String> = params
            .get_values("property")
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        let mut out = Parameters::new();
        out.add_value_string("name".to_string(), cs_name);
        if let Some(v) = version {
            out.add_value_string("version".to_string(), v);
        }
        out.add_value_string(
            "display".to_string(),
            cs.display.clone().unwrap_or_else(|| code.clone()),
        );

        // Add properties
        if let Some(properties) = cs.properties {
            if let Some(arr) = properties.as_array() {
                for prop in arr {
                    let prop_code = prop.get("code").and_then(|v| v.as_str());

                    // If specific properties requested, only include those
                    if !requested_properties.is_empty() {
                        if let Some(code) = prop_code {
                            if !requested_properties.contains(&code.to_string()) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    // Add property to output
                    let mut prop_parts = Vec::new();
                    if let Some(code) = prop_code {
                        prop_parts.push(crate::models::Parameter {
                            name: "code".to_string(),
                            value: crate::models::ParameterValue::Value(HashMap::from([(
                                "valueCode".to_string(),
                                JsonValue::String(code.to_string()),
                            )])),
                        });
                    }

                    // Add property value
                    if let Some(value_code) = prop.get("valueCode") {
                        prop_parts.push(crate::models::Parameter {
                            name: "value".to_string(),
                            value: crate::models::ParameterValue::Value(HashMap::from([(
                                "valueCode".to_string(),
                                value_code.clone(),
                            )])),
                        });
                    } else if let Some(value) = prop
                        .get("valueString")
                        .or_else(|| prop.get("valueBoolean"))
                        .or_else(|| prop.get("valueInteger"))
                        .or_else(|| prop.get("valueDecimal"))
                    {
                        prop_parts.push(crate::models::Parameter {
                            name: "value".to_string(),
                            value: crate::models::ParameterValue::Value(HashMap::from([(
                                "valueString".to_string(),
                                value.clone(),
                            )])),
                        });
                    }

                    if !prop_parts.is_empty() {
                        out.add_parts("property".to_string(), prop_parts);
                    }
                }
            }
        }

        // Add designations
        if let Some(designations) = cs.designations {
            if let Some(arr) = designations.as_array() {
                for desig in arr {
                    let mut desig_parts = Vec::new();

                    if let Some(lang) = desig.get("language").and_then(|v| v.as_str()) {
                        desig_parts.push(crate::models::Parameter {
                            name: "language".to_string(),
                            value: crate::models::ParameterValue::Value(HashMap::from([(
                                "valueCode".to_string(),
                                JsonValue::String(lang.to_string()),
                            )])),
                        });
                    }

                    if let Some(use_obj) = desig.get("use") {
                        desig_parts.push(crate::models::Parameter {
                            name: "use".to_string(),
                            value: crate::models::ParameterValue::Value(HashMap::from([(
                                "valueCoding".to_string(),
                                use_obj.clone(),
                            )])),
                        });
                    }

                    if let Some(value) = desig.get("value").and_then(|v| v.as_str()) {
                        desig_parts.push(crate::models::Parameter {
                            name: "value".to_string(),
                            value: crate::models::ParameterValue::Value(HashMap::from([(
                                "valueString".to_string(),
                                JsonValue::String(value.to_string()),
                            )])),
                        });
                    }

                    if !desig_parts.is_empty() {
                        out.add_parts("designation".to_string(), desig_parts);
                    }
                }
            }
        }

        Ok(out)
    }

    pub async fn validate_code(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<Parameters> {
        match context {
            OperationContext::Instance(rt, _) | OperationContext::Type(rt) if rt == "ValueSet" => {
                self.validate_code_in_valueset(context, params).await
            }
            OperationContext::Instance(rt, _) | OperationContext::Type(rt)
                if rt == "CodeSystem" =>
            {
                self.validate_code_in_codesystem(context, params).await
            }
            _ => Err(Error::Validation(
                "$validate-code must be invoked on ValueSet or CodeSystem".to_string(),
            )),
        }
    }

    pub async fn subsumes(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<Parameters> {
        let system = match context {
            OperationContext::Instance(rt, id) if rt == "CodeSystem" => {
                let cs = self
                    .repo
                    .find_resource_by_id("CodeSystem", id)
                    .await?
                    .ok_or_else(|| Error::ResourceNotFound {
                        resource_type: "CodeSystem".to_string(),
                        id: id.to_string(),
                    })?;
                cs.get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("CodeSystem instance has no url".to_string()))?
                    .to_string()
            }
            _ => params
                .get_value("system")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Validation("Missing parameter: system".to_string()))?
                .to_string(),
        };

        let (code_a, code_b) = self.resolve_code_a_b(params)?;

        let outcome = self.subsumption_outcome(&system, &code_a, &code_b).await?;

        let mut out = Parameters::new();
        out.add_value_code("outcome".to_string(), outcome);
        Ok(out)
    }

    pub async fn translate(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<Parameters> {
        // Resolve map: url param or ConceptMap instance
        let map = if let Some(url) = params.get_value("url").and_then(|v| v.as_str()) {
            self.repo
                .find_resource_by_canonical_url("ConceptMap", url, None)
                .await?
                .ok_or_else(|| Error::NotFound(format!("ConceptMap not found for url '{}'", url)))?
        } else if let OperationContext::Instance(rt, id) = context {
            if rt != "ConceptMap" {
                return Err(Error::Validation(
                    "ConceptMap instance required when url is not provided".to_string(),
                ));
            }
            self.repo
                .find_resource_by_id("ConceptMap", id)
                .await?
                .ok_or_else(|| Error::ResourceNotFound {
                    resource_type: "ConceptMap".to_string(),
                    id: id.to_string(),
                })?
        } else {
            return Err(Error::Validation(
                "Missing parameter: url (ConceptMap canonical)".to_string(),
            ));
        };

        let reverse = params
            .get_value("reverse")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let (system, code) = self.resolve_system_and_code_or_coding(params)?;
        let matches = translate_with_map(&map, &system, &code, reverse);

        let mut out = Parameters::new();
        let result = matches
            .iter()
            .any(|m| m.equivalence != "unmatched" && m.equivalence != "disjoint");
        out.add_value_boolean("result".to_string(), result);
        for m in matches {
            let mut parts = Vec::new();
            parts.push(crate::models::Parameter {
                name: "equivalence".to_string(),
                value: crate::models::ParameterValue::Value(HashMap::from([(
                    "valueCode".to_string(),
                    JsonValue::String(m.equivalence),
                )])),
            });

            let mut coding_obj = serde_json::Map::new();
            coding_obj.insert("system".to_string(), JsonValue::String(m.system));
            coding_obj.insert("code".to_string(), JsonValue::String(m.code));
            if let Some(d) = m.display {
                coding_obj.insert("display".to_string(), JsonValue::String(d));
            }
            parts.push(crate::models::Parameter {
                name: "concept".to_string(),
                value: crate::models::ParameterValue::Value(HashMap::from([(
                    "valueCoding".to_string(),
                    JsonValue::Object(coding_obj),
                )])),
            });

            out.add_parts("match".to_string(), parts);
        }
        Ok(out)
    }

    pub async fn closure(&self, params: &Parameters) -> Result<JsonValue> {
        let name = params
            .get_value("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Validation("Missing parameter: name".to_string()))?
            .to_string();

        let requested_since = params
            .get_value("version")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());

        // Ensure closure table exists (create on first call when no version is requested).
        let exists = self.repo.get_closure_version(&name).await?;

        let mut current_version = if let Some(v) = exists {
            v
        } else {
            if requested_since.is_some() {
                return Err(Error::NotFound(format!(
                    "invalid closure name \"{}\"",
                    name
                )));
            }
            self.repo.create_closure_table(&name).await?;
            1
        };

        let requires_reinit = self.repo.closure_requires_reinit(&name).await?;

        if requires_reinit {
            return Err(Error::PreconditionFailed(format!(
                "closure \"{}\" must be reinitialized",
                name
            )));
        }

        let new_concepts = params.get_values("concept");
        let mut inserted_any = false;

        if !new_concepts.is_empty() {
            let mut tx = self.repo.begin_transaction().await?;

            for coding in new_concepts {
                let Some(system) = coding.get("system").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(code) = coding.get("code").and_then(|v| v.as_str()) else {
                    continue;
                };

                let display = coding.get("display").and_then(|v| v.as_str());

                let was_inserted = TerminologyRepository::insert_closure_concept(
                    &mut tx, &name, system, code, display,
                )
                .await?;

                if was_inserted {
                    inserted_any = true;
                }
            }

            // If we inserted any new concepts, increment version and compute new relations.
            if inserted_any {
                current_version += 1;
                TerminologyRepository::update_closure_version(&mut tx, &name, current_version)
                    .await?;

                // Compute new relations within each system based on explicit CodeSystem hierarchy (best-effort).
                let rows = TerminologyRepository::fetch_closure_concepts(&mut tx, &name).await?;

                let mut by_system: HashMap<String, Vec<String>> = HashMap::new();
                for (system, code) in rows {
                    by_system.entry(system).or_default().push(code);
                }

                for (system, codes) in by_system {
                    // Precompute subsumption closure among codes in this system.
                    // This is O(n^2) and intended for small closure tables.
                    for i in 0..codes.len() {
                        for j in 0..codes.len() {
                            if i == j {
                                continue;
                            }
                            let a = &codes[i];
                            let b = &codes[j];
                            match self.subsumption_outcome(&system, a, b).await {
                                Ok(outcome) if outcome == "subsumes" => {
                                    // a subsumes b -> store relation (source=b, target=a) with equivalence=subsumes (target subsumes source)
                                    let _ = TerminologyRepository::insert_closure_relation(
                                        &mut tx,
                                        &name,
                                        &system,
                                        b,
                                        &system,
                                        a,
                                        "subsumes",
                                        current_version,
                                    )
                                    .await;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            tx.commit().await.map_err(Error::Database)?;
        }

        // Determine which relations to return.
        let since = requested_since.unwrap_or_else(|| {
            if inserted_any {
                current_version - 1
            } else {
                current_version
            }
        });

        if since > current_version {
            return Err(Error::Validation(format!(
                "Requested version {} is newer than current version {}",
                since, current_version
            )));
        }

        let relations = if since == 0 {
            self.repo.fetch_closure_relations(&name, None).await?
        } else {
            self.repo
                .fetch_closure_relations(&name, Some(since))
                .await?
        };

        Ok(build_closure_conceptmap(current_version, relations))
    }

    async fn validate_code_in_codesystem(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<Parameters> {
        let url = if let Some(url) = params.get_value("url").and_then(|v| v.as_str()) {
            url.to_string()
        } else if let OperationContext::Instance(rt, id) = context {
            if rt != "CodeSystem" {
                return Err(Error::Validation(
                    "CodeSystem instance required when url is not provided".to_string(),
                ));
            }
            let cs = self
                .repo
                .find_resource_by_id("CodeSystem", id)
                .await?
                .ok_or_else(|| Error::ResourceNotFound {
                    resource_type: "CodeSystem".to_string(),
                    id: id.to_string(),
                })?;
            cs.get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Validation("CodeSystem instance has no url".to_string()))?
                .to_string()
        } else {
            return Err(Error::Validation("Missing parameter: url".to_string()));
        };

        let (system, code) = if let Some(code) = params.get_value("code").and_then(|v| v.as_str()) {
            (url.clone(), code.to_string())
        } else {
            self.resolve_system_and_code_or_coding(params)?
        };
        if system != url {
            return Err(Error::Validation(
                "Code system does not match CodeSystem url".to_string(),
            ));
        }

        let found = self
            .find_concept_in_codesystem(&url, None, &code)
            .await?
            .is_some();

        let mut out = Parameters::new();
        out.add_value_boolean("result".to_string(), found);
        if found {
            if let Some(display) = self
                .find_concept_in_codesystem(&url, None, &code)
                .await?
                .and_then(|c| c.display)
            {
                out.add_value_string("display".to_string(), display);
            }
        } else {
            out.add_value_string(
                "message".to_string(),
                format!("Unknown code '{}' in system '{}'", code, url),
            );
        }
        Ok(out)
    }

    async fn validate_code_in_valueset(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<Parameters> {
        let valueset = self.resolve_valueset(context, params).await?;
        let (system, code) = self.resolve_system_and_code_or_coding(params)?;

        let expanded = self.expand_valueset(&valueset).await?;
        let mut found_display: Option<String> = None;
        let found = expanded.iter().any(|c| {
            let ok = c.system == system && c.code == code;
            if ok {
                found_display = c.display.clone();
            }
            ok
        });

        let mut out = Parameters::new();
        out.add_value_boolean("result".to_string(), found);
        if let Some(d) = found_display {
            out.add_value_string("display".to_string(), d);
        }
        if !found {
            out.add_value_string("message".to_string(), "Code not in ValueSet".to_string());
        }
        Ok(out)
    }

    async fn resolve_valueset(
        &self,
        context: &OperationContext,
        params: &Parameters,
    ) -> Result<JsonValue> {
        if let OperationContext::Instance(rt, id) = context {
            if rt == "ValueSet" {
                return self
                    .repo
                    .find_resource_by_id("ValueSet", id)
                    .await?
                    .ok_or_else(|| Error::ResourceNotFound {
                        resource_type: "ValueSet".to_string(),
                        id: id.to_string(),
                    });
            }
        }

        if let Some(url) = params.get_value("url").and_then(|v| v.as_str()) {
            return self
                .repo
                .find_resource_by_canonical_url("ValueSet", url, None)
                .await?
                .ok_or_else(|| Error::NotFound(format!("ValueSet not found for url '{}'", url)));
        }

        if let Some(vs) = params.get_resource("valueSet") {
            return Ok(vs.clone());
        }

        Err(Error::Validation(
            "Missing ValueSet input: use instance invocation, parameter 'url', or parameter 'valueSet'".to_string(),
        ))
    }

    async fn expand_valueset(&self, valueset: &JsonValue) -> Result<Vec<Concept>> {
        let mut out: HashMap<String, Concept> = HashMap::new();
        let mut pending_valuesets: Vec<String> = Vec::new();
        let mut visited_valuesets: HashSet<String> = HashSet::new();

        if let Some(url) = valueset.get("url").and_then(|v| v.as_str()) {
            visited_valuesets.insert(url.to_string());
        }

        self.process_valueset_for_expansion(valueset, &mut out, &mut pending_valuesets)
            .await?;

        while let Some(url) = pending_valuesets.pop() {
            if !visited_valuesets.insert(url.clone()) {
                continue;
            }
            let vs = self
                .repo
                .find_resource_by_canonical_url("ValueSet", &url, None)
                .await?
                .ok_or_else(|| Error::NotFound(format!("ValueSet not found for url '{}'", url)))?;
            self.process_valueset_for_expansion(&vs, &mut out, &mut pending_valuesets)
                .await?;
        }

        if out.is_empty() {
            return Err(Error::TooCostly(
                "ValueSet cannot be expanded with available terminology data".to_string(),
            ));
        }

        Ok(out.into_values().collect())
    }

    async fn process_valueset_for_expansion(
        &self,
        valueset: &JsonValue,
        out: &mut HashMap<String, Concept>,
        pending_valuesets: &mut Vec<String>,
    ) -> Result<()> {
        if let Some(exp) = valueset.get("expansion").and_then(|v| v.get("contains")) {
            extract_valueset_expansion_contains(exp, out);
        }

        if let Some(compose) = valueset.get("compose") {
            // include
            if let Some(includes) = compose.get("include").and_then(|v| v.as_array()) {
                for include in includes {
                    let system = include
                        .get("system")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if let Some(concepts) = include.get("concept").and_then(|v| v.as_array()) {
                        let Some(system) = system.clone() else {
                            continue;
                        };
                        for concept in concepts {
                            let Some(code) = concept.get("code").and_then(|v| v.as_str()) else {
                                continue;
                            };
                            let display = concept
                                .get("display")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            out.insert(
                                format!("{}|{}", system, code),
                                Concept {
                                    system: system.clone(),
                                    code: code.to_string(),
                                    display,
                                    inactive: None,
                                    designations: concept.get("designation").cloned(),
                                },
                            );
                        }
                    } else if let Some(system) = system {
                        // include an entire CodeSystem (only possible if codes are present / indexed)
                        let rows = self.repo.fetch_system_concepts(&system).await?;

                        for row in rows {
                            out.insert(
                                format!("{}|{}", system, row.code),
                                Concept {
                                    system: system.clone(),
                                    code: row.code,
                                    display: Some(row.display),
                                    inactive: None,
                                    designations: None,
                                },
                            );
                        }
                    }

                    // include referenced ValueSets
                    if let Some(vs_refs) = include.get("valueSet").and_then(|v| v.as_array()) {
                        for vs_ref in vs_refs {
                            if let Some(url) = vs_ref.as_str() {
                                pending_valuesets.push(url.to_string());
                            }
                        }
                    }
                }
            }

            // exclude (explicit code list only)
            if let Some(excludes) = compose.get("exclude").and_then(|v| v.as_array()) {
                for exclude in excludes {
                    if let Some(system) = exclude.get("system").and_then(|v| v.as_str()) {
                        if let Some(concepts) = exclude.get("concept").and_then(|v| v.as_array()) {
                            for concept in concepts {
                                if let Some(code) = concept.get("code").and_then(|v| v.as_str()) {
                                    out.remove(&format!("{}|{}", system, code));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn resolve_code_a_b(&self, params: &Parameters) -> Result<(String, String)> {
        if let (Some(a), Some(b)) = (
            params.get_value("codeA").and_then(|v| v.as_str()),
            params.get_value("codeB").and_then(|v| v.as_str()),
        ) {
            return Ok((a.to_string(), b.to_string()));
        }

        // TODO: support codingA/codingB
        Err(Error::Validation(
            "Missing parameters: codeA and codeB".to_string(),
        ))
    }

    async fn subsumption_outcome(
        &self,
        system: &str,
        code_a: &str,
        code_b: &str,
    ) -> Result<String> {
        if code_a == code_b {
            return Ok("equivalent".to_string());
        }

        let parent_map = self.load_codesystem_parent_map(system).await?;
        if is_ancestor(&parent_map, code_a, code_b) {
            return Ok("subsumes".to_string());
        }
        if is_ancestor(&parent_map, code_b, code_a) {
            return Ok("subsumed-by".to_string());
        }
        Ok("not-subsumed".to_string())
    }

    async fn load_codesystem_parent_map(
        &self,
        system: &str,
    ) -> Result<HashMap<String, HashSet<String>>> {
        let cs = self
            .repo
            .find_resource_by_canonical_url("CodeSystem", system, None)
            .await?
            .ok_or_else(|| Error::NotFound(format!("CodeSystem not found for url '{}'", system)))?;
        let mut parent_map: HashMap<String, HashSet<String>> = HashMap::new();

        let Some(concepts) = cs.get("concept") else {
            return Err(Error::NotImplemented(format!(
                "CodeSystem '{}' has no concept hierarchy available",
                system
            )));
        };
        build_parent_map_recursive(concepts, None, &mut parent_map);
        Ok(parent_map)
    }

    async fn resolve_system_code(
        &self,
        params: &Parameters,
        context: &OperationContext,
        default_resource_type: &str,
    ) -> Result<(String, String, Option<String>)> {
        let version = params
            .get_value("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let (Some(system), Some(code)) = (
            params.get_value("system").and_then(|v| v.as_str()),
            params.get_value("code").and_then(|v| v.as_str()),
        ) {
            return Ok((system.to_string(), code.to_string(), version));
        }

        if let Some(coding) = params.get_value("coding") {
            let Some(system) = coding.get("system").and_then(|v| v.as_str()) else {
                return Err(Error::Validation("coding.system is required".to_string()));
            };
            let Some(code) = coding.get("code").and_then(|v| v.as_str()) else {
                return Err(Error::Validation("coding.code is required".to_string()));
            };
            return Ok((system.to_string(), code.to_string(), version));
        }

        if let OperationContext::Instance(rt, id) = context {
            if rt == default_resource_type {
                let res = self
                    .repo
                    .find_resource_by_id(rt, id)
                    .await?
                    .ok_or_else(|| Error::ResourceNotFound {
                        resource_type: rt.to_string(),
                        id: id.to_string(),
                    })?;
                let system = res
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation(format!("{} instance has no url", rt)))?
                    .to_string();
                let code = params
                    .get_value("code")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Validation("Missing parameter: code".to_string()))?
                    .to_string();
                return Ok((system, code, version));
            }
        }

        Err(Error::Validation(
            "Missing system+code (or coding)".to_string(),
        ))
    }

    fn resolve_system_and_code_or_coding(&self, params: &Parameters) -> Result<(String, String)> {
        if let (Some(system), Some(code)) = (
            params.get_value("system").and_then(|v| v.as_str()),
            params.get_value("code").and_then(|v| v.as_str()),
        ) {
            return Ok((system.to_string(), code.to_string()));
        }

        if let Some(coding) = params.get_value("coding") {
            let system = coding
                .get("system")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let code = coding
                .get("code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Validation("coding.code is required".to_string()))?
                .to_string();
            return Ok((system, code));
        }

        Err(Error::Validation(
            "Missing parameters: (system, code) or coding".to_string(),
        ))
    }

    async fn find_concept_in_codesystem(
        &self,
        system: &str,
        version: Option<&str>,
        code: &str,
    ) -> Result<Option<ConceptDetails>> {
        // Prefer extracted codesystem_concepts table when available.
        if let Some(concept) = self
            .repo
            .find_concept_in_table(system, code, version)
            .await?
        {
            return Ok(Some(concept));
        }

        // Fallback to CodeSystem resource scanning.
        let cs = match self
            .repo
            .find_resource_by_canonical_url("CodeSystem", system, version)
            .await
        {
            Ok(Some(v)) => v,
            _ => return Ok(None),
        };

        let Some(concepts) = cs.get("concept") else {
            return Ok(None);
        };

        let found = find_concept_recursive(concepts, code);
        Ok(found.map(|c| ConceptDetails {
            display: c
                .get("display")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            properties: c.get("property").cloned(),
            designations: c.get("designation").cloned(),
        }))
    }

    // Expansion caching helpers
    fn compute_expansion_params_hash(
        &self,
        filter: &Option<String>,
        offset: usize,
        count: usize,
        display_language: &Option<String>,
        active_only: bool,
        include_designations: bool,
    ) -> String {
        let mut hasher = Sha256::new();
        if let Some(f) = filter {
            hasher.update(f.as_bytes());
        }
        hasher.update(offset.to_string().as_bytes());
        hasher.update(count.to_string().as_bytes());
        if let Some(lang) = display_language {
            hasher.update(lang.as_bytes());
        }
        hasher.update(active_only.to_string().as_bytes());
        hasher.update(include_designations.to_string().as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone)]
struct Concept {
    system: String,
    code: String,
    display: Option<String>,
    inactive: Option<bool>,
    designations: Option<JsonValue>,
}

#[derive(Debug, Clone)]
struct TranslationMatch {
    system: String,
    code: String,
    display: Option<String>,
    equivalence: String,
}

fn extract_valueset_expansion_contains(value: &JsonValue, out: &mut HashMap<String, Concept>) {
    let Some(arr) = value.as_array() else {
        return;
    };

    for item in arr {
        let system = item.get("system").and_then(|v| v.as_str()).map(str::trim);
        let code = item.get("code").and_then(|v| v.as_str()).map(str::trim);
        let display = item
            .get("display")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let (Some(system), Some(code)) = (system, code) {
            if !system.is_empty() && !code.is_empty() {
                out.insert(
                    format!("{}|{}", system, code),
                    Concept {
                        system: system.to_string(),
                        code: code.to_string(),
                        display,
                        inactive: None,
                        designations: None,
                    },
                );
            }
        }

        if let Some(nested) = item.get("contains") {
            extract_valueset_expansion_contains(nested, out);
        }
    }
}

fn find_concept_recursive<'a>(concepts: &'a JsonValue, code: &str) -> Option<&'a JsonValue> {
    let Some(arr) = concepts.as_array() else {
        return None;
    };

    for concept in arr {
        let is_match = concept
            .get("code")
            .and_then(|v| v.as_str())
            .is_some_and(|c| c == code);
        if is_match {
            return Some(concept);
        }

        if let Some(nested) = concept.get("concept") {
            if let Some(found) = find_concept_recursive(nested, code) {
                return Some(found);
            }
        }
    }

    None
}

fn build_parent_map_recursive(
    concepts: &JsonValue,
    parent_code: Option<&str>,
    out: &mut HashMap<String, HashSet<String>>,
) {
    let Some(arr) = concepts.as_array() else {
        return;
    };

    for concept in arr {
        let Some(code) = concept.get("code").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(parent) = parent_code {
            out.entry(code.to_string())
                .or_default()
                .insert(parent.to_string());
        } else {
            out.entry(code.to_string()).or_default();
        }

        if let Some(nested) = concept.get("concept") {
            build_parent_map_recursive(nested, Some(code), out);
        }
    }
}

fn is_ancestor(
    parent_map: &HashMap<String, HashSet<String>>,
    ancestor: &str,
    descendant: &str,
) -> bool {
    let mut stack = vec![descendant];
    let mut visited = HashSet::<String>::new();
    while let Some(current) = stack.pop() {
        if current == ancestor {
            return true;
        }
        if !visited.insert(current.to_string()) {
            continue;
        }
        if let Some(parents) = parent_map.get(current) {
            for p in parents {
                stack.push(p);
            }
        }
    }
    false
}

fn translate_with_map(
    map: &JsonValue,
    system: &str,
    code: &str,
    reverse: bool,
) -> Vec<TranslationMatch> {
    let mut out = Vec::new();
    let Some(groups) = map.get("group").and_then(|v| v.as_array()) else {
        return out;
    };

    for group in groups {
        let source = group.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let target = group.get("target").and_then(|v| v.as_str()).unwrap_or("");

        if !reverse && source != system {
            continue;
        }
        if reverse && target != system {
            continue;
        }

        let Some(elements) = group.get("element").and_then(|v| v.as_array()) else {
            continue;
        };

        for element in elements {
            let src_code = element.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let Some(targets) = element.get("target").and_then(|v| v.as_array()) else {
                continue;
            };

            if !reverse {
                if src_code != code {
                    continue;
                }
                for t in targets {
                    let t_code = t
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let t_display = t
                        .get("display")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let eq = t
                        .get("equivalence")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unmatched")
                        .to_string();
                    if !t_code.is_empty() {
                        out.push(TranslationMatch {
                            system: target.to_string(),
                            code: t_code,
                            display: t_display,
                            equivalence: eq,
                        });
                    }
                }
            } else {
                for t in targets {
                    let t_code = t.get("code").and_then(|v| v.as_str()).unwrap_or("");
                    if t_code != code {
                        continue;
                    }
                    let eq = t
                        .get("equivalence")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unmatched")
                        .to_string();
                    out.push(TranslationMatch {
                        system: source.to_string(),
                        code: src_code.to_string(),
                        display: element
                            .get("display")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        equivalence: eq,
                    });
                }
            }
        }
    }

    out
}

fn build_closure_conceptmap(
    current_version: i32,
    relations: Vec<(String, String, String, String, String)>,
) -> JsonValue {
    let mut grouped: HashMap<(String, String), HashMap<String, Vec<(String, String)>>> =
        HashMap::new();
    for (src_sys, src_code, tgt_sys, tgt_code, eq) in relations {
        grouped
            .entry((src_sys.clone(), tgt_sys.clone()))
            .or_default()
            .entry(src_code)
            .or_default()
            .push((tgt_code, eq));
    }

    let mut groups = Vec::new();
    for ((src_sys, tgt_sys), elements) in grouped {
        let mut element_arr = Vec::new();
        for (src_code, targets) in elements {
            let target_arr = targets
                .into_iter()
                .map(|(code, eq)| json!({ "code": code, "equivalence": eq }))
                .collect::<Vec<_>>();
            element_arr.push(json!({ "code": src_code, "target": target_arr }));
        }
        groups.push(json!({
            "source": src_sys,
            "target": tgt_sys,
            "element": element_arr
        }));
    }

    json!({
        "resourceType": "ConceptMap",
        "status": "active",
        "experimental": true,
        "version": current_version.to_string(),
        "date": Utc::now().format("%Y-%m-%d").to_string(),
        "group": groups
    })
}
