use crate::db::search::escape::unescape_search_value;
use crate::db::search::string_normalization::normalize_casefold_strip_combining;

use super::super::bind::push_text;
use super::super::{BindValue, ResolvedParam, SearchModifier};

pub(in crate::db::search::query_builder) fn build_uri_clause(
    resolved: &ResolvedParam,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut parts = Vec::new();
    for v in &resolved.values {
        let clause = match resolved.modifier {
            Some(SearchModifier::Contains) => {
                let raw_unescaped = unescape_search_value(&v.raw).unwrap_or_else(|_| v.raw.clone());
                let normalized = normalize_casefold_strip_combining(&raw_unescaped);
                if normalized.is_empty() {
                    continue;
                }

                let norm_idx = push_text(
                    bind_params,
                    format!("%{}%", escape_like_pattern(&normalized)),
                );
                let raw_idx = push_text(
                    bind_params,
                    format!("%{}%", escape_like_pattern(&raw_unescaped)),
                );
                format!(
                    "((sp.value_normalized <> '' AND sp.value_normalized LIKE ${0} ESCAPE E'\\\\') OR (sp.value_normalized = '' AND sp.value ILIKE ${1} ESCAPE E'\\\\'))",
                    norm_idx, raw_idx
                )
            }
            Some(SearchModifier::Below) => {
                // Segment-based descendant matching (URLs only).
                let norm = normalize_url_like(&v.raw);
                let idx = push_text(bind_params, norm);
                format!(
                    "(rtrim(sp.value, '/') = ${0} OR rtrim(sp.value, '/') LIKE ${0} || '/%')",
                    idx
                )
            }
            Some(SearchModifier::Above) => {
                // Segment-based ancestor matching (URLs only).
                let norm = normalize_url_like(&v.raw);
                let idx = push_text(bind_params, norm);
                format!(
                    "(rtrim(sp.value, '/') = ${0} OR ${0} LIKE rtrim(sp.value, '/') || '/%')",
                    idx
                )
            }
            _ => {
                let idx = push_text(bind_params, v.raw.clone());
                format!("sp.value = ${}", idx)
            }
        };
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

fn normalize_url_like(s: &str) -> String {
    s.trim().trim_end_matches('/').to_string()
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
