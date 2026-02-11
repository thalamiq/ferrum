use super::util::is_valid_fhir_logical_id;
use super::{params, query_builder, SearchEngine, SearchParameters};
use crate::db::search::escape::split_unescaped;
use crate::db::search::parameter_lookup::{SearchParamDef, SearchParamType};
use crate::Result;
use sqlx::PgConnection;
use std::collections::HashMap;

impl SearchEngine {
    pub(super) async fn resolve_search_params_type(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        params: &SearchParameters,
    ) -> Result<(
        Vec<query_builder::ResolvedParam>,
        Option<query_builder::FilterExpr>,
        Vec<String>,
    )> {
        let mut resolved = Vec::new();
        let mut unknown = Vec::new();
        let mut filter: Option<query_builder::FilterExpr> = None;

        // Count repeats for multiple_and validation.
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for p in &params.resource_params {
            *counts.entry(p.code.as_str()).or_insert(0) += 1;
        }

        for p in &params.resource_params {
            if p.code == "_filter" {
                if p.modifier.is_some() || p.chain.is_some() || p.reverse_chain.is_some() {
                    return Err(crate::Error::Validation(
                        "Search parameter '_filter' does not support modifiers or chaining"
                            .to_string(),
                    ));
                }

                let expr = p.or_values.first().map(|s| s.as_str()).unwrap_or("");
                let next = self
                    .resolve_filter_expression(conn, resource_type, expr)
                    .await?;
                filter = Some(match filter {
                    Some(prev) => query_builder::FilterExpr::And(Box::new(prev), Box::new(next)),
                    None => next,
                });
                continue;
            }
            if p.code == "_query" {
                return Err(crate::Error::Validation(
                    "Search parameter '_query' (named queries) is not supported".to_string(),
                ));
            }

            // Built-in specials
            if let Some(rp) = self.resolve_builtin_param(p)? {
                resolved.push(rp);
                continue;
            }

            // Check for computed parameter hook
            if let Some(hook) = self.computed_hooks.find_query_hook(resource_type, &p.code) {
                let values = query_builder::resolve_values_for_type(
                    SearchParamType::Number,
                    p.modifier
                        .as_deref()
                        .and_then(query_builder::SearchModifier::from_str)
                        .as_ref(),
                    &p.or_values,
                );

                if let Some(transformed) = hook.transform(&values) {
                    resolved.extend(transformed);
                    continue;
                }
            }

            // Handle _has reverse chaining
            if p.code == "_has" {
                let Some(ref spec) = p.reverse_chain else {
                    return Err(crate::Error::Validation(
                        "_has parameter must follow format: _has:<resource>:<param>:<filter>=<value>".to_string()
                    ));
                };

                // Validate referring resource exists
                // Validate referring param exists for that resource
                let Some(referring_param_def) = self
                    .param_cache
                    .get_param_with_conn(conn, &spec.referring_resource, &spec.referring_param)
                    .await?
                else {
                    return Err(crate::Error::Validation(format!(
                        "Unknown search parameter '{}.{}'",
                        spec.referring_resource, spec.referring_param
                    )));
                };

                // Validate it's a reference parameter
                if referring_param_def.param_type != SearchParamType::Reference {
                    return Err(crate::Error::Validation(format!(
                        "Parameter '{}.{}' must be a reference parameter for _has",
                        spec.referring_resource, spec.referring_param
                    )));
                }

                // Validate filter parameter exists
                let Some(filter_param_def) = self
                    .param_cache
                    .get_param_with_conn(conn, &spec.referring_resource, &spec.filter_param)
                    .await?
                else {
                    return Err(crate::Error::Validation(format!(
                        "Unknown filter parameter '{}.{}'",
                        spec.referring_resource, spec.filter_param
                    )));
                };

                // Resolve filter values
                let values = query_builder::resolve_values_for_type(
                    filter_param_def.param_type.clone(),
                    p.modifier
                        .as_deref()
                        .and_then(query_builder::SearchModifier::from_str)
                        .as_ref(),
                    &p.or_values,
                );

                resolved.push(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: "_has".to_string(),
                    param_type: SearchParamType::Special,
                    modifier: None,
                    chain: None,
                    values,
                    composite: None,
                    reverse_chain: p.reverse_chain.clone(),
                    chain_metadata: None,
                });
                continue;
            }

            let Some(def) = self
                .param_cache
                .get_param_with_conn(conn, resource_type, &p.code)
                .await?
            else {
                unknown.push(p.raw_name.clone());
                continue;
            };

            // "special" parameters require code-level support. If we don't implement a special
            // parameter, treat it as unsupported rather than silently no-op'ing later.
            if def.param_type == SearchParamType::Special {
                return Err(crate::Error::Validation(format!(
                    "Search parameter '{}' is not supported",
                    p.code
                )));
            }

            if let Some(chain) = p.chain.as_deref() {
                // Support membership chaining: `patient._in=Group/104`, `ingredient._in:not=List/105`.
                if matches!(chain, "_in" | "_list") {
                    if def.param_type != SearchParamType::Reference {
                        return Err(crate::Error::Validation(format!(
                            "Chained membership parameter '{}.{}' requires '{}' to be a reference parameter",
                            resource_type, p.raw_name, p.code
                        )));
                    }

                    let modifier = match p.modifier.as_deref() {
                        None => None,
                        Some("not") => Some(query_builder::SearchModifier::Not),
                        Some(other) => {
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for chained membership parameter '{}.{}'",
                                other, resource_type, p.raw_name
                            )));
                        }
                    };

