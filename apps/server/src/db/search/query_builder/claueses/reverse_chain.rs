//! Reverse chaining (_has) clause builder
//!
//! Builds SQL EXISTS clauses for _has reverse chaining searches.
//! Example: Patient?_has:Observation:patient:code=1234-5
//! Finds Patients referenced by Observations (via patient param) where Observation has code=1234-5

use super::super::{BindValue, ResolvedParam, SearchValue};
use crate::db::search::parameter_lookup::SearchParamType;

pub fn build_reverse_chain_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    searched_resource_type: Option<&str>,
    resource_alias: &str,
) -> Option<String> {
    let spec = resolved.reverse_chain.as_ref()?;
    let searched_resource_type = searched_resource_type?;

    // Create a temporary ResolvedParam for the filter parameter
    // This allows us to reuse existing clause builders for the filter
    let filter_param = ResolvedParam {
        raw_name: spec.filter_param.clone(),
        code: spec.filter_param.clone(),
        param_type: infer_param_type_from_values(&resolved.values),
        modifier: None,
        chain: None,
        values: resolved.values.clone(),
        composite: None,
        reverse_chain: None,
        chain_metadata: None,
    };

    // Build the filter clause using existing builders
    let filter_clause = super::build_param_clause_for_resource(
        &filter_param,
        bind_params,
        base_url,
        Some(&spec.referring_resource),
        "ref_r",
    )?;

    // Build the reverse reference clause
    let target_type_idx =
        super::super::bind::push_text(bind_params, searched_resource_type.to_string());
    let resource_type_idx =
        super::super::bind::push_text(bind_params, spec.referring_resource.clone());
    let param_name_idx = super::super::bind::push_text(bind_params, spec.referring_param.clone());

    // EXISTS clause: find referring resources that:
    // 1. Match the filter
    // 2. Reference back to the searched resource
    Some(format!(
        r#"EXISTS (
            SELECT 1 FROM resources ref_r
            WHERE ref_r.is_current = true
              AND ref_r.deleted = false
              AND ref_r.resource_type = ${}
              AND ({})
              AND EXISTS (
                SELECT 1 FROM search_reference sr
                WHERE sr.resource_type = ref_r.resource_type
                  AND sr.resource_id = ref_r.id
                  AND sr.version_id = ref_r.version_id
                  AND sr.parameter_name = ${}
                  AND sr.target_type = ${}
                  AND sr.target_id = {}.id
              )
        )"#,
        resource_type_idx, filter_clause, param_name_idx, target_type_idx, resource_alias
    ))
}

/// Infer parameter type from search values
/// This is a simple heuristic - in practice the type is already known from resolution
fn infer_param_type_from_values(values: &[SearchValue]) -> SearchParamType {
    if values.is_empty() {
        return SearchParamType::String;
    }

    // Check if it looks like a date (has prefix like gt, lt)
    if values.iter().any(|v| v.prefix.is_some()) {
        return SearchParamType::Date;
    }

    // Check if it looks like a token (has |)
    if values.iter().any(|v| v.raw.contains('|')) {
        return SearchParamType::Token;
    }

    // Default to string
    SearchParamType::String
}
