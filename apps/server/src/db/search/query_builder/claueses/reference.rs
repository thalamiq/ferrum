use crate::db::search::escape::unescape_search_value;

use super::super::bind::push_text;
use super::super::{hierarchy_parent_param_for_type, BindValue, ResolvedParam, SearchModifier};
use super::token::{
    exact_ci_match, is_case_sensitive_token_system, parse_token_value, TokenSearchValue,
};

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

#[derive(Debug, Clone, Copy)]
enum ReferenceHierarchyDirection {
    Above,
    Below,
}

#[derive(Debug, Clone)]
pub(crate) enum ParsedReferenceQuery {
    Canonical {
        url: String,
        version: String,
    },
    Absolute {
        url: String,
        is_local: bool,
        typ: Option<String>,
        id: Option<String>,
        version: Option<String>,
    },
    Relative {
        typ: Option<String>,
        id: String,
        version: Option<String>,
    },
    Fragment {
        id: String,
    },
}

pub(crate) fn parse_reference_query_value(
    raw: &str,
    base_url: Option<&str>,
) -> Option<ParsedReferenceQuery> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(fragment) = raw.strip_prefix('#') {
        return Some(ParsedReferenceQuery::Fragment {
            id: fragment.to_string(),
        });
    }

    if let Some((left, right)) = raw.split_once('|') {
        let left = normalize_url_like(left);
        if !left.is_empty() && (looks_like_absolute_url(&left) || left.starts_with("urn:")) {
            return Some(ParsedReferenceQuery::Canonical {
                url: left,
                version: right.trim().to_string(),
            });
        }
    }

    let normalized = normalize_url_like(raw);

    if looks_like_absolute_url(&normalized) || normalized.starts_with("urn:") {
        let segs = normalized
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let (typ, id, version) = parse_reference_type_id_version_from_segs(&segs)
            .map(|(t, i, v)| (Some(t), Some(i), v))
            .unwrap_or((None, None, None));
        let is_local = base_url
            .map(normalize_url_like)
            .map(|b| normalized.starts_with(&(b + "/")))
            .unwrap_or(false);
        return Some(ParsedReferenceQuery::Absolute {
            url: normalized,
            is_local,
            typ,
            id,
            version,
        });
    }

    let segs = normalized
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if segs.is_empty() {
        return None;
    }

    if segs.len() == 1 {
        return Some(ParsedReferenceQuery::Relative {
            typ: None,
            id: segs[0].to_string(),
            version: None,
        });
    }

    let (typ, id, version) = parse_reference_type_id_version_from_segs(&segs)?;
    Some(ParsedReferenceQuery::Relative {
        typ: Some(typ),
        id,
        version,
    })
}

pub(in crate::db::search::query_builder) fn build_reference_identifier_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    resource_alias: &str,
) -> Option<String> {
    // Token semantics applied to Reference.identifier values indexed under the same parameter name.
    let mut parts = Vec::new();
    let param_name_idx = push_text(bind_params, resolved.code.clone());
    for v in &resolved.values {
        let ts = parse_token_value(v.raw.as_str());
        let clause = match ts {
            TokenSearchValue::AnySystemCode(code) => {
                exact_ci_match("st", "code", "code_ci", &code, bind_params, false)
            }
            TokenSearchValue::NoSystemCode(code) => {
                format!(
                    "(st.system IS NULL AND {})",
                    exact_ci_match("st", "code", "code_ci", &code, bind_params, false)
                )
            }
            TokenSearchValue::SystemOnly(system) => {
                let s_idx = push_text(bind_params, system);
                format!("(st.system = ${})", s_idx)
            }
            TokenSearchValue::SystemCode { system, code } => {
                let case_sensitive = is_case_sensitive_token_system(system.as_str());
                let s_idx = push_text(bind_params, system);
                format!(
                    "(st.system = ${} AND {})",
                    s_idx,
                    exact_ci_match("st", "code", "code_ci", &code, bind_params, case_sensitive)
                )
            }
        };
        parts.push(clause);
    }

    if parts.is_empty() {
        return None;
    }

    Some(format!(
        "EXISTS (SELECT 1 FROM search_token st WHERE st.resource_type = {}.resource_type AND st.resource_id = {}.id AND st.version_id = {}.version_id AND st.parameter_name = ${} AND ({}))",
        resource_alias,
        resource_alias,
        resource_alias,
        param_name_idx,
        parts.join(" OR ")
    ))
}