                    let mut values = Vec::new();
                    for raw in &p.or_values {
                        let v = raw.trim();
                        if v.is_empty() {
                            continue;
                        }

                        match chain {
                            "_in" => {
                                if v.contains('|') {
                                    return Err(crate::Error::Validation(format!(
                                        "Chained membership parameter '{}.{}' must be a reference (id or Type/id), not token: {}",
                                        resource_type, p.raw_name, raw
                                    )));
                                }
                                if let Some((typ, id)) = v.split_once('/') {
                                    let typ = typ.trim();
                                    let id = id.trim();
                                    if !matches!(typ, "CareTeam" | "Group" | "List") {
                                        return Err(crate::Error::Validation(format!(
                                            "Chained membership parameter '{}.{}' target must be CareTeam, Group, or List: {}",
                                            resource_type, p.raw_name, raw
                                        )));
                                    }
                                    if !is_valid_fhir_logical_id(id) {
                                        return Err(crate::Error::Validation(format!(
                                            "Invalid chained _in target id: {}",
                                            raw
                                        )));
                                    }
                                    values.push(query_builder::SearchValue {
                                        raw: format!("{}/{}", typ, id),
                                        prefix: None,
                                    });
                                } else if v.contains("://") {
                                    values.push(query_builder::SearchValue {
                                        raw: v.to_string(),
                                        prefix: None,
                                    });
                                } else {
                                    if !is_valid_fhir_logical_id(v) {
                                        return Err(crate::Error::Validation(format!(
                                            "Invalid chained _in value (must be a FHIR id): {}",
                                            raw
                                        )));
                                    }
                                    values.push(query_builder::SearchValue {
                                        raw: v.to_string(),
                                        prefix: None,
                                    });
                                }
                            }
                            "_list" => {
                                if v.contains('|') {
                                    return Err(crate::Error::Validation(format!(
                                        "Chained membership parameter '{}.{}' must be a List id or functional literal (no '|'): {}",
                                        resource_type, p.raw_name, raw
                                    )));
                                }
                                let v = v.strip_prefix('$').unwrap_or(v);
                                let id = if let Some((typ, id)) = v.split_once('/') {
                                    if typ.trim() != "List" {
                                        return Err(crate::Error::Validation(format!(
                                            "Chained membership parameter '{}.{}' only supports List targets: {}",
                                            resource_type, p.raw_name, raw
                                        )));
                                    }
                                    id.trim()
                                } else {
                                    v
                                };
                                if !is_valid_fhir_logical_id(id) {
                                    return Err(crate::Error::Validation(format!(
                                        "Invalid chained _list value (must be a FHIR id): {}",
                                        raw
                                    )));
                                }
                                values.push(query_builder::SearchValue {
                                    raw: id.to_string(),
                                    prefix: None,
                                });
                            }
                            _ => {}
                        }
                    }

