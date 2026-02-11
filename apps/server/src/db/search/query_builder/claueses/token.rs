use crate::db::search::escape::{split_unescaped, unescape_search_value};

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam, SearchModifier};

pub(in crate::db::search::query_builder) enum TokenSearchValue {
    AnySystemCode(String),
    NoSystemCode(String),
    SystemOnly(String),
    SystemCode { system: String, code: String },
}

pub(in crate::db::search::query_builder) fn parse_token_value(raw: &str) -> TokenSearchValue {
    let parts = split_unescaped(raw, '|');
    match parts.len() {
        1 => {
            let code = unescape_search_value(parts[0]).unwrap_or_else(|_| parts[0].to_string());
            TokenSearchValue::AnySystemCode(code)
        }
        2 => {
            let left = unescape_search_value(parts[0]).unwrap_or_else(|_| parts[0].to_string());
            let right = unescape_search_value(parts[1]).unwrap_or_else(|_| parts[1].to_string());
            if left.is_empty() {
                return TokenSearchValue::NoSystemCode(right);
            }
            if right.is_empty() {
                return TokenSearchValue::SystemOnly(left);
            }
            TokenSearchValue::SystemCode {
                system: left,
                code: right,
            }
        }
        _ => TokenSearchValue::AnySystemCode(raw.to_string()),
    }
}

pub(in crate::db::search::query_builder) fn build_token_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    match &resolved.modifier {
        // :text searches display text with starts-with or is matching
        Some(SearchModifier::Text) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
                if raw_unescaped.trim().is_empty() {
                    continue;
                }
                let pattern = format!("{}%", escape_like_pattern(&raw_unescaped));
                let idx = push_text(bind_params, pattern);
                parts.push(format!("sp.display ILIKE ${} ESCAPE E'\\\\'", idx));
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }

        // :code-text searches code with basic string matching (starts with or is, case-insensitive)
        Some(SearchModifier::CodeText) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
                if raw_unescaped.trim().is_empty() {
                    continue;
                }
                let pattern = format!("{}%", escape_like_pattern(&raw_unescaped));
                let idx = push_text(bind_params, pattern);
                parts.push(format!("sp.code ILIKE ${} ESCAPE E'\\\\'", idx));
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }

        // :text-advanced uses full-text search on display
        Some(SearchModifier::TextAdvanced) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                if v.raw.is_empty() {
                    continue;
                }
                let idx = push_text(bind_params, v.raw.clone());
                parts.push(format!(
                    "to_tsvector('simple', sp.display) @@ websearch_to_tsquery('simple', ${})",
                    idx
                ));
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }

        // :above - hierarchical subsumption (value in resource is or subsumes search value)
        Some(SearchModifier::Above) => {
            // For tokens, this typically means checking if the code is in a hierarchy
            // Basic implementation: substring matching on code
            let mut parts = Vec::new();
            for v in &resolved.values {
                let pattern = format!("{}%", v.raw);
                let idx = push_text(bind_params, pattern);
                parts.push(format!("sp.code LIKE ${}", idx));
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }

        // :below - hierarchical subsumption (value in resource is subsumed by search value)
        Some(SearchModifier::Below) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let idx = push_text(bind_params, v.raw.clone());
                parts.push(format!("${} LIKE sp.code || '%'", idx));
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }

        // :in - value is in the supplied ValueSet
        Some(SearchModifier::In) => build_token_in_clause(resolved, bind_params),

        // :not-in is handled at a higher level (set semantics, like :not)
        Some(SearchModifier::NotIn) => None,

        // :of-type - for Identifier type tokens
        Some(SearchModifier::OfType) => {
            // This searches for Identifier.type matching the value
            // Requires special handling - not fully implemented
            None
        }

        // :not - negation
        Some(SearchModifier::Not) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let clause = match parse_token_value(&v.raw) {
                    TokenSearchValue::SystemCode { system, code } => {
                        let sys_idx = push_text(bind_params, system);
                        let code_idx = push_text(bind_params, code);
                        format!("NOT (sp.system = ${} AND sp.code = ${})", sys_idx, code_idx)
                    }
                    TokenSearchValue::NoSystemCode(code) => {
                        let code_idx = push_text(bind_params, code);
                        format!("NOT (sp.system IS NULL AND sp.code = ${})", code_idx)
                    }
                    TokenSearchValue::SystemOnly(system) => {
                        let sys_idx = push_text(bind_params, system);
                        format!("sp.system != ${}", sys_idx)
                    }
                    TokenSearchValue::AnySystemCode(code) => {
                        let code_idx = push_text(bind_params, code);
                        format!("sp.code != ${}", code_idx)
                    }
                };
                parts.push(clause);
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                // For :not with multiple values, ALL must not match (AND logic)
                Some(format!("({})", parts.join(" AND ")))
            }
        }

        // Default token search (no modifier or other modifiers)
        None | Some(_) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let ts = parse_token_value(&v.raw);
                let clause = token_match_clause("sp", &ts, bind_params);
                parts.push(clause);
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }
    }
}