pub(in crate::db::search::query_builder) fn build_reference_json_clause(
    idx: usize,
    raw_value: &str,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
) -> Option<String> {
    let raw = unescape_search_value(raw_value).ok()?;
    #[allow(clippy::question_mark)]
    let Some(parsed) = parse_reference_query_value(raw.as_str(), base_url) else {
        return None;
    };

    match parsed {
        ParsedReferenceQuery::Canonical { url, version } => {
            let url_idx = push_text(bind_params, url);
            let mut clause = format!("(sc.components->{}->>'canonical_url' = ${})", idx, url_idx);
            if let Some(vcl) = version_prefix_match_clause(
                &format!("sc.components->{}->>'canonical_version'", idx),
                &version,
                bind_params,
            ) {
                clause.push_str(" AND ");
                clause.push_str(&vcl);
            }
            Some(format!("({})", clause))
        }
        ParsedReferenceQuery::Relative { typ, id, version } => {
            let id_idx = push_text(bind_params, id);
            if let Some(t) = typ {
                let t_idx = push_text(bind_params, t);
                let mut clause = format!(
                    "(sc.components->{}->>'target_type' = ${} AND sc.components->{}->>'target_id' = ${})",
                    idx, t_idx, idx, id_idx
                );
                if let Some(v) = version {
                    let v_idx = push_text(bind_params, v);
                    clause.push_str(&format!(
                        " AND sc.components->{}->>'target_version_id' = ${}",
                        idx, v_idx
                    ));
                }
                Some(format!("({})", clause))
            } else {
                Some(format!(
                    "(sc.components->{}->>'target_id' = ${})",
                    idx, id_idx
                ))
            }
        }
        ParsedReferenceQuery::Absolute { url, .. } => {
            let url_idx = push_text(bind_params, url);
            Some(format!(
                "(sc.components->{}->>'target_url' = ${} OR sc.components->{}->>'canonical_url' = ${})",
                idx, url_idx, idx, url_idx
            ))
        }
        ParsedReferenceQuery::Fragment { id } => {
            let id_idx = push_text(bind_params, id);
            Some(format!(
                "(sc.components->{}->>'target_id' = ${})",
                idx, id_idx
            ))
        }
    }
}

fn looks_like_absolute_url(s: &str) -> bool {
    s.contains("://")
}
fn parse_reference_type_id_version_from_segs(
    segs: &[&str],
) -> Option<(String, String, Option<String>)> {
    if segs.len() >= 4 && segs[segs.len() - 2] == "_history" {
        let typ = segs[segs.len() - 4];
        let id = segs[segs.len() - 3];
        let vid = segs[segs.len() - 1];
        return Some((typ.to_string(), id.to_string(), Some(vid.to_string())));
    }
    if segs.len() >= 2 {
        let typ = segs[segs.len() - 2];
        let id = segs[segs.len() - 1];
        return Some((typ.to_string(), id.to_string(), None));
    }
    None
}

fn version_prefix_match_clause(
    column: &str,
    version: &str,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let v = version.trim();
    if v.is_empty() {
        return None;
    }
    let v_idx = push_text(bind_params, v.to_string());
    Some(format!(
        "({col} = ${v} OR {col} LIKE ${v} || '.%' OR {col} LIKE ${v} || '-%')",
        col = column,
        v = v_idx
    ))
}

fn local_reference_predicate(base_url: Option<&str>, bind_params: &mut Vec<BindValue>) -> String {
    local_reference_predicate_for_alias("sp", base_url, bind_params)
}