                    resolved.push(query_builder::ResolvedParam {
                        raw_name: p.raw_name.clone(),
                        code: p.code.clone(),
                        param_type: SearchParamType::Reference,
                        modifier,
                        chain: Some(chain.to_string()),
                        values,
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    });
                    continue;
                }

                // Handle standard chaining (e.g., subject.name=peter)
                let chain_metadata = match self.resolve_chain(conn, resource_type, p, &def).await {
                    Ok(Some(meta)) => Some(meta),
                    Ok(None) => {
                        // Chain resolution failed (unsupported target type or chained param)
                        unknown.push(p.raw_name.clone());
                        continue;
                    }
                    Err(e) => {
                        // Log error but treat as unknown rather than failing the entire search
                        tracing::warn!("Failed to resolve chain for {}: {}", p.raw_name, e);
                        unknown.push(p.raw_name.clone());
                        continue;
                    }
                };

                let modifier = match p.modifier.as_deref() {
                    None => None,
                    Some(m) => {
                        if let Some(modifier) = query_builder::SearchModifier::from_str(m) {
                            Some(modifier)
                        } else if query_builder::is_valid_resource_type(m) {
                            Some(query_builder::SearchModifier::TypeModifier(m.to_string()))
                        } else {
                            unknown.push(p.raw_name.clone());
                            continue;
                        }
                    }
                };

                let values = query_builder::resolve_values_for_type(
                    SearchParamType::Reference,
                    modifier.as_ref(),
                    &p.or_values,
                );

                resolved.push(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: p.code.clone(),
                    param_type: SearchParamType::Reference,
                    modifier,
                    chain: Some(chain.to_string()),
                    values,
                    composite: None,
                    reverse_chain: None,
                    chain_metadata,
                });
                continue;
            }

            if counts.get(p.code.as_str()).copied().unwrap_or(0) > 1 && !def.multiple_and {
                unknown.push(p.raw_name.clone());
                continue;
            }

            if p.or_values.len() > 1 && !def.multiple_or {
                unknown.push(p.raw_name.clone());
                continue;
            }

            let modifier = match p.modifier.as_deref() {
                None => None,
                Some(m) => {
                    // Check if it's a recognized modifier
                    if let Some(modifier) = query_builder::SearchModifier::from_str(m) {
                        Some(modifier)
                    } else {
                        // Check if it might be a resource type modifier for reference params
                        if def.param_type == SearchParamType::Reference
                            && query_builder::is_valid_resource_type(m)
                        {
                            // This is a [type] modifier
                            Some(query_builder::SearchModifier::TypeModifier(m.to_string()))
                        } else {
                            // Unrecognized modifier - reject per FHIR spec
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for search parameter '{}'",
                                m, p.code
                            )));
                        }
                    }
                }
            };

            // Validate modifier is allowed for this parameter type
            if let Some(m) = &modifier {
                if !query_builder::is_modifier_valid_for_type(&def.param_type, m) {
                    return Err(crate::Error::Validation(format!(
                        "Modifier '{}' is not valid for parameter '{}' of type '{:?}'",
                        p.modifier.as_deref().unwrap_or(""),
                        p.code,
                        def.param_type
                    )));
                }

                // Enforce supported modifiers if the definition provides them
                if !matches!(
                    m,
                    query_builder::SearchModifier::Missing
                        | query_builder::SearchModifier::TypeModifier(_)
                ) && !def.modifiers.is_empty()
                    && !def
                        .modifiers
                        .iter()
                        .any(|s| s == p.modifier.as_deref().unwrap_or(""))
                {
                    return Err(crate::Error::Validation(format!(
                        "Modifier '{}' is not supported for search parameter '{}'",
                        p.modifier.as_deref().unwrap_or(""),
                        p.code
                    )));
                }
            }

            // Composite search parameters do not allow modifiers.
            if def.param_type == SearchParamType::Composite && modifier.is_some() {
                return Err(crate::Error::Validation(format!(
                    "Modifiers are not allowed on composite search parameter '{}'",
                    p.code
                )));
            }

            if def.param_type == SearchParamType::Token {
                validate_token_search_value(
                    def.expression.as_deref(),
                    modifier.as_ref(),
                    &p.or_values,
                )?;
            }

            // `:identifier` on reference parameters uses token syntax/validation, but targets
            // Reference.identifier (not the referenced resource's identifiers).
            if def.param_type == SearchParamType::Reference
                && matches!(modifier, Some(query_builder::SearchModifier::Identifier))
            {
                // Reference.identifier is an Identifier, so system|value is always allowed.
                validate_token_search_value(None, None, &p.or_values)?;
            }

            if def.param_type == SearchParamType::Composite {
                validate_composite_search_value(&p.code, &def.components, &p.or_values)?;
            }

            // Validate :missing value early (must be boolean).
            if matches!(modifier, Some(query_builder::SearchModifier::Missing)) {
                if p.or_values.len() != 1 {
                    return Err(crate::Error::Validation(format!(
                        "Invalid :missing value for {} (expected single true|false): {}",
                        p.raw_name, p.raw_value
                    )));
                }
                if p.or_values.first().map(|v| v.to_ascii_lowercase()) != Some("true".to_string())
                    && p.or_values.first().map(|v| v.to_ascii_lowercase())
                        != Some("false".to_string())
                {
                    return Err(crate::Error::Validation(format!(
                        "Invalid :missing value for {} (expected true|false): {}",
                        p.raw_name, p.raw_value
                    )));
                }
            }

            let param_type = def.param_type.clone();
            let is_composite = param_type == SearchParamType::Composite;
            let values = query_builder::resolve_values_for_type(
                param_type.clone(),
                modifier.as_ref(),
                &p.or_values,
            );
            resolved.push(query_builder::ResolvedParam {
                raw_name: p.raw_name.clone(),
                code: p.code.clone(),
                param_type,
                modifier,
                chain: p.chain.clone(),
                values,
                composite: if is_composite {
                    Some(query_builder::CompositeParamMeta {
                        components: def
                            .components
                            .iter()
                            .map(|c| query_builder::CompositeComponentMeta {
                                code: c.component_code.clone(),
                                param_type: c.component_type.clone(),
                            })
                            .collect(),
                    })
                } else {
                    None
                },
                reverse_chain: None,
                chain_metadata: None,
            });
        }

        Ok((resolved, filter, unknown))
    }

    pub(super) async fn resolve_search_params_system(
        &self,
        conn: &mut PgConnection,
        params: &SearchParameters,
    ) -> Result<(
        Vec<query_builder::ResolvedParam>,
        Option<query_builder::FilterExpr>,
        Vec<String>,
    )> {
        // If the client selected a single type via `_type`, system search should behave like a
        // normal type-level search for resolution/validation purposes.
        if params.types.len() == 1 {
            return self
                .resolve_search_params_type(conn, &params.types[0], params)
                .await;
        }

        let mut resolved = Vec::new();
        let mut unknown = Vec::new();
        let filter: Option<query_builder::FilterExpr> = None;

        // Count repeats for multiple_and validation.
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for p in &params.resource_params {
            *counts.entry(p.code.as_str()).or_insert(0) += 1;
        }

        for p in &params.resource_params {
            if matches!(p.code.as_str(), "_filter") {
                return Err(crate::Error::Validation(format!(
                    "Search parameter '_filter' is not supported for multi-type system searches (use a type search instead): {}",
                    p.raw_name
                )));
            }
            if p.code == "_query" {
                return Err(crate::Error::Validation(
                    "Search parameter '_query' (named queries) is not supported".to_string(),
                ));
            }

            if let Some(rp) = self.resolve_builtin_param(p)? {
                resolved.push(rp);
                continue;
            }

            // For multi-type searches, only accept parameters that are defined for all selected
            // resource types with compatible definitions.
            if params.types.is_empty() {
                unknown.push(p.raw_name.clone());
                continue;
            }

            let mut def: Option<crate::db::search::parameter_lookup::SearchParamDef> = None;
            for rt in &params.types {
                let Some(next) = self
                    .param_cache
                    .get_param_with_conn(conn, rt, &p.code)
                    .await?
                else {
                    def = None;
                    break;
                };

                if next.param_type == SearchParamType::Special {
                    // Special parameters require code-level handling; reject rather than no-op.
                    return Err(crate::Error::Validation(format!(
                        "Search parameter '{}' is not supported",
                        p.code
                    )));
                }

                if let Some(prev) = &def {
                    let compatible = prev.param_type == next.param_type
                        && prev.multiple_and == next.multiple_and
                        && prev.multiple_or == next.multiple_or
                        && prev.modifiers == next.modifiers
                        && prev.comparators == next.comparators
                        && prev.components.len() == next.components.len();
                    if !compatible {
                        def = None;
                        break;
                    }
                } else {
                    def = Some(next);
                }
            }

            let Some(def) = def else {
                unknown.push(p.raw_name.clone());
                continue;
            };

            if let Some(chain) = p.chain.as_deref() {
                if matches!(chain, "_in" | "_list") {
                    if def.param_type != SearchParamType::Reference {
                        return Err(crate::Error::Validation(format!(
                            "Chained membership parameter '{}' requires '{}' to be a reference parameter",
                            p.raw_name, p.code
                        )));
                    }

                    let modifier = match p.modifier.as_deref() {
                        None => None,
                        Some("not") => Some(query_builder::SearchModifier::Not),
                        Some(other) => {
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for chained membership parameter '{}'",
                                other, p.raw_name
                            )));
                        }
                    };

                    let mut values = Vec::new();
                    for raw in &p.or_values {
                        let v = raw.trim();
                        if v.is_empty() {
                            continue;
                        }

                        match chain {
                            "_in" => {
                                if v.contains('|') {
                                    return Err(crate::Error::Validation(format!(
                                        "Chained membership parameter '{}' must be a reference (id or Type/id), not token: {}",
                                        p.raw_name, raw
                                    )));
                                }
                                if let Some((typ, id)) = v.split_once('/') {
                                    let typ = typ.trim();
                                    let id = id.trim();
                                    if !matches!(typ, "CareTeam" | "Group" | "List") {
                                        return Err(crate::Error::Validation(format!(
                                            "Chained membership parameter '{}' target must be CareTeam, Group, or List: {}",
                                            p.raw_name, raw
                                        )));
                                    }
                                    if !is_valid_fhir_logical_id(id) {
                                        return Err(crate::Error::Validation(format!(
                                            "Invalid chained _in target id: {}",
                                            raw
                                        )));
                                    }
                                    values.push(query_builder::SearchValue {
                                        raw: format!("{}/{}", typ, id),
                                        prefix: None,
                                    });
                                } else if v.contains("://") {
                                    values.push(query_builder::SearchValue {
                                        raw: v.to_string(),
                                        prefix: None,
                                    });
                                } else {
                                    if !is_valid_fhir_logical_id(v) {
                                        return Err(crate::Error::Validation(format!(
                                            "Invalid chained _in value (must be a FHIR id): {}",
                                            raw
                                        )));
                                    }
                                    values.push(query_builder::SearchValue {
                                        raw: v.to_string(),
                                        prefix: None,
                                    });
                                }
                            }
                            "_list" => {
                                if v.contains('|') {
                                    return Err(crate::Error::Validation(format!(
                                        "Chained membership parameter '{}' must be a List id or functional literal (no '|'): {}",
                                        p.raw_name, raw
                                    )));
                                }
                                let v = v.strip_prefix('$').unwrap_or(v);
                                let id = if let Some((typ, id)) = v.split_once('/') {
                                    if typ.trim() != "List" {
                                        return Err(crate::Error::Validation(format!(
                                            "Chained membership parameter '{}' only supports List targets: {}",
                                            p.raw_name, raw
                                        )));
                                    }
                                    id.trim()
                                } else {
                                    v
                                };
                                if !is_valid_fhir_logical_id(id) {
                                    return Err(crate::Error::Validation(format!(
                                        "Invalid chained _list value (must be a FHIR id): {}",
                                        raw
                                    )));
                                }
                                values.push(query_builder::SearchValue {
                                    raw: id.to_string(),
                                    prefix: None,
                                });
                            }
                            _ => {}
                        }
                    }

                    resolved.push(query_builder::ResolvedParam {
                        raw_name: p.raw_name.clone(),
                        code: p.code.clone(),
                        param_type: SearchParamType::Reference,
                        modifier,
                        chain: Some(chain.to_string()),
                        values,
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    });
                    continue;
                }

                unknown.push(p.raw_name.clone());
                continue;
            }

            if counts.get(p.code.as_str()).copied().unwrap_or(0) > 1 && !def.multiple_and {
                unknown.push(p.raw_name.clone());
                continue;
            }

            if p.or_values.len() > 1 && !def.multiple_or {
                unknown.push(p.raw_name.clone());
                continue;
            }

            let modifier = match p.modifier.as_deref() {
                None => None,
                Some(m) => {
                    // Check if it's a recognized modifier
                    if let Some(modifier) = query_builder::SearchModifier::from_str(m) {
                        Some(modifier)
                    } else {
                        // Check if it might be a resource type modifier for reference params
                        if def.param_type == SearchParamType::Reference
                            && query_builder::is_valid_resource_type(m)
                        {
                            Some(query_builder::SearchModifier::TypeModifier(m.to_string()))
                        } else {
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for search parameter '{}'",
                                m, p.code
                            )));
                        }
                    }
                }
            };

            // Validate modifier is allowed for this parameter type per spec.
            if let Some(m) = &modifier {
                if !query_builder::is_modifier_valid_for_type(&def.param_type, m) {
                    return Err(crate::Error::Validation(format!(
                        "Modifier '{}' is not valid for parameter '{}' of type '{:?}'",
                        p.modifier.as_deref().unwrap_or(""),
                        p.code,
                        def.param_type
                    )));
                }

                // Enforce supported modifiers if the definition provides them
                if !matches!(
                    m,
                    query_builder::SearchModifier::Missing
                        | query_builder::SearchModifier::TypeModifier(_)
                ) && !def.modifiers.is_empty()
                    && !def
                        .modifiers
                        .iter()
                        .any(|s| s == p.modifier.as_deref().unwrap_or(""))
                {
                    return Err(crate::Error::Validation(format!(
                        "Modifier '{}' is not supported for search parameter '{}'",
                        p.modifier.as_deref().unwrap_or(""),
                        p.code
                    )));
                }
            }

            // Composite search parameters do not allow modifiers.
            if def.param_type == SearchParamType::Composite && modifier.is_some() {
                return Err(crate::Error::Validation(format!(
                    "Modifiers are not allowed on composite search parameter '{}'",
                    p.code
                )));
            }

            if def.param_type == SearchParamType::Token {
                validate_token_search_value(
                    def.expression.as_deref(),
                    modifier.as_ref(),
                    &p.or_values,
                )?;
            }

            // `:identifier` on reference parameters uses token syntax/validation.
            if def.param_type == SearchParamType::Reference
                && matches!(modifier, Some(query_builder::SearchModifier::Identifier))
            {
                validate_token_search_value(None, None, &p.or_values)?;
            }

            if def.param_type == SearchParamType::Composite {
                validate_composite_search_value(&p.code, &def.components, &p.or_values)?;
            }

            // Validate :missing value early (must be boolean).
            if matches!(modifier, Some(query_builder::SearchModifier::Missing)) {
                if p.or_values.len() != 1 {
                    return Err(crate::Error::Validation(format!(
                        "Invalid :missing value for {} (expected single true|false): {}",
                        p.raw_name, p.raw_value
                    )));
                }
                if p.or_values.first().map(|v| v.to_ascii_lowercase()) != Some("true".to_string())
                    && p.or_values.first().map(|v| v.to_ascii_lowercase())
                        != Some("false".to_string())
                {
                    return Err(crate::Error::Validation(format!(
                        "Invalid :missing value for {} (expected true|false): {}",
                        p.raw_name, p.raw_value
                    )));
                }
            }

            let param_type = def.param_type.clone();
            let is_composite = param_type == SearchParamType::Composite;
            let values = query_builder::resolve_values_for_type(
                param_type.clone(),
                modifier.as_ref(),
                &p.or_values,
            );
            resolved.push(query_builder::ResolvedParam {
                raw_name: p.raw_name.clone(),
                code: p.code.clone(),
                param_type,
                modifier,
                chain: p.chain.clone(),
                values,
                composite: if is_composite {
                    Some(query_builder::CompositeParamMeta {
                        components: def
                            .components
                            .iter()
                            .map(|c| query_builder::CompositeComponentMeta {
                                code: c.component_code.clone(),
                                param_type: c.component_type.clone(),
                            })
                            .collect(),
                    })
                } else {
                    None
                },
                reverse_chain: None,
                chain_metadata: None,
            });
        }

        Ok((resolved, filter, unknown))
    }

    pub(super) fn resolve_builtin_param(
        &self,
        p: &params::RawSearchParam,
    ) -> Result<Option<query_builder::ResolvedParam>> {
        let code = p.code.as_str();

        // Built-in parameters do not support chaining. Treat any suffix as invalid rather than
        // silently ignoring it.
        if matches!(
            code,
            "_id" | "_lastUpdated" | "_text" | "_content" | "_in" | "_list"
        ) && p.chain.is_some()
        {
            return Err(crate::Error::Validation(format!(
                "Chaining is not supported for search parameter '{}'",
                code
            )));
        }

        match code {
            "_id" => {
                // Spec: `_id` is token-like but code-only (no `|` pipes).
                if p.modifier.is_some() {
                    return Err(crate::Error::Validation(
                        "Search parameter '_id' does not support modifiers".to_string(),
                    ));
                }

                let mut values = Vec::new();
                for raw in &p.or_values {
                    let v = raw.trim();
                    if v.is_empty() {
                        continue;
                    }
                    if v.contains('|') {
                        return Err(crate::Error::Validation(format!(
                            "Search parameter '_id' must not contain '|': {}",
                            raw
                        )));
                    }
                    if !is_valid_fhir_logical_id(v) {
                        return Err(crate::Error::Validation(format!(
                            "Invalid _id value (must be a FHIR id): {}",
                            raw
                        )));
                    }
                    values.push(query_builder::SearchValue {
                        raw: v.to_string(),
                        prefix: None,
                    });
                }

                Ok(Some(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: "_id".to_string(),
                    param_type: SearchParamType::Special,
                    modifier: None,
                    chain: None,
                    values,
                    composite: None,
                    reverse_chain: None,
                    chain_metadata: None,
                }))
            }
            "_lastUpdated" => {
                if p.modifier.is_some() {
                    return Err(crate::Error::Validation(
                        "Search parameter '_lastUpdated' does not support modifiers".to_string(),
                    ));
                }
                Ok(Some(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: "_lastUpdated".to_string(),
                    param_type: SearchParamType::Special,
                    modifier: None,
                    chain: None,
                    values: query_builder::resolve_values_for_type(
                        SearchParamType::Special,
                        None,
                        &p.or_values,
                    ),
                    composite: None,
                    reverse_chain: None,
                    chain_metadata: None,
                }))
            }
            "_text" | "_content" => {
                if code == "_text" && !self.enable_text_search {
                    return Err(crate::Error::Validation(
                        "Search parameter '_text' is disabled by configuration".to_string(),
                    ));
                }
                if code == "_content" && !self.enable_content_search {
                    return Err(crate::Error::Validation(
                        "Search parameter '_content' is disabled by configuration".to_string(),
                    ));
                }

                let param_type = if code == "_text" {
                    SearchParamType::Text
                } else {
                    SearchParamType::Content
                };

                let modifier = match p.modifier.as_deref() {
                    None => None,
                    Some(m) => match query_builder::SearchModifier::from_str(m) {
                        Some(modifier) => Some(modifier),
                        None => {
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for search parameter '{}'",
                                m, code
                            )));
                        }
                    },
                };

                // Keep this intentionally tight: these parameters are implementation-defined,
                // but we should not accept unrelated modifiers.
                if let Some(m) = &modifier {
                    let allowed = matches!(
                        m,
                        query_builder::SearchModifier::Missing
                            | query_builder::SearchModifier::Exact
                            | query_builder::SearchModifier::Contains
                    );
                    if !allowed {
                        return Err(crate::Error::Validation(format!(
                            "Modifier '{:?}' is not supported for search parameter '{}'",
                            m, code
                        )));
                    }
                }

                let values = query_builder::resolve_values_for_type(
                    param_type.clone(),
                    modifier.as_ref(),
                    &p.or_values,
                );
                Ok(Some(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: code.to_string(),
                    param_type,
                    modifier,
                    chain: None,
                    values,
                    composite: None,
                    reverse_chain: None,
                    chain_metadata: None,
                }))
            }
            "_in" => {
                // `_in` is a standard parameter (reference) with non-index semantics; we implement it
                // via membership index tables derived from collection resources.
                let modifier = match p.modifier.as_deref() {
                    None => None,
                    Some(m) => match query_builder::SearchModifier::from_str(m) {
                        Some(query_builder::SearchModifier::Not) => {
                            Some(query_builder::SearchModifier::Not)
                        }
                        Some(query_builder::SearchModifier::Missing) => {
                            Some(query_builder::SearchModifier::Missing)
                        }
                        Some(_) | None => {
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for search parameter '_in'",
                                m
                            )));
                        }
                    },
                };

                let mut values = Vec::new();
                for raw in &p.or_values {
                    let v = raw.trim();
                    if v.is_empty() {
                        continue;
                    }

                    if v.starts_with('$') {
                        return Err(crate::Error::Validation(
                            "Search parameter '_in' does not support functional list literals"
                                .to_string(),
                        ));
                    }

                    // Accept either `id` or `Type/id` (CareTeam/Group/List). Local absolute references
                    // are normalized later (after base_url is known).
                    if v.contains('|') {
                        return Err(crate::Error::Validation(format!(
                            "Search parameter '_in' must be a reference (id or Type/id), not token: {}",
                            raw
                        )));
                    }

                    if let Some((typ, id)) = v.split_once('/') {
                        let typ = typ.trim();
                        let id = id.trim();
                        if !matches!(typ, "CareTeam" | "Group" | "List") {
                            return Err(crate::Error::Validation(format!(
                                "Search parameter '_in' target must be CareTeam, Group, or List: {}",
                                raw
                            )));
                        }
                        if !is_valid_fhir_logical_id(id) {
                            return Err(crate::Error::Validation(format!(
                                "Invalid _in value (must reference a FHIR id): {}",
                                raw
                            )));
                        }
                        values.push(query_builder::SearchValue {
                            raw: format!("{}/{}", typ, id),
                            prefix: None,
                        });
                        continue;
                    }

                    if v.contains("://") {
                        // Defer local absolute validation/normalization to normalize_search_params().
                        values.push(query_builder::SearchValue {
                            raw: v.to_string(),
                            prefix: None,
                        });
                        continue;
                    }

                    if !is_valid_fhir_logical_id(v) {
                        return Err(crate::Error::Validation(format!(
                            "Invalid _in value (must be a FHIR id): {}",
                            raw
                        )));
                    }
                    values.push(query_builder::SearchValue {
                        raw: v.to_string(),
                        prefix: None,
                    });
                }

                Ok(Some(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: "_in".to_string(),
                    param_type: SearchParamType::Special,
                    modifier,
                    chain: None,
                    values,
                    composite: None,
                    reverse_chain: None,
                    chain_metadata: None,
                }))
            }
            "_list" => {
                // `_list` is a standard parameter (special) with token-like inputs:
                // - a List logical id, or
                // - a functional list literal like `$current-allergies` (if materialized as a List).
                let modifier = match p.modifier.as_deref() {
                    None => None,
                    Some(m) => match query_builder::SearchModifier::from_str(m) {
                        Some(query_builder::SearchModifier::Not) => {
                            Some(query_builder::SearchModifier::Not)
                        }
                        Some(query_builder::SearchModifier::Missing) => {
                            Some(query_builder::SearchModifier::Missing)
                        }
                        Some(_) | None => {
                            return Err(crate::Error::Validation(format!(
                                "Unsupported modifier '{}' for search parameter '_list'",
                                m
                            )));
                        }
                    },
                };

                let mut values = Vec::new();
                for raw in &p.or_values {
                    let v = raw.trim();
                    if v.is_empty() {
                        continue;
                    }

                    if v.contains('|') {
                        return Err(crate::Error::Validation(format!(
                            "Search parameter '_list' must be a List id or functional literal (no '|'): {}",
                            raw
                        )));
                    }

                    // Functional list: `$current-allergies` -> `current-allergies` (List.id).
                    let v = v.strip_prefix('$').unwrap_or(v);

                    // Accept `List/{id}` or `{id}`.
                    let id = if let Some((typ, id)) = v.split_once('/') {
                        if typ.trim() != "List" {
                            return Err(crate::Error::Validation(format!(
                                "Search parameter '_list' only supports List targets: {}",
                                raw
                            )));
                        }
                        id.trim()
                    } else {
                        v
                    };

                    if !is_valid_fhir_logical_id(id) {
                        return Err(crate::Error::Validation(format!(
                            "Invalid _list value (must be a FHIR id): {}",
                            raw
                        )));
                    }

                    values.push(query_builder::SearchValue {
                        raw: id.to_string(),
                        prefix: None,
                    });
                }

                Ok(Some(query_builder::ResolvedParam {
                    raw_name: p.raw_name.clone(),
                    code: "_list".to_string(),
                    param_type: SearchParamType::Special,
                    modifier,
                    chain: None,
                    values,
                    composite: None,
                    reverse_chain: None,
                    chain_metadata: None,
                }))
            }
            _ => Ok(None),
        }
    }

    /// Resolve a chained parameter (e.g., subject.name=peter)
    ///
    /// Returns Some(ChainMetadata) if the chain can be resolved, None if it can't be supported
    async fn resolve_chain(
        &self,
        conn: &mut sqlx::PgConnection,
        resource_type: &str,
        raw_param: &crate::db::search::params::RawSearchParam,
        base_def: &SearchParamDef,
    ) -> Result<Option<query_builder::ChainMetadata>> {
        // Base parameter must be a reference
        if base_def.param_type != SearchParamType::Reference {
            return Ok(None);
        }

        let chain_str = raw_param.chain.as_deref().unwrap_or("");

        // Parse chain: could be "name" or "Patient.name"
        // The chain string from the parser contains everything after the first dot
        let (chain_type_filter, chain_param_code, chain_modifier) = parse_chain_string(chain_str);

        // Determine target types for the reference parameter
        let mut target_types = Vec::new();

        // If chain has explicit type (e.g., subject:Patient.name), use the type modifier
        if let Some(ref m) = raw_param.modifier {
            if query_builder::is_valid_resource_type(m) {
                target_types.push(m.clone());
            }
        }

        // If no explicit type modifier, check if chain starts with a resource type
        if target_types.is_empty() {
            if let Some(ref type_filter) = chain_type_filter {
                if query_builder::is_valid_resource_type(type_filter) {
                    target_types.push(type_filter.clone());
                }
            }
        }

        // If still no target types, use the reference parameter's target types from definition
        if target_types.is_empty() {
            // Parse target types from expression
            // For now, try common reference targets from the parameter name
            target_types = infer_reference_targets(resource_type, &base_def.code);
        }

        if target_types.is_empty() {
            return Ok(None);
        }

        // Try to resolve the chained parameter on each target type
        // Use the first successful resolution
        for target_type in &target_types {
            let chain_param_def = self
                .param_cache
                .get_param_with_conn(conn, target_type, &chain_param_code)
                .await?;

            if let Some(def) = chain_param_def {
                // Verify the chained parameter supports the modifier if present
                if let Some(ref modifier) = chain_modifier {
                    if !query_builder::is_modifier_valid_for_type(&def.param_type, modifier) {
                        continue;
                    }
                }

                // Successfully resolved the chain
                return Ok(Some(query_builder::ChainMetadata {
                    target_types: target_types.clone(),
                    param_code: chain_param_code.clone(),
                    param_type: def.param_type.clone(),
                    modifier: chain_modifier,
                }));
            }
        }

        // Could not resolve the chained parameter on any target type
        Ok(None)
    }
}

