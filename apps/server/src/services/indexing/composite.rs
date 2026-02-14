//! Composite search parameter indexing (tuple semantics).

use crate::db::search::string_normalization::normalize_string_for_search;
use crate::models::Resource;
use crate::Result;
use serde_json::Value;
use ferrum_fhirpath::{conversion::ToJson, Context, EvalOptions, Value as FhirPathValue};

use super::IndexingService;
use super::SearchParameter;
use super::{
    extract_date_ranges, extract_numbers, extract_quantity_values, extract_reference_values,
    extract_strings, extract_tokens,
};

impl IndexingService {
    pub(super) async fn insert_composite_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param: &SearchParameter,
    ) -> Result<()> {
        let Some(components_json) = param.components.as_ref() else {
            return Ok(());
        };

        let Some(components) = components_json.as_array() else {
            return Ok(());
        };

        if components.len() < 2 {
            return Ok(());
        }

        let component_defs = parse_composite_component_defs(components, &resource.resource_type);
        if component_defs.len() < 2 {
            return Ok(());
        }

        let group_root_expr = compute_composite_group_root_expr(&component_defs);

        let root = FhirPathValue::from_json(resource.resource.clone());
        let ctx = Context::new(root);

        let group_items: Vec<Value> = if group_root_expr.is_empty() {
            vec![resource.resource.clone()]
        } else {
            let collection = self
                .fhirpath_engine
                .evaluate_expr_with_options(
                    &group_root_expr,
                    &ctx,
                    EvalOptions {
                        base_type: Some(resource.resource_type.clone()),
                        strict: false,
                        infer_base_type: false,
                    },
                )
                .map_err(|e| crate::Error::FhirPath(e.to_string()))?;
            collection.iter().filter_map(|v| v.to_json()).collect()
        };

        for group_item in group_items {
            let group_root = FhirPathValue::from_json(group_item.clone());
            let group_ctx = Context::new(group_root);

            let mut per_component_values: Vec<Vec<Value>> = Vec::new();
            let mut missing_component = false;

            for c in &component_defs {
                let raw_values: Vec<Value> = if c.tail_expr.is_empty() {
                    vec![group_item.clone()]
                } else {
                    let collection = self
                        .fhirpath_engine
                        .evaluate_expr_with_options(
                            &c.tail_expr,
                            &group_ctx,
                            EvalOptions {
                                base_type: None,
                                strict: false,
                                infer_base_type: false,
                            },
                        )
                        .map_err(|e| crate::Error::FhirPath(e.to_string()))?;
                    collection.iter().filter_map(|v| v.to_json()).collect()
                };

                let indexed_values = index_component_values(&c.component_type, &raw_values);
                if indexed_values.is_empty() {
                    missing_component = true;
                    break;
                }
                per_component_values.push(indexed_values);
            }

            if missing_component || per_component_values.is_empty() {
                continue;
            }

            // Generate tuples (cartesian product) within this group item.
            let tuples = cartesian_product_capped(&per_component_values, 2000);
            for tuple in tuples {
                let components_value = Value::Array(tuple);
                sqlx::query(
                    "INSERT INTO search_composite (resource_type, resource_id, version_id, parameter_name, components)
                     VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
                )
                .bind(&resource.resource_type)
                .bind(&resource.id)
                .bind(resource.version_id)
                .bind(&param.code)
                .bind(components_value)
                .execute(&mut **tx)
                .await
                .map_err(crate::Error::Database)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CompositeComponentIndexDef {
    component_type: String,
    full_expr: String,
    tail_expr: String,
    grouping_segments: Vec<String>,
}

fn parse_composite_component_defs(
    components: &[serde_json::Value],
    resource_type: &str,
) -> Vec<CompositeComponentIndexDef> {
    let mut out = Vec::new();
    for comp in components {
        let _component_code = comp.get("component_code").and_then(|v| v.as_str());
        let component_type = comp.get("component_type").and_then(|v| v.as_str());
        let expression = comp.get("expression").and_then(|v| v.as_str());
        let (Some(_component_code), Some(component_type), Some(expression)) =
            (_component_code, component_type, expression)
        else {
            continue;
        };

        let mut expr = expression.trim().to_string();
        if let Some(prefix) = expr.strip_prefix(&format!("{}.", resource_type)) {
            expr = prefix.to_string();
        }

        let grouping_expr = expr.split('|').next().unwrap_or("").trim().to_string();

        let grouping_segments = grouping_expr
            .split('.')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        out.push(CompositeComponentIndexDef {
            component_type: component_type.to_string(),
            full_expr: expr,
            tail_expr: String::new(),
            grouping_segments,
        });
    }

    let group_root_segments = compute_composite_group_root_segments(&out);
    let group_root_expr = group_root_segments.join(".");
    for c in out.iter_mut() {
        if group_root_expr.is_empty() {
            c.tail_expr = c.full_expr.clone();
        } else if c.full_expr == group_root_expr {
            c.tail_expr = String::new();
        } else if let Some(rest) = c.full_expr.strip_prefix(&(group_root_expr.clone() + ".")) {
            c.tail_expr = rest.to_string();
        } else {
            // Fallback: evaluate relative to group item anyway.
            c.tail_expr = c.full_expr.clone();
        }
    }

    out
}

fn compute_composite_group_root_expr(components: &[CompositeComponentIndexDef]) -> String {
    compute_composite_group_root_segments(components).join(".")
}

fn compute_composite_group_root_segments(components: &[CompositeComponentIndexDef]) -> Vec<String> {
    let mut parents = Vec::new();
    for c in components {
        if c.grouping_segments.len() >= 2 {
            parents.push(&c.grouping_segments[..c.grouping_segments.len() - 1]);
        } else {
            parents.push(&[][..]);
        }
    }

    let mut prefix: Vec<String> = Vec::new();
    if parents.is_empty() {
        return prefix;
    }

    let min_len = parents.iter().map(|p| p.len()).min().unwrap_or(0);
    for i in 0..min_len {
        let seg = parents[0][i].to_string();
        if parents.iter().all(|p| p[i] == seg) {
            prefix.push(seg);
        } else {
            break;
        }
    }
    prefix
}

fn index_component_values(component_type: &str, raw_values: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();

    for value in raw_values {
        match component_type {
            "token" => {
                for token in extract_tokens(value) {
                    let code = match token.code {
                        Some(code) if !code.is_empty() => code,
                        _ => continue,
                    };
                    let system = match token.system {
                        Some(system) if !system.is_empty() => Some(system),
                        _ => None,
                    };
                    let display = match token.display {
                        Some(display) if !display.is_empty() => Some(display),
                        _ => None,
                    };
                    out.push(serde_json::json!({
                        "system": system,
                        "code": code,
                        "code_ci": code.to_lowercase(),
                        "display": display,
                    }));
                }
            }
            "quantity" => {
                for q in extract_quantity_values(value) {
                    out.push(serde_json::json!({
                        "value": q.value.to_string(),
                        "system": q.system,
                        "code": q.code,
                        "unit": q.unit,
                    }));
                }
            }
            "number" => {
                for n in extract_numbers(value) {
                    out.push(serde_json::json!({ "value": n.to_string() }));
                }
            }
            "date" => {
                for (start, end) in extract_date_ranges(value) {
                    out.push(serde_json::json!({
                        "start": start.to_rfc3339(),
                        "end": end.to_rfc3339(),
                    }));
                }
            }
            "string" => {
                for s in extract_strings(value) {
                    let normalized = normalize_string_for_search(&s);
                    out.push(serde_json::json!({
                        "value": s,
                        "value_normalized": normalized,
                    }));
                }
            }
            "reference" => {
                for r in extract_reference_values(value) {
                    out.push(serde_json::json!({
                        "reference_kind": r.reference_kind.as_str(),
                        "target_type": r.target_type,
                        "target_id": r.target_id,
                        "target_version_id": r.target_version_id,
                        "target_url": r.target_url,
                        "canonical_url": r.canonical_url,
                        "canonical_version": r.canonical_version,
                    }));
                }
            }
            "uri" => {
                for s in extract_strings(value) {
                    out.push(serde_json::json!({ "value": s }));
                }
            }
            _ => {}
        }
    }

    out
}

fn cartesian_product_capped(lists: &[Vec<Value>], max: usize) -> Vec<Vec<Value>> {
    if lists.is_empty() {
        return Vec::new();
    }
    let mut results: Vec<Vec<Value>> = vec![Vec::new()];
    for list in lists {
        if list.is_empty() {
            return Vec::new();
        }
        let mut next = Vec::new();
        for prefix in &results {
            for v in list {
                if next.len() >= max {
                    return next;
                }
                let mut new_tuple = prefix.clone();
                new_tuple.push(v.clone());
                next.push(new_tuple);
            }
        }
        results = next;
        if results.len() >= max {
            results.truncate(max);
            break;
        }
    }
    results
}
