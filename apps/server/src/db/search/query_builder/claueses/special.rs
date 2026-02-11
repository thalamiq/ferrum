use crate::db::search::parameter_lookup::SearchParamType;

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam, SearchModifier};

// Import type-specific clause builders
use super::composite::build_composite_param_clause;
use super::date::{build_date_clause, build_last_updated_clause};
use super::number::build_number_clause;
use super::number::build_quantity_clause;
use super::reference::{
    build_reference_clause, build_reference_contains_hierarchy_clause,
    build_reference_identifier_clause,
};
use super::reverse_chain::build_reverse_chain_clause;
use super::string::{build_fulltext_clause, build_string_clause};
use super::token::{
    build_token_clause, build_token_not_clause, build_token_not_in_clause,
    build_token_oftype_clause,
};
use super::uri::build_uri_clause;

/// Main entry point for building search parameter clauses.
/// Routes to type-specific builders based on parameter type and modifiers.
pub(in crate::db::search::query_builder) fn build_param_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    searched_resource_type: Option<&str>,
) -> Option<String> {
    build_param_clause_for_resource(resolved, bind_params, base_url, searched_resource_type, "r")
}

pub(crate) fn build_param_clause_for_resource(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    searched_resource_type: Option<&str>,
    resource_alias: &str,
) -> Option<String> {
    // Handle _has reverse chaining
    if resolved.code == "_has" && resolved.reverse_chain.is_some() {
        return build_reverse_chain_clause(
            resolved,
            bind_params,
            base_url,
            searched_resource_type,
            resource_alias,
        );
    }

    // Handle reference chaining
    if resolved.param_type == SearchParamType::Reference {
        if let Some(chain) = resolved.chain.as_deref() {
            match chain {
                "_in" => {
                    return build_reference_membership_in_clause(
                        resolved,
                        bind_params,
                        resource_alias,
                    );
                }
                "_list" => {
                    return build_reference_membership_list_clause(
                        resolved,
                        bind_params,
                        resource_alias,
                    );
                }
                _ => {
                    // Standard chaining (e.g., subject.name=peter)
                    if resolved.chain_metadata.is_some() {
                        return build_chained_parameter_clause(
                            resolved,
                            bind_params,
                            base_url,
                            resource_alias,
                        );
                    }
                    return None;
                }
            }
        }
    }

    if resolved.chain.is_some() && resolved.chain_metadata.is_none() {
        // Chain without metadata (shouldn't happen if resolve worked correctly)
        return None;
    }

    if resolved.param_type == SearchParamType::Composite {
        return build_composite_param_clause(resolved, bind_params, base_url, resource_alias);
    }

    // `:identifier` on reference parameters searches Reference.identifier (token semantics).
    if resolved.param_type == SearchParamType::Reference
        && matches!(resolved.modifier, Some(SearchModifier::Identifier))
    {
        return build_reference_identifier_clause(resolved, bind_params, resource_alias);
    }

    // `:contains` on hierarchical reference parameters returns the specified resource,
    // its ancestors, and its descendants in the hierarchy (FHIR 3.2.1.5.5.4.1).
    if resolved.param_type == SearchParamType::Reference
        && matches!(resolved.modifier, Some(SearchModifier::Contains))
    {
        return build_reference_contains_hierarchy_clause(
            resolved,
            bind_params,
            base_url,
            searched_resource_type,
            resource_alias,
        );
    }

    // Token :not applies to the set of values on the resource, not per-row.
    if resolved.param_type == SearchParamType::Token
        && matches!(resolved.modifier, Some(SearchModifier::Not))
    {
        return build_token_not_clause(resolved, bind_params, resource_alias);
    }

    // Token :not-in applies to the set of values (like :not) — the resource matches if
    // none of its token values are members of the given ValueSet expansion.
    if resolved.param_type == SearchParamType::Token
        && matches!(resolved.modifier, Some(SearchModifier::NotIn))
    {
        return build_token_not_in_clause(resolved, bind_params, resource_alias);
    }

    // Token :of-type matches Identifier.type + Identifier.value within the same Identifier element.
    if resolved.param_type == SearchParamType::Token
        && matches!(resolved.modifier, Some(SearchModifier::OfType))
    {
        return build_token_oftype_clause(resolved, bind_params, resource_alias);
    }

    // Special parameters operate on the `resources` table directly.
    if resolved.param_type == SearchParamType::Special {
        return build_special_clause(resolved, bind_params, resource_alias);
    }

    let table = resolved.param_type.table_name();

    // :missing modifier uses existence semantics.
    if matches!(resolved.modifier, Some(SearchModifier::Missing)) {
        let desired_missing = parse_missing_bool(resolved).ok()?;
        let param_name_idx = push_text(bind_params, resolved.code.clone());
        let exists = format!(
            "EXISTS (SELECT 1 FROM {} sp WHERE sp.resource_type = {}.resource_type AND sp.resource_id = {}.id AND sp.version_id = {}.version_id AND sp.parameter_name = ${})",
            table,
            resource_alias,
            resource_alias,
            resource_alias,
            param_name_idx
        );
        return Some(if desired_missing {
            format!("NOT {}", exists)
        } else {
            exists
        });
    }

    let mut sub = format!(
        "SELECT 1 FROM {} sp WHERE sp.resource_type = {}.resource_type AND sp.resource_id = {}.id AND sp.version_id = {}.version_id",
        table,
        resource_alias,
        resource_alias,
        resource_alias
    );
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    sub.push_str(&format!(" AND sp.parameter_name = ${}", param_name_idx));

    let value_clause = build_value_clause(resolved, bind_params, base_url);
    if let Some(value_clause) = value_clause {
        sub.push_str(" AND ");
        sub.push_str(&value_clause);
    }

    Some(format!("EXISTS ({})", sub))
}