fn validate_token_search_value(
    expression: Option<&str>,
    modifier: Option<&query_builder::SearchModifier>,
    raw_values: &[String],
) -> Result<()> {
    use query_builder::SearchModifier;

    // We don't support terminology-backed modifiers yet.
    if matches!(
        modifier,
        Some(
            SearchModifier::In
                | SearchModifier::NotIn
                | SearchModifier::Above
                | SearchModifier::Below
        )
    ) {
        return Err(crate::Error::Validation(
            "Token modifiers ':in', ':not-in', ':above', and ':below' are not supported yet"
                .to_string(),
        ));
    }

    for raw in raw_values {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }

        let pipe_parts = split_unescaped(raw, '|');

        if matches!(modifier, Some(SearchModifier::OfType)) {
            // :of-type is only defined for token parameters targeting Identifier.
            let expr = expression.unwrap_or("").to_ascii_lowercase();
            if !expr.contains("identifier") {
                return Err(crate::Error::Validation(
                    "Token modifier ':of-type' is only supported for token parameters targeting Identifier"
                        .to_string(),
                ));
            }
            if pipe_parts.len() != 3 {
                return Err(crate::Error::Validation(format!(
                    "Invalid :of-type token value (expected 'system|code|value'): {}",
                    raw
                )));
            }
            let system =
                crate::db::search::escape::unescape_search_value(pipe_parts[0]).map_err(|_| {
                    crate::Error::Validation(format!(
                        "Invalid escape sequence in :of-type token value: {}",
                        raw
                    ))
                })?;
            let code =
                crate::db::search::escape::unescape_search_value(pipe_parts[1]).map_err(|_| {
                    crate::Error::Validation(format!(
                        "Invalid escape sequence in :of-type token value: {}",
                        raw
                    ))
                })?;
            let value =
                crate::db::search::escape::unescape_search_value(pipe_parts[2]).map_err(|_| {
                    crate::Error::Validation(format!(
                        "Invalid escape sequence in :of-type token value: {}",
                        raw
                    ))
                })?;
            if system.trim().is_empty() || code.trim().is_empty() || value.trim().is_empty() {
                return Err(crate::Error::Validation(format!(
                    "Invalid :of-type token value (system, code, and value must be present): {}",
                    raw
                )));
            }
            continue;
        }

        if pipe_parts.len() > 2 {
            return Err(crate::Error::Validation(format!(
                "Invalid token value (unexpected extra '|'): {}",
                raw
            )));
        }

        if pipe_parts.len() == 2 {
            let system = pipe_parts[0];
            let code = pipe_parts[1];
            // `system|` is allowed, but `|` is not.
            if system.is_empty() && code.is_empty() {
                return Err(crate::Error::Validation(format!(
                    "Invalid token value (empty system and code): {}",
                    raw
                )));
            }
        }
    }

    Ok(())
}