fn escape_like_pattern(s: &str) -> String {
    // Escape SQL LIKE meta-characters so user input is treated literally.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

pub(in crate::db::search::query_builder) fn build_token_not_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    let mut parts = Vec::new();
    for v in &resolved.values {
        let ts = parse_token_value(v.raw.as_str());
        let clause = token_match_clause("st", &ts, bind_params);
        parts.push(clause);
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "NOT EXISTS (SELECT 1 FROM search_token st WHERE st.resource_type = {}.resource_type AND st.resource_id = {}.id AND st.version_id = {}.version_id AND st.parameter_name = ${} AND ({}))",
        resource_alias,
        resource_alias,
        resource_alias,
        param_name_idx,
        parts.join(" OR ")
    ))
}

pub(in crate::db::search::query_builder) fn build_token_oftype_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    let mut parts = Vec::new();
    for v in &resolved.values {
        let Some((type_system, type_code, value)) = parse_token_oftype_value(v.raw.as_str()) else {
            continue;
        };

        let ts = type_system.as_deref()?;
        let ts_idx = push_text(bind_params, ts.to_string());
        let type_system_clause = format!("si.type_system = ${}", ts_idx);

        let case_sensitive = type_system
            .as_deref()
            .map(is_case_sensitive_token_system)
            .unwrap_or(false);

        let type_code_clause = exact_ci_match(
            "si",
            "type_code",
            "type_code_ci",
            &type_code,
            bind_params,
            case_sensitive,
        );
        let value_clause = exact_ci_match("si", "value", "value_ci", &value, bind_params, false);

        parts.push(format!(
            "({} AND {} AND {})",
            type_system_clause, type_code_clause, value_clause
        ));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "EXISTS (SELECT 1 FROM search_token_identifier si WHERE si.resource_type = {}.resource_type AND si.resource_id = {}.id AND si.version_id = {}.version_id AND si.parameter_name = ${} AND ({}))",
        resource_alias,
        resource_alias,
        resource_alias,
        param_name_idx,
        parts.join(" OR ")
    ))
}

fn parse_token_oftype_value(raw: &str) -> Option<(Option<String>, String, String)> {
    let parts = split_unescaped(raw, '|');
    if parts.len() != 3 {
        return None;
    }
    let type_system = unescape_search_value(parts[0]).ok()?;
    let type_code = unescape_search_value(parts[1]).ok()?;
    let value = unescape_search_value(parts[2]).ok()?;
    let type_system = type_system.trim();
    let type_code = type_code.trim();
    let value = value.trim();
    if type_system.is_empty() || type_code.is_empty() || value.is_empty() {
        return None;
    }
    Some((
        Some(type_system.to_string()),
        type_code.to_string(),
        value.to_string(),
    ))
}

pub(in crate::db::search::query_builder) fn is_case_sensitive_token_system(system: &str) -> bool {
    // Known case-sensitive systems (non-exhaustive).
    matches!(system, "http://unitsofmeasure.org")
}