fn build_special_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    match resolved.code.as_str() {
        "_id" => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let idx = push_text(bind_params, v.raw.clone());
                parts.push(format!("{}.id = ${}", resource_alias, idx));
            }
            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }
        "_lastUpdated" => build_last_updated_clause(resolved, bind_params, resource_alias),
        "_in" => build_membership_in_clause(resolved, bind_params, resource_alias),
        "_list" => build_membership_list_clause(resolved, bind_params, resource_alias),
        _ => None,
    }
}

fn parse_missing_bool(resolved: &ResolvedParam) -> Result<bool, ()> {
    let v = resolved
        .values
        .first()
        .map(|v| v.raw.to_ascii_lowercase())
        .ok_or(())?;
    match v.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(()),
    }
}

fn active_membership_predicate(alias: &str) -> String {
    format!(
        "{alias}.member_inactive = false AND ({alias}.period_start IS NULL OR {alias}.period_start <= NOW()) AND ({alias}.period_end IS NULL OR {alias}.period_end >= NOW())"
    )
}

fn membership_target_filter(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    collection_type_col: &str,
    collection_id_col: &str,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let raw = v.raw.trim();
        if raw.is_empty() {
            continue;
        }

        if let Some((typ, id)) = raw.split_once('/') {
            let typ_idx = push_text(bind_params, typ.to_string());
            let id_idx = push_text(bind_params, id.to_string());
            parts.push(format!(
                "({collection_type_col} = ${} AND {collection_id_col} = ${})",
                typ_idx, id_idx
            ));
        } else {
            let id_idx = push_text(bind_params, raw.to_string());
            parts.push(format!(
                "({collection_id_col} = ${} AND {collection_type_col} IN ('CareTeam','Group','List'))",
                id_idx
            ));
        }
    }

    if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}

fn membership_list_id_filter(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let raw = v.raw.trim();
        if raw.is_empty() {
            continue;
        }
        let id_idx = push_text(bind_params, raw.to_string());
        parts.push(format!("ml.list_id = ${}", id_idx));
    }
    if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}

fn build_membership_in_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    // :missing applies to existence of *any* active membership.
    if matches!(resolved.modifier, Some(SearchModifier::Missing)) {
        let desired_missing = parse_missing_bool(resolved).ok()?;
        let active = active_membership_predicate("mi");
        let exists = format!(
            "EXISTS (SELECT 1 FROM search_membership_in mi WHERE mi.member_type = {}.resource_type AND mi.member_id = {}.id AND {active})",
            resource_alias,
            resource_alias
        );
        return Some(if desired_missing {
            format!("NOT {}", exists)
        } else {
            exists
        });
    }

    let active = active_membership_predicate("mi");
    let target = membership_target_filter(
        resolved,
        bind_params,
        "mi.collection_type",
        "mi.collection_id",
    )?;

    let exists = format!(
        "EXISTS (SELECT 1 FROM search_membership_in mi WHERE mi.member_type = {}.resource_type AND mi.member_id = {}.id AND {active} AND {target})",
        resource_alias,
        resource_alias
    );

    if matches!(resolved.modifier, Some(SearchModifier::Not)) {
        Some(format!("NOT {}", exists))
    } else {
        Some(exists)
    }
}

fn build_membership_list_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    // :missing applies to existence of membership in *any* List.
    if matches!(resolved.modifier, Some(SearchModifier::Missing)) {
        let desired_missing = parse_missing_bool(resolved).ok()?;
        let exists = format!(
            "EXISTS (SELECT 1 FROM search_membership_list ml WHERE ml.member_type = {}.resource_type AND ml.member_id = {}.id)",
            resource_alias,
            resource_alias
        );
        return Some(if desired_missing {
            format!("NOT {}", exists)
        } else {
            exists
        });
    }

    let list_filter = membership_list_id_filter(resolved, bind_params)?;
    let exists = format!(
        "EXISTS (SELECT 1 FROM search_membership_list ml WHERE ml.member_type = {}.resource_type AND ml.member_id = {}.id AND {list_filter})",
        resource_alias,
        resource_alias
    );

    if matches!(resolved.modifier, Some(SearchModifier::Not)) {
        Some(format!("NOT {}", exists))
    } else {
        Some(exists)
    }
}

