use crate::db::search::escape::{split_unescaped, unescape_search_value};
use crate::db::search::parameter_lookup::SearchParamType;

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam};

// Import type-specific component builders
use super::date::build_date_json_clause;
use super::number::{build_number_json_clause, build_quantity_json_clause};
use super::reference::build_reference_json_clause;
use super::string::build_string_json_clause;
use super::token::{is_case_sensitive_token_system, parse_token_value, TokenSearchValue};

pub(in crate::db::search::query_builder) fn build_composite_param_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    resource_alias: &str,
) -> Option<String> {
    let meta = resolved.composite.as_ref()?;
    if meta.components.is_empty() {
        return None;
    }

    let param_name_idx = push_text(bind_params, resolved.code.clone());
    let mut tuple_parts = Vec::new();

    for v in &resolved.values {
        let Ok(tuple) = parse_composite_tuple(v.raw.as_str(), meta.components.len()) else {
            continue;
        };
        let mut comp_clauses = Vec::new();
        for (idx, component_meta) in meta.components.iter().enumerate() {
            let raw_part = tuple.get(idx).map(|s| s.as_str()).unwrap_or("");
            let Some(clause) = build_composite_component_clause(
                idx,
                component_meta.param_type.clone(),
                raw_part,
                bind_params,
                base_url,
            ) else {
                comp_clauses.clear();
                break;
            };
            comp_clauses.push(clause);
        }
        if comp_clauses.len() == meta.components.len() {
            tuple_parts.push(format!("({})", comp_clauses.join(" AND ")));
        }
    }

    if tuple_parts.is_empty() {
        return None;
    }

    Some(format!(
        "EXISTS (SELECT 1 FROM search_composite sc WHERE sc.resource_type = {}.resource_type AND sc.resource_id = {}.id AND sc.version_id = {}.version_id AND sc.parameter_name = ${} AND ({}))",
        resource_alias,
        resource_alias,
        resource_alias,
        param_name_idx,
        tuple_parts.join(" OR ")
    ))
}

pub(in crate::db::search::query_builder) fn build_composite_component_clause(
    idx: usize,
    param_type: SearchParamType,
    raw_value: &str,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
) -> Option<String> {
    match param_type {
        SearchParamType::Token => {
            let ts = parse_token_value(raw_value);
            Some(token_match_json_clause(idx, &ts, bind_params))
        }
        SearchParamType::Quantity => build_quantity_json_clause(idx, raw_value, bind_params),
        SearchParamType::Number => build_number_json_clause(idx, raw_value, bind_params),
        SearchParamType::Date => build_date_json_clause(idx, raw_value, bind_params),
        SearchParamType::String => build_string_json_clause(idx, raw_value, bind_params),
        SearchParamType::Reference => {
            build_reference_json_clause(idx, raw_value, bind_params, base_url)
        }
        SearchParamType::Uri => {
            let v = unescape_search_value(raw_value).ok()?;
            let v_idx = push_text(bind_params, v);
            Some(format!("sc.components->{}->>'value' = ${}", idx, v_idx))
        }
        SearchParamType::Text
        | SearchParamType::Content
        | SearchParamType::Composite
        | SearchParamType::Special => None,
    }
}

fn token_match_json_clause(
    idx: usize,
    ts: &TokenSearchValue,
    bind_params: &mut Vec<BindValue>,
) -> String {
    let code_expr = format!("sc.components->{}->>'code'", idx);
    let code_ci_expr = format!("sc.components->{}->>'code_ci'", idx);
    let system_expr = format!("sc.components->{}->>'system'", idx);

    match ts {
        TokenSearchValue::SystemCode { system, code } => {
            let sys_idx = push_text(bind_params, system.clone());
            let case_sensitive = is_case_sensitive_token_system(system.as_str());
            let code_pred =
                exact_ci_match_expr(&code_expr, &code_ci_expr, code, bind_params, case_sensitive);
            format!("({} = ${} AND {})", system_expr, sys_idx, code_pred)
        }
        TokenSearchValue::NoSystemCode(code) => {
            let code_pred =
                exact_ci_match_expr(&code_expr, &code_ci_expr, code, bind_params, false);
            format!("({} IS NULL AND {})", system_expr, code_pred)
        }
        TokenSearchValue::SystemOnly(system) => {
            let sys_idx = push_text(bind_params, system.clone());
            format!("{} = ${}", system_expr, sys_idx)
        }
        TokenSearchValue::AnySystemCode(code) => {
            exact_ci_match_expr(&code_expr, &code_ci_expr, code, bind_params, false)
        }
    }
}

fn exact_ci_match_expr(
    expr: &str,
    expr_ci: &str,
    raw: &str,
    bind_params: &mut Vec<BindValue>,
    case_sensitive: bool,
) -> String {
    let raw_unescaped = unescape_search_value(raw).unwrap_or_else(|_| raw.to_string());
    let raw_idx = push_text(bind_params, raw_unescaped.clone());
    if case_sensitive {
        return format!("{expr} = ${raw_idx}");
    }
    let ci_idx = push_text(bind_params, raw_unescaped.to_lowercase());
    format!(
        "({expr} = ${raw} OR {expr_ci} = ${ci} OR (({expr_ci} IS NULL OR {expr_ci} = '') AND {expr} ILIKE ${raw}))",
        expr = expr,
        expr_ci = expr_ci,
        raw = raw_idx,
        ci = ci_idx
    )
}

pub(crate) fn parse_composite_tuple(raw: &str, expected: usize) -> Result<Vec<String>, ()> {
    let parts = split_unescaped(raw, '$');
    if parts.len() != expected {
        return Err(());
    }
    Ok(parts.into_iter().map(|s| s.trim().to_string()).collect())
}

pub(crate) fn validate_composite_component_value(
    param_type: SearchParamType,
    raw_value: &str,
    base_url: Option<&str>,
) -> bool {
    let mut bind_params = Vec::new();
    build_composite_component_clause(0, param_type, raw_value, &mut bind_params, base_url).is_some()
}
