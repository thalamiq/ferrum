use super::bind::{push_text, push_text_array};
use super::claueses;
use super::{BindValue, ResolvedParam};
use crate::db::search::params::ReverseChainSpec;
use crate::db::search::string_normalization::normalize_string_for_search;

#[derive(Debug, Clone)]
pub enum FilterExpr {
    Atom(FilterAtom),
    Has {
        spec: ReverseChainSpec,
        filter: Box<FilterExpr>,
    },
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
    Not(Box<FilterExpr>),
}

#[derive(Debug, Clone)]
pub struct FilterAtom {
    pub chain: Vec<FilterChainStep>,
    pub kind: FilterAtomKind,
}

#[derive(Debug, Clone)]
pub struct FilterChainStep {
    pub reference_param: String,
    /// Restrict the chained target resource types (empty means "any").
    pub target_types: Vec<String>,
    pub filter: Option<Box<FilterExpr>>,
}

#[derive(Debug, Clone)]
pub enum FilterAtomKind {
    Standard(ResolvedParam),
    StringEq { code: String, value: String },
    StringEndsWith { code: String, value: String },
    DateOverlaps { code: String, value: String },
    LastUpdatedOverlaps { value: String },
}

impl FilterExpr {
    pub(crate) fn build_sql(
        &self,
        bind_params: &mut Vec<BindValue>,
        base_url: Option<&str>,
        searched_resource_type: Option<&str>,
        resource_alias: &str,
    ) -> String {
        match self {
            Self::Atom(a) => a.build_sql(
                bind_params,
                base_url,
                searched_resource_type,
                resource_alias,
            ),
            Self::Has { spec, filter } => {
                build_has_sql(spec, filter, bind_params, base_url, resource_alias)
            }
            Self::And(a, b) => format!(
                "({} AND {})",
                a.build_sql(
                    bind_params,
                    base_url,
                    searched_resource_type,
                    resource_alias
                ),
                b.build_sql(
                    bind_params,
                    base_url,
                    searched_resource_type,
                    resource_alias
                )
            ),
            Self::Or(a, b) => format!(
                "({} OR {})",
                a.build_sql(
                    bind_params,
                    base_url,
                    searched_resource_type,
                    resource_alias
                ),
                b.build_sql(
                    bind_params,
                    base_url,
                    searched_resource_type,
                    resource_alias
                )
            ),
            Self::Not(inner) => format!(
                "NOT ({})",
                inner.build_sql(
                    bind_params,
                    base_url,
                    searched_resource_type,
                    resource_alias
                )
            ),
        }
    }
}

impl FilterAtom {
    fn build_sql(
        &self,
        bind_params: &mut Vec<BindValue>,
        base_url: Option<&str>,
        searched_resource_type: Option<&str>,
        resource_alias: &str,
    ) -> String {
        let mut alias_counter = 0usize;
        build_chain_sql(
            &self.chain,
            &self.kind,
            bind_params,
            base_url,
            searched_resource_type,
            resource_alias,
            &mut alias_counter,
        )
    }
}

fn build_chain_sql(
    chain: &[FilterChainStep],
    kind: &FilterAtomKind,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    searched_resource_type: Option<&str>,
    current_alias: &str,
    alias_counter: &mut usize,
) -> String {
    if chain.is_empty() {
        return build_atom_sql(
            kind,
            bind_params,
            base_url,
            searched_resource_type,
            current_alias,
        );
    }

    let step = &chain[0];
    *alias_counter += 1;
    let sr_alias = format!("sr_f{}", alias_counter);
    let tgt_alias = format!("t_f{}", alias_counter);

    let param_name_idx = push_text(bind_params, step.reference_param.clone());
    let mut sql = format!(
        r#"EXISTS (
            SELECT 1
            FROM search_reference {sr}
            INNER JOIN resources {tgt}
                ON {tgt}.resource_type = {sr}.target_type
               AND {tgt}.id = {sr}.target_id
            WHERE {sr}.resource_type = {cur}.resource_type
              AND {sr}.resource_id = {cur}.id
              AND {sr}.version_id = {cur}.version_id
              AND {sr}.parameter_name = ${pidx}
              AND {tgt}.is_current = true
              AND {tgt}.deleted = false"#,
        sr = sr_alias,
        tgt = tgt_alias,
        cur = current_alias,
        pidx = param_name_idx
    );

    if !step.target_types.is_empty() {
        let types_idx = push_text_array(bind_params, step.target_types.clone());
        sql.push_str(&format!(
            " AND {tgt}.resource_type = ANY(${types_idx})",
            tgt = tgt_alias
        ));
    }

    if let Some(step_filter) = &step.filter {
        let next_type_hint = if step.target_types.len() == 1 {
            Some(step.target_types[0].as_str())
        } else {
            None
        };
        let filter_sql =
            step_filter.build_sql(bind_params, base_url, next_type_hint, tgt_alias.as_str());
        sql.push_str(&format!(" AND ({})", filter_sql));
    }

    let inner = build_chain_sql(
        &chain[1..],
        kind,
        bind_params,
        base_url,
        if step.target_types.len() == 1 {
            Some(step.target_types[0].as_str())
        } else {
            None
        },
        tgt_alias.as_str(),
        alias_counter,
    );
    sql.push_str(&format!(" AND ({inner})"));
    sql.push(')');
    sql
}