fn validate_composite_search_value(
    code: &str,
    components: &[crate::db::search::parameter_lookup::CompositeComponentDef],
    raw_values: &[String],
) -> Result<()> {
    if components.is_empty() {
        return Err(crate::Error::Validation(format!(
            "Composite search parameter '{}' has no components",
            code
        )));
    }
    let expected = components.len();
    for raw in raw_values {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let tuple = query_builder::parse_composite_tuple(raw, expected).map_err(|_| {
            crate::Error::Validation(format!(
                "Invalid composite value for '{}': expected {} components separated by '$': {}",
                code, expected, raw
            ))
        })?;
        for (idx, part) in tuple.iter().enumerate() {
            let comp_type = components
                .get(idx)
                .map(|c| c.component_type.clone())
                .ok_or_else(|| {
                    crate::Error::Validation(format!(
                        "Invalid composite value for '{}': missing component {}",
                        code,
                        idx + 1
                    ))
                })?;
            if !query_builder::validate_composite_component_value(comp_type, part, None) {
                return Err(crate::Error::Validation(format!(
                    "Invalid composite value for '{}': component {} is invalid: {}",
                    code,
                    idx + 1,
                    part
                )));
            }
        }
    }
    Ok(())
}

/// Parse a chain string into components
/// Returns (type_filter, param_code, modifier)
/// Examples:
///   "name" -> (None, "name", None)
///   "Patient.name" -> (Some("Patient"), "name", None)
///   "name:exact" -> (None, "name", Some(SearchModifier::Exact))
///   "Patient.name:exact" -> (Some("Patient"), "name", Some(SearchModifier::Exact))
fn parse_chain_string(
    chain: &str,
) -> (
    Option<String>,
    String,
    Option<query_builder::SearchModifier>,
) {
    // Check if there's a type prefix (e.g., "Patient.name")
    if let Some((prefix, rest)) = chain.split_once('.') {
        // prefix might be a resource type
        let (param_and_modifier, modifier) = parse_param_and_modifier(rest);
        (Some(prefix.to_string()), param_and_modifier, modifier)
    } else {
        // No dot, just parse param and modifier
        let (param_and_modifier, modifier) = parse_param_and_modifier(chain);
        (None, param_and_modifier, modifier)
    }
}