fn build_reference_membership_in_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    let active = active_membership_predicate("mi");
    let target = membership_target_filter(
        resolved,
        bind_params,
        "mi.collection_type",
        "mi.collection_id",
    )?;

    let exists = format!(
        "EXISTS (SELECT 1 FROM search_reference sr INNER JOIN search_membership_in mi ON mi.member_type = sr.target_type AND mi.member_id = sr.target_id WHERE sr.resource_type = {}.resource_type AND sr.resource_id = {}.id AND sr.version_id = {}.version_id AND sr.parameter_name = ${} AND {active} AND {target})",
        resource_alias,
        resource_alias,
        resource_alias,
        param_name_idx
    );

    if matches!(resolved.modifier, Some(SearchModifier::Not)) {
        Some(format!("NOT {}", exists))
    } else {
        Some(exists)
    }
}

fn build_reference_membership_list_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    let list_filter = membership_list_id_filter(resolved, bind_params)?;

    let exists = format!(
        "EXISTS (SELECT 1 FROM search_reference sr INNER JOIN search_membership_list ml ON ml.member_type = sr.target_type AND ml.member_id = sr.target_id WHERE sr.resource_type = {}.resource_type AND sr.resource_id = {}.id AND sr.version_id = {}.version_id AND sr.parameter_name = ${} AND {list_filter})",
        resource_alias,
        resource_alias,
        resource_alias,
        param_name_idx
    );

    if matches!(resolved.modifier, Some(SearchModifier::Not)) {
        Some(format!("NOT {}", exists))
    } else {
        Some(exists)
    }
}

fn build_value_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
) -> Option<String> {
    match resolved.param_type {
        SearchParamType::String => build_string_clause(resolved, bind_params),
        SearchParamType::Token => build_token_clause(resolved, bind_params),
        SearchParamType::Date => build_date_clause(resolved, bind_params),
        SearchParamType::Number => build_number_clause(resolved, bind_params),
        SearchParamType::Quantity => build_quantity_clause(resolved, bind_params),
        SearchParamType::Reference => build_reference_clause(resolved, bind_params, base_url),
        SearchParamType::Uri => build_uri_clause(resolved, bind_params),
        SearchParamType::Text => build_fulltext_clause("sp.content", resolved, bind_params),
        SearchParamType::Content => build_fulltext_clause("sp.content", resolved, bind_params),
        SearchParamType::Composite | SearchParamType::Special => None,
    }
}

/// Build SQL clause for chained parameter search
/// Example: DiagnosticReport?subject.name=peter
/// Joins through search_reference to the target resource, then applies chained parameter filter
fn build_chained_parameter_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    resource_alias: &str,
) -> Option<String> {
    let chain_meta = resolved.chain_metadata.as_ref()?;

    // Build WHERE clause for reference parameter (base of the chain)
    let param_name_idx = push_text(bind_params, resolved.code.clone());

    // Build target type filter
    let target_types = &chain_meta.target_types;
    let target_type_filter = if target_types.len() == 1 {
        let type_idx = push_text(bind_params, target_types[0].clone());
        format!("sr.target_type = ${}", type_idx)
    } else {
        let type_array_idx = super::super::bind::push_text_array(bind_params, target_types.clone());
        format!("sr.target_type = ANY(${})", type_array_idx)
    };

    // Create a temporary ResolvedParam for the chained parameter to reuse existing builders
    let chain_param = ResolvedParam {
        raw_name: chain_meta.param_code.clone(),
        code: chain_meta.param_code.clone(),
        param_type: chain_meta.param_type.clone(),
        modifier: chain_meta.modifier.clone(),
        chain: None,
        values: resolved.values.clone(), // Use the search values from the original query
        composite: None,
        reverse_chain: None,
        chain_metadata: None,
    };

    // Build the filter clause for the chained parameter on the target resource
    // Use "target_r" as the resource alias for the target resource
    let chain_filter = build_param_clause_for_resource(
        &chain_param,
        bind_params,
        base_url,
        target_types.first().map(|s| s.as_str()),
        "target_r",
    )?;

    // Build EXISTS clause that:
    // 1. Joins through search_reference to find referenced resources
    // 2. Joins to the resources table to get current target resources
    // 3. Applies the chained parameter filter on the target resource
    Some(format!(
        "EXISTS (
            SELECT 1
            FROM search_reference sr
            INNER JOIN resources target_r
                ON target_r.resource_type = sr.target_type
                AND target_r.id = sr.target_id
                AND target_r.is_current = true
                AND target_r.deleted = false
            WHERE sr.resource_type = {alias}.resource_type
                AND sr.resource_id = {alias}.id
                AND sr.version_id = {alias}.version_id
                AND sr.parameter_name = ${param_idx}
                AND {target_filter}
                AND (sr.reference_kind = 'relative' OR sr.reference_kind = 'absolute')
                AND {chain_clause}
        )",
        alias = resource_alias,
        param_idx = param_name_idx,
        target_filter = target_type_filter,
        chain_clause = chain_filter
    ))
}