fn build_atom_sql(
    kind: &FilterAtomKind,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    searched_resource_type: Option<&str>,
    resource_alias: &str,
) -> String {
    match kind {
        FilterAtomKind::Standard(resolved) => claueses::build_param_clause_for_resource(
            resolved,
            bind_params,
            base_url,
            searched_resource_type,
            resource_alias,
        )
        .unwrap_or_else(|| "FALSE".to_string()),
        FilterAtomKind::StringEq { code, value } => {
            build_string_eq_clause(code, value, bind_params, resource_alias)
        }
        FilterAtomKind::StringEndsWith { code, value } => {
            build_string_ends_with_clause(code, value, bind_params, resource_alias)
        }
        FilterAtomKind::DateOverlaps { code, value } => {
            build_date_overlaps_clause(code, value, bind_params, resource_alias)
        }
        FilterAtomKind::LastUpdatedOverlaps { value } => {
            build_last_updated_overlaps_clause(value, bind_params, resource_alias)
        }
    }
}

fn build_string_eq_clause(
    code: &str,
    value: &str,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> String {
    let normalized = normalize_string_for_search(value);
    if normalized.is_empty() {
        return "FALSE".to_string();
    }

    let param_name_idx = push_text(bind_params, code.to_string());
    let norm_idx = push_text(bind_params, normalized);
    let raw_idx = push_text(bind_params, value.to_string());

    format!(
        "EXISTS (SELECT 1 FROM search_string sp WHERE sp.resource_type = {}.resource_type AND sp.resource_id = {}.id AND sp.version_id = {}.version_id AND sp.parameter_name = ${p} AND ((sp.value_normalized <> '' AND sp.value_normalized = ${n}) OR (sp.value_normalized = '' AND lower(sp.value) = lower(${r}))))",
        resource_alias,
        resource_alias,
        resource_alias,
        p = param_name_idx,
        n = norm_idx,
        r = raw_idx,
    )
}

fn build_string_ends_with_clause(
    code: &str,
    value: &str,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> String {
    let normalized = normalize_string_for_search(value);
    if normalized.is_empty() {
        return "FALSE".to_string();
    }

    let param_name_idx = push_text(bind_params, code.to_string());
    let norm_pat = format!("%{}", escape_like_pattern(&normalized));
    let raw_pat = format!("%{}", escape_like_pattern(value));
    let norm_idx = push_text(bind_params, norm_pat);
    let raw_idx = push_text(bind_params, raw_pat);

    format!(
        "EXISTS (SELECT 1 FROM search_string sp WHERE sp.resource_type = {}.resource_type AND sp.resource_id = {}.id AND sp.version_id = {}.version_id AND sp.parameter_name = ${p} AND ((sp.value_normalized <> '' AND sp.value_normalized LIKE ${n} ESCAPE E'\\\\') OR (sp.value_normalized = '' AND sp.value ILIKE ${r} ESCAPE E'\\\\')))",
        resource_alias,
        resource_alias,
        resource_alias,
        p = param_name_idx,
        n = norm_idx,
        r = raw_idx,
    )
}

fn escape_like_pattern(s: &str) -> String {
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

fn build_date_overlaps_clause(
    code: &str,
    value: &str,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> String {
    let Ok((start, end)) = claueses::fhir_date_range(value) else {
        return "FALSE".to_string();
    };

    let param_name_idx = push_text(bind_params, code.to_string());
    let start_idx = push_text(bind_params, start.to_rfc3339());
    let end_idx = push_text(bind_params, end.to_rfc3339());

    format!(
        "EXISTS (SELECT 1 FROM search_date sp WHERE sp.resource_type = {}.resource_type AND sp.resource_id = {}.id AND sp.version_id = {}.version_id AND sp.parameter_name = ${p} AND (sp.start_date < ${e}::timestamptz AND sp.end_date > ${s}::timestamptz))",
        resource_alias,
        resource_alias,
        resource_alias,
        p = param_name_idx,
        s = start_idx,
        e = end_idx,
    )
}

fn build_last_updated_overlaps_clause(
    value: &str,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> String {
    let Ok((start, end)) = claueses::fhir_date_range(value) else {
        return "FALSE".to_string();
    };

    let start_idx = push_text(bind_params, start.to_rfc3339());
    let end_idx = push_text(bind_params, end.to_rfc3339());

    format!(
        "({a}.last_updated >= ${s}::timestamptz AND {a}.last_updated < ${e}::timestamptz)",
        a = resource_alias,
        s = start_idx,
        e = end_idx
    )
}

fn build_has_sql(
    spec: &ReverseChainSpec,
    filter: &FilterExpr,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    resource_alias: &str,
) -> String {
    let referring_type_idx = push_text(bind_params, spec.referring_resource.clone());
    let param_name_idx = push_text(bind_params, spec.referring_param.clone());

    let filter_sql = filter.build_sql(
        bind_params,
        base_url,
        Some(spec.referring_resource.as_str()),
        "ref_r",
    );

    format!(
        r#"EXISTS (
            SELECT 1 FROM resources ref_r
            WHERE ref_r.is_current = true
              AND ref_r.deleted = false
              AND ref_r.resource_type = ${ref_typ}
              AND ({filter_sql})
              AND EXISTS (
                SELECT 1 FROM search_reference sr
                WHERE sr.resource_type = ref_r.resource_type
                  AND sr.resource_id = ref_r.id
                  AND sr.version_id = ref_r.version_id
                  AND sr.parameter_name = ${pname}
                  AND sr.target_type = {outer}.resource_type
                  AND sr.target_id = {outer}.id
              )
        )"#,
        ref_typ = referring_type_idx,
        pname = param_name_idx,
        outer = resource_alias,
    )
}

#[cfg(test)]
mod tests {
    use super::super::SearchValue;
    use super::*;
    use crate::db::search::parameter_lookup::SearchParamType;

    #[test]
    fn builds_simple_string_eq_sql() {
        let expr = FilterExpr::Atom(FilterAtom {
            chain: Vec::new(),
            kind: FilterAtomKind::StringEq {
                code: "name".to_string(),
                value: "Peter".to_string(),
            },
        });

        let mut binds = Vec::new();
        let sql = expr.build_sql(&mut binds, None, Some("Patient"), "r");
        assert!(sql.contains("FROM search_string sp"));
        assert!(sql.contains("sp.parameter_name"));
    }

    #[test]
    fn builds_chained_filter_sql() {
        let expr = FilterExpr::Atom(FilterAtom {
            chain: vec![FilterChainStep {
                reference_param: "patient".to_string(),
                target_types: vec!["Patient".to_string()],
                filter: None,
            }],
            kind: FilterAtomKind::StringEq {
                code: "name".to_string(),
                value: "Peter".to_string(),
            },
        });

        let mut binds = Vec::new();
        let sql = expr.build_sql(&mut binds, None, Some("Observation"), "r");
        assert!(sql.contains("FROM search_reference sr_f1"));
        assert!(sql.contains("INNER JOIN resources t_f1"));
        assert!(sql.contains("sp.resource_type = t_f1.resource_type"));
    }

    #[test]
    fn builds_element_scoped_filter_sql() {
        let gender = FilterExpr::Atom(FilterAtom {
            chain: Vec::new(),
            kind: FilterAtomKind::Standard(ResolvedParam {
                raw_name: "gender".to_string(),
                code: "gender".to_string(),
                param_type: SearchParamType::Token,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "female".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            }),
        });

        let expr = FilterExpr::Atom(FilterAtom {
            chain: vec![FilterChainStep {
                reference_param: "patient".to_string(),
                target_types: vec!["Patient".to_string()],
                filter: Some(Box::new(gender)),
            }],
            kind: FilterAtomKind::StringEq {
                code: "name".to_string(),
                value: "Peter".to_string(),
            },
        });

        let mut binds = Vec::new();
        let sql = expr.build_sql(&mut binds, None, Some("Observation"), "r");
        assert!(sql.contains("FROM search_reference sr_f1"));
        assert!(sql.contains("FROM search_token sp"));
        assert!(sql.contains("FROM search_string sp"));
    }

    #[test]
    fn builds_has_filter_sql() {
        let spec = ReverseChainSpec {
            referring_resource: "Observation".to_string(),
            referring_param: "patient".to_string(),
            filter_param: "code".to_string(),
        };

        let filter = FilterExpr::Atom(FilterAtom {
            chain: Vec::new(),
            kind: FilterAtomKind::Standard(ResolvedParam {
                raw_name: "code".to_string(),
                code: "code".to_string(),
                param_type: SearchParamType::Token,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "http://loinc.org|1234-5".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            }),
        });

        let expr = FilterExpr::Has {
            spec,
            filter: Box::new(filter),
        };

        let mut binds = Vec::new();
        let sql = expr.build_sql(&mut binds, None, Some("Patient"), "r");
        assert!(sql.contains("FROM resources ref_r"));
        assert!(sql.contains("FROM search_reference sr"));
        assert!(sql.contains("sr.target_id = r.id"));
    }
}