fn local_reference_predicate_for_alias(
    alias: &str,
    base_url: Option<&str>,
    bind_params: &mut Vec<BindValue>,
) -> String {
    let Some(b) = base_url else {
        // Without a base URL, we can't safely treat absolute references as local.
        return format!("({}.reference_kind = 'relative')", alias);
    };
    let pattern = format!("{}/%", normalize_url_like(b));
    let idx = push_text(bind_params, pattern);
    format!(
        "({a}.reference_kind = 'relative' OR ({a}.reference_kind = 'absolute' AND {a}.target_url LIKE ${idx}))",
        a = alias,
        idx = idx
    )
}
pub(in crate::db::search::query_builder) fn build_reference_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
) -> Option<String> {
    match &resolved.modifier {
        // Handle [type] modifier - filter by resource type
        Some(SearchModifier::TypeModifier(resource_type)) => {
            let mut parts = Vec::new();
            let type_idx = push_text(bind_params, resource_type.clone());
            let local_pred = local_reference_predicate(base_url, bind_params);

            for v in &resolved.values {
                let id = v.raw.trim();
                if id.is_empty() {
                    continue;
                };

                let id_idx = push_text(bind_params, id.to_string());
                parts.push(format!(
                    "(sp.target_type = ${} AND sp.target_id = ${} AND {})",
                    type_idx, id_idx, local_pred
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

        // :above and :below for hierarchical references.
        //
        // Per FHIR spec (3.2.1.5.5.1.1), this traverses known circular reference hierarchies
        // on the referenced resource type (e.g., Location.partOf / Organization.partOf),
        // matching the exact resource plus all ancestors (:above) or descendants (:below).
        //
        // Additionally, for canonical targets (FHIR type `canonical` stored as
        // `reference_kind='canonical'`), :above/:below perform version comparisons
        // (3.2.1.5.5.1.2 / 3.2.1.5.5.2.2).
        Some(SearchModifier::Above) => build_reference_hierarchy_clause(
            ReferenceHierarchyDirection::Above,
            resolved,
            bind_params,
            base_url,
        ),

        Some(SearchModifier::Below) => build_reference_hierarchy_clause(
            ReferenceHierarchyDirection::Below,
            resolved,
            bind_params,
            base_url,
        ),

        // :code-text searches the "code" parts of the reference (identifier fields)
        // Per FHIR spec 3.2.1.5.5.3: searches reference identifiers (type/id, canonical URL)
        // Case-insensitive and combining-character insensitive (ILIKE provides case-insensitivity)
        Some(SearchModifier::CodeText) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
                if raw_unescaped.trim().is_empty() {
                    continue;
                }
                let pattern = format!("{}%", escape_like_pattern(&raw_unescaped));
                let idx = push_text(bind_params, pattern);
                // Search reference identifier fields: target_type, target_id, canonical_url
                // These are the "code" parts of a reference (the structured identifier)
                parts.push(format!(
                    "(sp.target_type ILIKE ${0} ESCAPE E'\\\\' OR sp.target_id ILIKE ${0} ESCAPE E'\\\\' OR sp.canonical_url ILIKE ${0} ESCAPE E'\\\\')",
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

        // :text searches the display text (human-readable description)
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

        Some(SearchModifier::TextAdvanced) => {
            // Advanced text search on display field
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

        // Default reference search behavior (no modifier)
        None | Some(_) => {
            let mut parts = Vec::new();
            let local_pred = local_reference_predicate(base_url, bind_params);
            for v in &resolved.values {
                let raw = v.raw.as_str();
                let Some(parsed) = parse_reference_query_value(raw, base_url) else {
                    continue;
                };
                match parsed {
                    ParsedReferenceQuery::Canonical { url, version } => {
                        let url_idx = push_text(bind_params, url);
                        let mut clause = format!(
                            "(sp.reference_kind = 'canonical' AND sp.canonical_url = ${})",
                            url_idx
                        );
                        if let Some(vcl) = version_prefix_match_clause(
                            "sp.canonical_version",
                            &version,
                            bind_params,
                        ) {
                            clause.push_str(" AND ");
                            clause.push_str(&vcl);
                        }
                        parts.push(clause);
                    }
                    ParsedReferenceQuery::Absolute {
                        url,
                        is_local,
                        typ,
                        id,
                        version,
                    } => {
                        if !is_local {
                            let url_idx = push_text(bind_params, url);
                            parts.push(format!(
                                "((sp.reference_kind = 'absolute' AND sp.target_url = ${0}) OR (sp.reference_kind = 'canonical' AND sp.canonical_url = ${0}))",
                                url_idx
                            ));
                            continue;
                        }

                        // Local absolute URL: match local references (absolute or relative).
                        let Some(t) = typ else { continue };
                        let Some(i) = id else { continue };
                        let t_idx = push_text(bind_params, t);
                        let i_idx = push_text(bind_params, i);
                        let mut clause = format!(
                            "(sp.target_type = ${} AND sp.target_id = ${})",
                            t_idx, i_idx
                        );
                        if let Some(v) = version {
                            let v_idx = push_text(bind_params, v);
                            clause.push_str(&format!(" AND sp.target_version_id = ${}", v_idx));
                        } else {
                            // Absolute non-versioned searches do not match versioned references.
                            clause.push_str(" AND sp.target_version_id = ''");
                        }
                        clause.push_str(&format!(" AND {}", local_pred));
                        parts.push(format!("({})", clause));
                    }
                    ParsedReferenceQuery::Relative { typ, id, version } => {
                        if let Some(t) = typ {
                            let t_idx = push_text(bind_params, t);
                            let i_idx = push_text(bind_params, id);
                            let mut clause = format!(
                                "(sp.target_type = ${} AND sp.target_id = ${})",
                                t_idx, i_idx
                            );
                            if let Some(v) = version {
                                let v_idx = push_text(bind_params, v);
                                clause.push_str(&format!(" AND sp.target_version_id = ${}", v_idx));
                            }
                            // Relative non-versioned searches SHOULD match versioned references.
                            clause.push_str(&format!(" AND {}", local_pred));
                            parts.push(format!("({})", clause));
                        } else {
                            // ID-only: may match multiple resource types.
                            let i_idx = push_text(bind_params, id);
                            parts.push(format!(
                                "((sp.target_id = ${} AND {}) OR (sp.reference_kind = 'fragment' AND sp.target_id = ${}))",
                                i_idx, local_pred, i_idx
                            ));
                        }
                    }
                    ParsedReferenceQuery::Fragment { id } => {
                        let id_idx = push_text(bind_params, id);
                        parts.push(format!(
                            "(sp.reference_kind = 'fragment' AND sp.target_id = ${})",
                            id_idx
                        ));
                    }
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
    }
}

pub(in crate::db::search::query_builder) fn build_reference_contains_hierarchy_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
    searched_resource_type: Option<&str>,
    resource_alias: &str,
) -> Option<String> {
    let searched_type_hint = searched_resource_type.map(|s| s.to_string());
    let mut parts = Vec::new();

    for v in &resolved.values {
        let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
        let Some(parsed) = parse_reference_query_value(raw_unescaped.as_str(), base_url) else {
            continue;
        };

        let (typ_opt, id_opt) = match parsed {
            ParsedReferenceQuery::Relative { typ, id, .. } => (typ, Some(id)),
            ParsedReferenceQuery::Absolute {
                is_local, typ, id, ..
            } => {
                if !is_local {
                    continue;
                }
                (typ, id)
            }
            ParsedReferenceQuery::Canonical { .. } | ParsedReferenceQuery::Fragment { .. } => {
                continue;
            }
        };

        let id = match id_opt {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };

        let typ = match (typ_opt, &searched_type_hint) {
            (Some(t), Some(hint)) => {
                if t != *hint {
                    continue;
                }
                t
            }
            (Some(t), None) => t,
            (None, Some(hint)) => hint.clone(),
            (None, None) => continue,
        };

        let Some(expected_parent_param) = hierarchy_parent_param_for_type(&typ) else {
            continue;
        };
        if !resolved.code.eq_ignore_ascii_case(expected_parent_param) {
            continue;
        }

        let typ_idx = push_text(bind_params, typ.clone());
        let id_idx = push_text(bind_params, id);
        let parent_param_idx = push_text(bind_params, resolved.code.clone());
        let local_pred_sr = local_reference_predicate_for_alias("sr", base_url, bind_params);

        // The closure is: the specified resource, its ancestors, and its descendants.
        // Importantly, this is not the entire connected component: descendants are
        // computed from the specified resource only (no siblings via ancestors).
        let closure_subquery = format!(
            "(
                WITH RECURSIVE
                above(id) AS (
                    SELECT ${id}::text
                    UNION
                    SELECT sr.target_id
                    FROM search_reference sr
                    INNER JOIN resources rr
                        ON rr.resource_type = sr.resource_type
                        AND rr.id = sr.resource_id
                        AND rr.version_id = sr.version_id
                        AND rr.is_current = true
                        AND rr.deleted = false
                    INNER JOIN above a ON a.id = sr.resource_id
                    WHERE sr.resource_type = ${typ}
                        AND sr.parameter_name = ${parent_param}
                        AND (sr.target_type = ${typ} OR sr.target_type = '')
                        AND sr.target_id <> ''
                        AND {local_pred_sr}
                ),
                below(id) AS (
                    SELECT ${id}::text
                    UNION
                    SELECT sr.resource_id
                    FROM search_reference sr
                    INNER JOIN resources rr
                        ON rr.resource_type = sr.resource_type
                        AND rr.id = sr.resource_id
                        AND rr.version_id = sr.version_id
                        AND rr.is_current = true
                        AND rr.deleted = false
                    INNER JOIN below b ON b.id = sr.target_id
                    WHERE sr.resource_type = ${typ}
                        AND sr.parameter_name = ${parent_param}
                        AND (sr.target_type = ${typ} OR sr.target_type = '')
                        AND sr.target_id <> ''
                        AND {local_pred_sr}
                )
                SELECT id FROM above
                UNION
                SELECT id FROM below
            )",
            id = id_idx,
            typ = typ_idx,
            parent_param = parent_param_idx,
            local_pred_sr = local_pred_sr,
        );

        parts.push(format!(
            "({a}.resource_type = ${typ} AND {a}.id IN {closure})",
            typ = typ_idx,
            closure = closure_subquery,
            a = resource_alias
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

fn build_reference_hierarchy_clause(
    direction: ReferenceHierarchyDirection,
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
) -> Option<String> {
    let canonical = build_reference_canonical_version_clause(direction, resolved, bind_params);
    let hierarchy =
        build_reference_resource_hierarchy_clause(direction, resolved, bind_params, base_url);
    match (canonical, hierarchy) {
        (Some(c), Some(h)) => Some(format!("({} OR {})", c, h)),
        (Some(c), None) => Some(c),
        (None, Some(h)) => Some(h),
        (None, None) => None,
    }
}

fn build_reference_canonical_version_clause(
    direction: ReferenceHierarchyDirection,
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let Some(ParsedReferenceQuery::Canonical { url, version }) =
            parse_reference_query_value(&v.raw, None)
        else {
            continue;
        };

        let url = normalize_url_like(&url);
        let version = version.trim();
        if url.is_empty() || version.is_empty() {
            continue;
        }

        let url_idx = push_text(bind_params, url);
        let version_idx = push_text(bind_params, version.to_string());

        // Implementation choice: we only support numeric dot-separated versions here.
        // Input validation in the search engine ensures the query version is numeric.
        // We also guard the stored value with a regex before casting.
        let cmp = match direction {
            ReferenceHierarchyDirection::Above => ">",
            ReferenceHierarchyDirection::Below => "<",
        };

        // Compare versions as padded int arrays so `1.2` == `1.2.0` under this scheme.
        let pad_len = 6;
        let sp_arr = format!(
            "(string_to_array(sp.canonical_version, '.')::int[] || array_fill(0, ARRAY[GREATEST(0, {pad_len} - array_length(string_to_array(sp.canonical_version, '.'), 1))]))",
            pad_len = pad_len
        );
        let q_arr = format!(
            "(string_to_array(${ver}, '.')::int[] || array_fill(0, ARRAY[GREATEST(0, {pad_len} - array_length(string_to_array(${ver}, '.'), 1))]))",
            ver = version_idx,
            pad_len = pad_len
        );

        parts.push(format!(
            "(sp.reference_kind = 'canonical' AND sp.canonical_url = ${url} AND sp.canonical_version <> '' AND sp.canonical_version ~ '^[0-9]+(\\\\.[0-9]+)*$' AND {sp_arr} {cmp} {q_arr})",
            url = url_idx,
            sp_arr = sp_arr,
            cmp = cmp,
            q_arr = q_arr
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

fn build_reference_resource_hierarchy_clause(
    direction: ReferenceHierarchyDirection,
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
    base_url: Option<&str>,
) -> Option<String> {
    let local_pred_sp = local_reference_predicate_for_alias("sp", base_url, bind_params);
    let local_pred_sr = local_reference_predicate_for_alias("sr", base_url, bind_params);

    let mut parts = Vec::new();

    for v in &resolved.values {
        let Some(parsed) = parse_reference_query_value(&v.raw, base_url) else {
            continue;
        };

        let (typ, id) = match parsed {
            ParsedReferenceQuery::Relative { typ, id, .. } => (typ?, id),
            ParsedReferenceQuery::Absolute {
                is_local, typ, id, ..
            } => {
                if !is_local {
                    continue;
                }
                (typ?, id?)
            }
            ParsedReferenceQuery::Canonical { .. } | ParsedReferenceQuery::Fragment { .. } => {
                continue;
            }
        };

        let Some(parent_param) = hierarchy_parent_param_for_type(&typ) else {
            continue;
        };

        let type_idx = push_text(bind_params, typ.clone());
        let id_idx = push_text(bind_params, id);
        let parent_param_idx = push_text(bind_params, parent_param.to_string());

        let (recursive_select, recursive_join) = match direction {
            ReferenceHierarchyDirection::Above => ("sr.target_id", "h.id = sr.resource_id"),
            ReferenceHierarchyDirection::Below => ("sr.resource_id", "h.id = sr.target_id"),
        };

        let hierarchy_subquery = format!(
            "(
                WITH RECURSIVE hier(id) AS (
                    SELECT ${id}::text
                    UNION
                    SELECT {recursive_select}
                    FROM search_reference sr
                    INNER JOIN resources rr
                        ON rr.resource_type = sr.resource_type
                        AND rr.id = sr.resource_id
                        AND rr.version_id = sr.version_id
                        AND rr.is_current = true
                        AND rr.deleted = false
                    INNER JOIN hier h ON {recursive_join}
                    WHERE sr.resource_type = ${typ}
                        AND sr.parameter_name = ${parent_param}
                        AND (sr.target_type = ${typ} OR sr.target_type = '')
                        AND {local_pred_sr}
                )
                SELECT id FROM hier
            )",
            id = id_idx,
            typ = type_idx,
            parent_param = parent_param_idx,
            recursive_select = recursive_select,
            recursive_join = recursive_join,
            local_pred_sr = local_pred_sr,
        );

        parts.push(format!(
            "((sp.target_type = ${typ} OR sp.target_type = '') AND {local_pred_sp} AND sp.target_id IN {hierarchy_subquery})",
            typ = type_idx,
            local_pred_sp = local_pred_sp,
            hierarchy_subquery = hierarchy_subquery
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

fn normalize_url_like(s: &str) -> String {
    s.trim().trim_end_matches('/').to_string()
}