pub(in crate::db::search::query_builder) fn exact_ci_match(
    alias: &str,
    col: &str,
    col_ci: &str,
    raw: &str,
    bind_params: &mut Vec<BindValue>,
    case_sensitive: bool,
) -> String {
    let raw_idx = push_text(bind_params, raw.to_string());
    if case_sensitive {
        return format!("{alias}.{col} = ${raw_idx}");
    }
    let ci = raw.to_lowercase();
    let ci_idx = push_text(bind_params, ci);
    format!(
        "({alias}.{col} = ${raw} OR {alias}.{col_ci} = ${ci} OR ({alias}.{col_ci} = '' AND {alias}.{col} ILIKE ${raw}))",
        alias = alias,
        col = col,
        col_ci = col_ci,
        raw = raw_idx,
        ci = ci_idx
    )
}

fn token_match_clause(
    alias: &str,
    ts: &TokenSearchValue,
    bind_params: &mut Vec<BindValue>,
) -> String {
    match ts {
        TokenSearchValue::SystemCode { system, code } => {
            let sys_idx = push_text(bind_params, system.clone());
            let case_sensitive = is_case_sensitive_token_system(system.as_str());
            format!(
                "({alias}.system = ${sys} AND {code_clause})",
                alias = alias,
                sys = sys_idx,
                code_clause =
                    exact_ci_match(alias, "code", "code_ci", code, bind_params, case_sensitive)
            )
        }
        TokenSearchValue::NoSystemCode(code) => format!(
            "({alias}.system IS NULL AND {code_clause})",
            alias = alias,
            code_clause = exact_ci_match(alias, "code", "code_ci", code, bind_params, false)
        ),
        TokenSearchValue::SystemOnly(system) => {
            let sys_idx = push_text(bind_params, system.clone());
            format!("({alias}.system = ${})", sys_idx)
        }
        TokenSearchValue::AnySystemCode(code) => {
            exact_ci_match(alias, "code", "code_ci", code, bind_params, false)
        }
    }
}

/// Build the value-level clause for `:in` modifier.
///
/// Returns a predicate that correlates with the outer `sp` alias from the
/// enclosing `EXISTS (SELECT 1 FROM search_token sp ...)` subquery.
///
/// Matches tokens whose (system, code) appear in any cached expansion of
/// the supplied ValueSet URL(s). If no expansion is cached, the subquery
/// returns no rows (the search silently misses — callers should `$expand`
/// the ValueSet first).
fn build_token_in_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let vs_url_filter = build_vs_url_filter(resolved, bind_params)?;
    Some(format!(
        "EXISTS (SELECT 1 FROM valueset_expansions ve \
         JOIN valueset_expansion_concepts vec ON vec.expansion_id = ve.id \
         WHERE {vs_url_filter} \
         AND (ve.expires_at IS NULL OR ve.expires_at > NOW()) \
         AND vec.system = sp.system AND vec.code = sp.code)"
    ))
}

/// Build a top-level clause for `:not-in` modifier (set semantics).
///
/// Like `:not`, this operates on the set of token values on the resource:
/// the resource matches if it has NO token values that are members of the
/// given ValueSet expansion.
pub(in crate::db::search::query_builder) fn build_token_not_in_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    let vs_url_filter = build_vs_url_filter(resolved, bind_params)?;
    Some(format!(
        "NOT EXISTS (SELECT 1 FROM search_token sp \
         JOIN valueset_expansions ve ON ({vs_url_filter} \
         AND (ve.expires_at IS NULL OR ve.expires_at > NOW())) \
         JOIN valueset_expansion_concepts vec ON vec.expansion_id = ve.id \
         AND vec.system = sp.system AND vec.code = sp.code \
         WHERE sp.resource_type = {a}.resource_type \
         AND sp.resource_id = {a}.id \
         AND sp.version_id = {a}.version_id \
         AND sp.parameter_name = ${pn})",
        a = resource_alias,
        pn = param_name_idx,
        vs_url_filter = vs_url_filter,
    ))
}

/// Parse ValueSet URL(s) from the search values and return a SQL filter fragment.
fn build_vs_url_filter(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let url = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
        let url = url.trim().to_string();
        if url.is_empty() {
            continue;
        }
        let idx = push_text(bind_params, url);
        parts.push(format!("ve.valueset_url = ${}", idx));
    }

    if parts.is_empty() {
        return None;
    }

    if parts.len() == 1 {
        Some(parts.remove(0))
    } else {
        Some(format!("({})", parts.join(" OR ")))
    }
}