/// Parse parameter name and optional modifier
/// Returns (param_code, modifier)
fn parse_param_and_modifier(s: &str) -> (String, Option<query_builder::SearchModifier>) {
    if let Some((param, modifier_str)) = s.split_once(':') {
        let modifier = query_builder::SearchModifier::from_str(modifier_str);
        (param.to_string(), modifier)
    } else {
        (s.to_string(), None)
    }
}

/// Infer likely reference target types based on the parameter name and source resource type
/// This is a heuristic for when the chain doesn't explicitly specify the target type
fn infer_reference_targets(resource_type: &str, param_code: &str) -> Vec<String> {
    // Common reference parameter names and their typical targets
    match param_code.to_lowercase().as_str() {
        "patient" | "subject" if resource_type == "DiagnosticReport" => {
            vec![
                "Patient".to_string(),
                "Group".to_string(),
                "Location".to_string(),
                "Device".to_string(),
            ]
        }
        "patient" | "subject" if resource_type == "Observation" => {
            vec![
                "Patient".to_string(),
                "Group".to_string(),
                "Device".to_string(),
                "Location".to_string(),
            ]
        }
        "patient" => vec!["Patient".to_string()],
        "subject" => vec!["Patient".to_string()],
        "encounter" => vec!["Encounter".to_string()],
        "practitioner" | "performer" => vec!["Practitioner".to_string()],
        "organization" => vec!["Organization".to_string()],
        "location" => vec!["Location".to_string()],
        "device" => vec!["Device".to_string()],
        "medication" => vec!["Medication".to_string()],
        "general-practitioner" => vec!["Practitioner".to_string(), "Organization".to_string()],
        _ => {
            // Default: try the most common types
            vec![
                "Patient".to_string(),
                "Practitioner".to_string(),
                "Organization".to_string(),
            ]
        }
    }
}
