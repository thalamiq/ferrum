use super::util::is_untyped_logical_id_reference;
use super::{query_builder, SearchEngine};
use crate::db::search::parameter_lookup::SearchParamType;
use crate::Result;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::Value as JsonValue;
use sqlx::PgConnection;
use std::collections::HashSet;

impl SearchEngine {
    pub(super) async fn normalize_search_params(
        &self,
        conn: &mut PgConnection,
        resolved: &mut [query_builder::ResolvedParam],
        base_url: Option<&str>,
        searched_type_hint: Option<&str>,
    ) -> Result<()> {
        let base_url = base_url.map(|b| b.trim_end_matches('/').to_string());

        for p in resolved.iter_mut() {
            match p.param_type {
                SearchParamType::Token => {
                    if matches!(
                        p.modifier,
                        Some(
                            query_builder::SearchModifier::In
                                | query_builder::SearchModifier::NotIn
                        )
                    ) {
                        let is_not_in =
                            matches!(p.modifier, Some(query_builder::SearchModifier::NotIn));
                        let mut expanded = Vec::new();

                        for v in &p.values {
                            let vs_ref = crate::db::search::escape::unescape_search_value(&v.raw)
                                .unwrap_or_else(|_| v.raw.clone());
                            let codes = self
                                .expand_valueset_to_token_codes(conn, vs_ref.as_str())
                                .await?;
                            expanded.extend(codes);
                        }

                        if expanded.is_empty() {
                            return Err(crate::Error::Validation(format!(
                                "ValueSet expansion for '{}:{}' produced no codes",
                                p.raw_name,
                                p.values.first().map(|v| v.raw.as_str()).unwrap_or("")
                            )));
                        }

                        expanded.sort();
                        expanded.dedup();

                        p.values = expanded
                            .into_iter()
                            .map(|raw| query_builder::SearchValue { raw, prefix: None })
                            .collect();

                        p.modifier = if is_not_in {
                            Some(query_builder::SearchModifier::Not)
                        } else {
                            None
                        };
                    }
                }
                SearchParamType::Special => {
                    // Membership parameters rely on collection resources and need special normalization.
                    if p.code == "_in" {
                        let base = base_url.as_deref();
                        for v in &mut p.values {
                            let raw = v.raw.trim();
                            if raw.is_empty() {
                                continue;
                            }

                            // Normalize local absolute references to `Type/id`.
                            if raw.contains("://") {
                                let Some(parsed) =
                                    query_builder::parse_reference_query_value(raw, base)
                                else {
                                    return Err(crate::Error::Validation(format!(
                                        "Invalid _in reference: {}",
                                        v.raw
                                    )));
                                };

                                match parsed {
                                    query_builder::ParsedReferenceQuery::Absolute {
                                        is_local,
                                        typ,
                                        id,
                                        version,
                                        ..
                                    } => {
                                        if !is_local {
                                            return Err(crate::Error::Validation(format!(
                                                "_in does not support non-local absolute references: {}",
                                                v.raw
                                            )));
                                        }
                                        if version.is_some() {
                                            return Err(crate::Error::Validation(
                                                "_in does not support versioned references"
                                                    .to_string(),
                                            ));
                                        }
                                        let Some(typ) = typ else {
                                            return Err(crate::Error::Validation(format!(
                                                "_in reference must include a type: {}",
                                                v.raw
                                            )));
                                        };
                                        let Some(id) = id else {
                                            return Err(crate::Error::Validation(format!(
                                                "_in reference must include an id: {}",
                                                v.raw
                                            )));
                                        };
                                        if !matches!(typ.as_str(), "CareTeam" | "Group" | "List") {
                                            return Err(crate::Error::Validation(format!(
                                                "_in target must be CareTeam, Group, or List: {}",
                                                v.raw
                                            )));
                                        }
                                        v.raw = format!("{}/{}", typ, id);
                                    }
                                    query_builder::ParsedReferenceQuery::Relative {
                                        typ,
                                        id,
                                        version,
                                    } => {
                                        if version.is_some() {
                                            return Err(crate::Error::Validation(
                                                "_in does not support versioned references"
                                                    .to_string(),
                                            ));
                                        }
                                        if let Some(typ) = typ {
                                            if !matches!(
                                                typ.as_str(),
                                                "CareTeam" | "Group" | "List"
                                            ) {
                                                return Err(crate::Error::Validation(format!(
                                                    "_in target must be CareTeam, Group, or List: {}",
                                                    v.raw
                                                )));
                                            }
                                            v.raw = format!("{}/{}", typ, id);
                                        } else {
                                            // id-only form (applies across CareTeam/Group/List).
                                            v.raw = id;
                                        }
                                    }
                                    query_builder::ParsedReferenceQuery::Canonical { .. }
                                    | query_builder::ParsedReferenceQuery::Fragment { .. } => {
                                        return Err(crate::Error::Validation(format!(
                                            "Invalid _in reference (canonical/fragment not supported): {}",
                                            v.raw
                                        )));
                                    }
                                }
                            }
                        }
                    }
                }
                SearchParamType::Reference => {
                    // Membership chaining uses special value semantics (collection ids), so skip
                    // the regular reference-value normalization (which would treat id-only values
                    // as references to the *target* resource).
                    if matches!(p.chain.as_deref(), Some("_in")) {
                        if let Some(m) = &p.modifier {
                            if !matches!(m, query_builder::SearchModifier::Not) {
                                return Err(crate::Error::Validation(format!(
                                    "Unsupported modifier '{:?}' for chained membership parameter '{}'",
                                    m, p.raw_name
                                )));
                            }
                        }

                        let base = base_url.as_deref();
                        for v in &mut p.values {
                            let raw = v.raw.trim();
                            if raw.is_empty() {
                                continue;
                            }

                            if raw.contains("://") {
                                let Some(parsed) =
                                    query_builder::parse_reference_query_value(raw, base)
                                else {
                                    return Err(crate::Error::Validation(format!(
                                        "Invalid chained _in reference: {}",
                                        v.raw
                                    )));
                                };

                                match parsed {
                                    query_builder::ParsedReferenceQuery::Absolute {
                                        is_local,
                                        typ,
                                        id,
                                        version,
                                        ..
                                    } => {
                                        if !is_local {
                                            return Err(crate::Error::Validation(format!(
                                                "Chained _in does not support non-local absolute references: {}",
                                                v.raw
                                            )));
                                        }
                                        if version.is_some() {
                                            return Err(crate::Error::Validation(
                                                "Chained _in does not support versioned references"
                                                    .to_string(),
                                            ));
                                        }
                                        let Some(typ) = typ else {
                                            return Err(crate::Error::Validation(format!(
                                                "Chained _in reference must include a type: {}",
                                                v.raw
                                            )));
                                        };
                                        let Some(id) = id else {
                                            return Err(crate::Error::Validation(format!(
                                                "Chained _in reference must include an id: {}",
                                                v.raw
                                            )));
                                        };
                                        if !matches!(typ.as_str(), "CareTeam" | "Group" | "List") {
                                            return Err(crate::Error::Validation(format!(
                                                "Chained _in target must be CareTeam, Group, or List: {}",
                                                v.raw
                                            )));
                                        }
                                        v.raw = format!("{}/{}", typ, id);
                                    }
                                    query_builder::ParsedReferenceQuery::Relative {
                                        typ,
                                        id,
                                        version,
                                    } => {
                                        if version.is_some() {
                                            return Err(crate::Error::Validation(
                                                "Chained _in does not support versioned references"
                                                    .to_string(),
                                            ));
                                        }
                                        if let Some(typ) = typ {
                                            if !matches!(
                                                typ.as_str(),
                                                "CareTeam" | "Group" | "List"
                                            ) {
                                                return Err(crate::Error::Validation(format!(
                                                    "Chained _in target must be CareTeam, Group, or List: {}",
                                                    v.raw
                                                )));
                                            }
                                            v.raw = format!("{}/{}", typ, id);
                                        } else {
                                            v.raw = id;
                                        }
                                    }
                                    query_builder::ParsedReferenceQuery::Canonical { .. }
                                    | query_builder::ParsedReferenceQuery::Fragment { .. } => {
                                        return Err(crate::Error::Validation(format!(
                                            "Invalid chained _in reference (canonical/fragment not supported): {}",
                                            v.raw
                                        )));
                                    }
                                }
                            }
                        }

                        continue;
                    }

                    if matches!(p.chain.as_deref(), Some("_list")) {
                        if let Some(m) = &p.modifier {
                            if !matches!(m, query_builder::SearchModifier::Not) {
                                return Err(crate::Error::Validation(format!(
                                    "Unsupported modifier '{:?}' for chained membership parameter '{}'",
                                    m, p.raw_name
                                )));
                            }
                        }
                        continue;
                    }

                    if matches!(p.modifier, Some(query_builder::SearchModifier::Contains)) {
                        let Some(searched_type) = searched_type_hint else {
                            return Err(crate::Error::Validation(
                                "Reference modifier ':contains' requires a single resource type"
                                    .to_string(),
                            ));
                        };

                        let Some(expected_param) =
                            query_builder::hierarchy_parent_param_for_type(searched_type)
                        else {
                            return Err(crate::Error::Validation(format!(
                                "Reference modifier ':contains' is not supported for resource type '{}'",
                                searched_type
                            )));
                        };

                        if !p.code.eq_ignore_ascii_case(expected_param) {
                            return Err(crate::Error::Validation(format!(
                                "Reference modifier ':contains' is only supported for '{}.{}'",
                                searched_type, expected_param
                            )));
                        }

                        // Validate value forms to avoid silent no-ops.
                        for v in &p.values {
                            let raw = crate::db::search::escape::unescape_search_value(&v.raw)
                                .map_err(|_| {
                                    crate::Error::Validation(format!(
                                        "Invalid escape sequence in reference value: {}",
                                        v.raw
                                    ))
                                })?;
                            let Some(parsed) = query_builder::parse_reference_query_value(
                                raw.as_str(),
                                base_url.as_deref(),
                            ) else {
                                return Err(crate::Error::Validation(format!(
                                    "Invalid reference value for ':contains': {}",
                                    v.raw
                                )));
                            };

                            match parsed {
                                query_builder::ParsedReferenceQuery::Relative {
                                    typ,
                                    version,
                                    ..
                                } => {
                                    if let Some(t) = typ {
                                        if t != searched_type {
                                            return Err(crate::Error::Validation(format!(
                                                "Reference ':contains' value must refer to '{}': {}",
                                                searched_type, v.raw
                                            )));
                                        }
                                    }
                                    if version.is_some() {
                                        return Err(crate::Error::Validation(
                                            "Reference ':contains' does not support versioned references"
                                                .to_string(),
                                        ));
                                    }
                                }
                                query_builder::ParsedReferenceQuery::Absolute {
                                    is_local,
                                    typ,
                                    version,
                                    ..
                                } => {
                                    if !is_local {
                                        return Err(crate::Error::Validation(format!(
                                            "Reference ':contains' does not support non-local absolute references: {}",
                                            v.raw
                                        )));
                                    }
                                    if typ.as_deref() != Some(searched_type) {
                                        return Err(crate::Error::Validation(format!(
                                            "Reference ':contains' value must refer to '{}': {}",
                                            searched_type, v.raw
                                        )));
                                    }
                                    if version.is_some() {
                                        return Err(crate::Error::Validation(
                                            "Reference ':contains' does not support versioned references"
                                                .to_string(),
                                        ));
                                    }
                                }
                                query_builder::ParsedReferenceQuery::Canonical { .. }
                                | query_builder::ParsedReferenceQuery::Fragment { .. } => {
                                    return Err(crate::Error::Validation(
                                        "Reference ':contains' does not support canonical or fragment references"
                                            .to_string(),
                                    ));
                                }
                            }
                        }
                    }

                    // Per spec, `:{type}` modifier restricts the value format to an id only.
                    // Example: `subject:Patient=23` (equivalent to `subject=Patient/23`).
                    if let Some(query_builder::SearchModifier::TypeModifier(type_name)) =
                        &p.modifier
                    {
                        if p.values.is_empty() {
                            return Err(crate::Error::Validation(format!(
                                "Modifier ':{}' requires an id-only value (e.g. ':{}=23')",
                                type_name, type_name
                            )));
                        }
                        for v in &p.values {
                            if !is_untyped_logical_id_reference(&v.raw) {
                                return Err(crate::Error::Validation(format!(
                                    "Modifier ':{}' requires an id-only value (e.g. ':{}=23')",
                                    type_name, type_name
                                )));
                            }
                        }
                        // Skip automatic type resolution for id-only reference values; the type is
                        // already provided by the modifier.
                        continue;
                    }

                    if matches!(p.modifier, Some(query_builder::SearchModifier::Identifier)) {
                        continue;
                    }

                    // For `param={id}` searches, resolve the id to a unique resource type if possible.
                    // If multiple types exist for the same id, fail to avoid ambiguity.
                    for v in &mut p.values {
                        if !is_untyped_logical_id_reference(&v.raw) {
                            continue;
                        }
                        let id = v.raw.trim();

                        let rows: Vec<(String,)> = sqlx::query_as(
                            r#"
                            SELECT DISTINCT resource_type
                            FROM resources
                            WHERE id = $1 AND is_current = true AND deleted = false
                            LIMIT 2
                            "#,
                        )
                        .bind(id)
                        .fetch_all(&mut *conn)
                        .await
                        .map_err(crate::Error::Database)?;

                        if rows.len() > 1 {
                            let types = rows
                                .into_iter()
                                .map(|(t,)| t)
                                .collect::<Vec<_>>()
                                .join(", ");
                            return Err(crate::Error::Validation(format!(
                                "Ambiguous reference id '{}' matches multiple resource types ({types}); specify type explicitly (e.g. 'Patient/{id}' or ':Patient={id}')",
                                id
                            )));
                        }

                        if let Some((t,)) = rows.first() {
                            v.raw = format!("{}/{}", t, id);
                        }
                    }

                    // Validate hierarchy modifiers on reference params.
                    if matches!(
                        p.modifier,
                        Some(
                            query_builder::SearchModifier::Above
                                | query_builder::SearchModifier::Below
                        )
                    ) {
                        let base = base_url.as_deref();
                        for v in &p.values {
                            let Some(parsed) =
                                query_builder::parse_reference_query_value(&v.raw, base)
                            else {
                                return Err(crate::Error::Validation(format!(
                                    "Invalid reference value for ':above'/'below': '{}'",
                                    v.raw
                                )));
                            };

                            match &parsed {
                                query_builder::ParsedReferenceQuery::Canonical { url, version } => {
                                    if url.trim().is_empty() || version.trim().is_empty() {
                                        return Err(crate::Error::Validation(
                                            "Canonical ':above'/'below' searches require 'url|version'"
                                                .to_string(),
                                        ));
                                    }
                                    // Implementation choice: numeric dot-separated versions only.
                                    if !version.split('.').all(|s| {
                                        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
                                    }) {
                                        return Err(crate::Error::Validation(format!(
                                            "Unsupported canonical version '{}' for ':above'/'below' (expected numeric dotted version like '1.2.3')",
                                            version
                                        )));
                                    }
                                    continue;
                                }
                                query_builder::ParsedReferenceQuery::Fragment { .. } => {
                                    return Err(crate::Error::Validation(
                                        "Reference ':above'/'below' modifier is not supported for fragment references"
                                            .to_string(),
                                    ));
                                }
                                query_builder::ParsedReferenceQuery::Absolute {
                                    is_local, ..
                                } => {
                                    if !*is_local {
                                        return Err(crate::Error::Validation(format!(
                                            "Reference ':above'/'below' modifier cannot be used with non-local absolute reference '{}'",
                                            v.raw
                                        )));
                                    }
                                }
                                _ => {}
                            }

                            let target_type = match parsed {
                                query_builder::ParsedReferenceQuery::Relative { typ, .. } => typ,
                                query_builder::ParsedReferenceQuery::Absolute { typ, .. } => typ,
                                _ => None,
                            };

                            let Some(target_type) = target_type else {
                                return Err(crate::Error::Validation(format!(
                                    "Reference ':above'/'below' modifier requires an explicit resource type (e.g. 'Location/{}')",
                                    v.raw.trim()
                                )));
                            };

                            if query_builder::hierarchy_parent_param_for_type(&target_type)
                                .is_none()
                            {
                                return Err(crate::Error::Validation(format!(
                                    "Reference ':above'/'below' modifier is not supported for target type '{}'",
                                    target_type
                                )));
                            }
                        }
                    }
                }
                SearchParamType::Uri => {
                    if matches!(
                        p.modifier,
                        Some(
                            query_builder::SearchModifier::Above
                                | query_builder::SearchModifier::Below
                        )
                    ) {
                        for v in &p.values {
                            let raw = v.raw.trim();
                            if raw.starts_with("urn:") || !raw.contains("://") {
                                return Err(crate::Error::Validation(format!(
                                    "Modifier ':above'/'below' is only supported for URL values (not URNs): '{}'",
                                    raw
                                )));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub(super) fn normalize_filter_expr<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        expr: &'a mut query_builder::FilterExpr,
        base_url: Option<&'a str>,
        searched_type_hint: Option<&'a str>,
    ) -> BoxFuture<'a, Result<()>> {
        async move {
            match expr {
                query_builder::FilterExpr::Atom(atom) => {
                    let mut current_type = searched_type_hint.map(|s| s.to_string());

                    for step in &mut atom.chain {
                        let step_type = if step.target_types.len() == 1 {
                            Some(step.target_types[0].clone())
                        } else {
                            None
                        };

                        if let Some(step_filter) = step.filter.as_mut() {
                            self.normalize_filter_expr(
                                conn,
                                step_filter,
                                base_url,
                                step_type.as_deref(),
                            )
                            .await?;
                        }

                        current_type = step_type;
                    }

                    if let query_builder::FilterAtomKind::Standard(p) = &mut atom.kind {
                        self.normalize_search_params(
                            conn,
                            std::slice::from_mut(p),
                            base_url,
                            current_type.as_deref(),
                        )
                        .await?;
                    }

                    Ok(())
                }
                query_builder::FilterExpr::Has { spec, filter } => {
                    self.normalize_filter_expr(
                        conn,
                        filter,
                        base_url,
                        Some(spec.referring_resource.as_str()),
                    )
                    .await
                }
                query_builder::FilterExpr::And(a, b) | query_builder::FilterExpr::Or(a, b) => {
                    self.normalize_filter_expr(conn, a, base_url, searched_type_hint)
                        .await?;
                    self.normalize_filter_expr(conn, b, base_url, searched_type_hint)
                        .await?;
                    Ok(())
                }
                query_builder::FilterExpr::Not(inner) => {
                    self.normalize_filter_expr(conn, inner, base_url, searched_type_hint)
                        .await
                }
            }
        }
        .boxed()
    }

    async fn expand_valueset_to_token_codes(
        &self,
        conn: &mut PgConnection,
        vs_ref: &str,
    ) -> Result<Vec<String>> {
        let vs_ref = vs_ref.trim();
        if vs_ref.is_empty() {
            return Ok(Vec::new());
        }

        if vs_ref.starts_with('$') {
            return Err(crate::Error::Validation(format!(
                "Functional ValueSet references are not supported for ':in' searches: '{}'",
                vs_ref
            )));
        }

        let valueset: Option<JsonValue> = if vs_ref.contains("://") || vs_ref.starts_with("urn:") {
            sqlx::query_scalar::<_, JsonValue>(
                "SELECT resource FROM resources WHERE resource_type = 'ValueSet' AND is_current = true AND deleted = false AND (url = $1 OR resource->>'url' = $1) LIMIT 1",
            )
            .bind(vs_ref)
            .fetch_optional(&mut *conn)
            .await?
        } else if let Some(id) = vs_ref.strip_prefix("ValueSet/") {
            let id = id.split('/').next().unwrap_or(id).trim();
            sqlx::query_scalar::<_, JsonValue>(
                "SELECT resource FROM resources WHERE resource_type = 'ValueSet' AND is_current = true AND deleted = false AND id = $1 LIMIT 1",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
        } else {
            sqlx::query_scalar::<_, JsonValue>(
                "SELECT resource FROM resources WHERE resource_type = 'ValueSet' AND is_current = true AND deleted = false AND id = $1 LIMIT 1",
            )
            .bind(vs_ref)
            .fetch_optional(&mut *conn)
            .await?
        };

        let Some(valueset) = valueset else {
            return Err(crate::Error::Validation(format!(
                "Unknown ValueSet for ':in' search: '{}'",
                vs_ref
            )));
        };

        let mut out = HashSet::<String>::new();

        if let Some(expansion) = valueset.get("expansion") {
            if let Some(contains) = expansion.get("contains") {
                extract_valueset_expansion_contains(contains, &mut out);
            }
        }

        if out.is_empty() {
            if let Some(compose) = valueset.get("compose") {
                if let Some(includes) = compose.get("include").and_then(|v| v.as_array()) {
                    for include in includes {
                        let Some(system) = include.get("system").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        if let Some(concepts) = include.get("concept").and_then(|v| v.as_array()) {
                            for concept in concepts {
                                let Some(code) = concept.get("code").and_then(|v| v.as_str())
                                else {
                                    continue;
                                };
                                if !system.trim().is_empty() && !code.trim().is_empty() {
                                    out.insert(format!("{}|{}", system.trim(), code.trim()));
                                }
                            }
                        }
                    }
                }
            }
        }

        if out.is_empty() {
            return Err(crate::Error::Validation(format!(
                "ValueSet '{}' cannot be expanded without terminology support (no explicit codes found in 'expansion.contains' or 'compose.include.concept')",
                vs_ref
            )));
        }

        Ok(out.into_iter().collect())
    }
}

fn extract_valueset_expansion_contains(value: &JsonValue, out: &mut HashSet<String>) {
    let Some(arr) = value.as_array() else {
        return;
    };

    for item in arr {
        let system = item.get("system").and_then(|v| v.as_str()).map(str::trim);
        let code = item.get("code").and_then(|v| v.as_str()).map(str::trim);
        if let (Some(system), Some(code)) = (system, code) {
            if !system.is_empty() && !code.is_empty() {
                out.insert(format!("{}|{}", system, code));
            }
        }

        if let Some(nested) = item.get("contains") {
            extract_valueset_expansion_contains(nested, out);
        }
    }
}
