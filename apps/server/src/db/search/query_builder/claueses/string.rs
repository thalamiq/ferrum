use crate::db::search::escape::unescape_search_value;
use crate::db::search::string_normalization::normalize_string_for_search;

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam, SearchModifier};
use super::fulltext_query::compile_fhir_text_query;

pub(in crate::db::search::query_builder) fn build_string_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    match &resolved.modifier {
        Some(SearchModifier::Exact) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                if v.raw.is_empty() {
                    continue;
                }
                let idx = push_text(bind_params, v.raw.clone());
                parts.push(format!("sp.value = ${}", idx));
            }

            if parts.is_empty() {
                None
            } else if parts.len() == 1 {
                Some(parts.remove(0))
            } else {
                Some(format!("({})", parts.join(" OR ")))
            }
        }

        Some(SearchModifier::Contains) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
                let normalized = normalize_string_for_search(&raw_unescaped);
                if normalized.is_empty() {
                    continue;
                }
                let norm_idx = push_text(bind_params, format!("%{}%", normalized));
                let raw_idx = push_text(
                    bind_params,
                    format!("%{}%", escape_like_pattern(&raw_unescaped)),
                );
                parts.push(format!(
                    "((sp.value_normalized <> '' AND sp.value_normalized LIKE ${}) OR (sp.value_normalized = '' AND sp.value ILIKE ${} ESCAPE E'\\\\'))",
                    norm_idx, raw_idx
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

        // :text for string type - advanced text handling using full-text search
        Some(SearchModifier::Text) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                if v.raw.is_empty() {
                    continue;
                }
                let idx = push_text(bind_params, v.raw.clone());
                parts.push(format!(
                    "to_tsvector('simple', sp.value) @@ websearch_to_tsquery('simple', ${})",
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

        // Default string search (starts-with, case-insensitive)
        None | Some(_) => {
            let mut parts = Vec::new();
            for v in &resolved.values {
                let normalized = normalize_string_for_search(&v.raw);
                if normalized.is_empty() {
                    continue;
                }
                let norm_idx = push_text(bind_params, format!("{}%", normalized));
                let raw_idx = push_text(bind_params, format!("{}%", v.raw));
                parts.push(format!(
                    "((sp.value_normalized <> '' AND sp.value_normalized LIKE ${}) OR (sp.value_normalized = '' AND sp.value ILIKE ${}))",
                    norm_idx, raw_idx
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
    }
}

pub(in crate::db::search::query_builder) fn build_string_json_clause(
    idx: usize,
    raw_value: &str,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let v = unescape_search_value(raw_value).ok()?;
    let norm = normalize_string_for_search(&v);
    if norm.is_empty() {
        return None;
    }
    let norm_idx = push_text(bind_params, format!("{}%", norm));
    Some(format!(
        "sc.components->{}->>'value_normalized' LIKE ${}",
        idx, norm_idx
    ))
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

pub(in crate::db::search::query_builder) fn build_fulltext_clause(
    content_expr: &str,
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    if resolved.values.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    for v in &resolved.values {
        if v.raw.is_empty() {
            continue;
        }

        let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());

        // :exact means phrase search for the whole input.
        if matches!(resolved.modifier, Some(SearchModifier::Exact)) {
            let idx = push_text(bind_params, raw_unescaped);
            parts.push(format!(
                "to_tsvector('simple', {}) @@ phraseto_tsquery('simple', ${})",
                content_expr, idx
            ));
            continue;
        }

        // FHIR spec for `_text` / `_content` uses a free-form search syntax that commonly includes
        // boolean operators and parentheses (e.g. "(bone OR liver) AND metastases").
        //
        // `websearch_to_tsquery` supports OR but does not treat `AND` as an operator and ignores
        // parentheses. We parse/compile those here into a safe `tsquery` expression composed from
        // `plainto_tsquery` / `phraseto_tsquery`.
        if let Some(tsquery_sql) = compile_fhir_text_query(&raw_unescaped, bind_params) {
            parts.push(format!(
                "to_tsvector('simple', {}) @@ ({})",
                content_expr, tsquery_sql
            ));
        } else {
            // Fallback for malformed expressions (unbalanced quotes/parentheses, etc.)
            let idx = push_text(bind_params, raw_unescaped);
            parts.push(format!(
                "to_tsvector('simple', {}) @@ websearch_to_tsquery('simple', ${})",
                content_expr, idx
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
