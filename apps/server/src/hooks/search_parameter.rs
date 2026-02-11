//! SearchParameter hook - updates search_parameters table when SearchParameter resources change

use super::ResourceHook;
use crate::db::SearchEngine;
use crate::models::Resource;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::borrow::Cow;
use std::sync::Arc;

use crate::{services::IndexingService, Result};

pub struct SearchParameterHook {
    pool: PgPool,
    indexing_service: Arc<IndexingService>,
    search_engine: Arc<SearchEngine>,
    active_statuses: Vec<String>,
}

impl SearchParameterHook {
    pub fn new(
        pool: PgPool,
        indexing_service: Arc<IndexingService>,
        search_engine: Arc<SearchEngine>,
        active_statuses: Vec<String>,
    ) -> Self {
        Self {
            pool,
            indexing_service,
            search_engine,
            active_statuses,
        }
    }

    async fn upsert_search_parameter(&self, resource: &Value) -> Result<()> {
        // Extract fields from SearchParameter resource
        let code = resource
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::Internal("SearchParameter missing code".to_string()))?;

        let mut bases: Vec<String> = resource
            .get("base")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .ok_or_else(|| crate::Error::Internal("SearchParameter missing base".to_string()))?;

        let mut type_: String = resource
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::Internal("SearchParameter missing type".to_string()))?
            .to_string();

        let mut expression: Option<String> = resource
            .get("expression")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let url = resource.get("url").and_then(|v| v.as_str());
        let description = resource.get("description").and_then(|v| v.as_str());

        // `_text` / `_content` are standard parameters, but their SearchParameter resources are
        // typically typed as `string`. This server stores them as custom types so indexing and
        // querying go through `search_text` / `search_content` (not `search_string`).
        //
        // Keep the scope aligned with the spec:
        // - `_text`: DomainResource (narrative)
        // - `_content`: Resource (all text)
        if code.eq_ignore_ascii_case("_text") {
            type_ = "text".to_string();
            expression = None;
            bases = vec!["DomainResource".to_string()];
        } else if code.eq_ignore_ascii_case("_content") {
            type_ = "content".to_string();
            expression = None;
            bases = vec!["Resource".to_string()];
        }

        // Determine active status based on configured SearchParameter.status values.
        let status = resource.get("status").and_then(|v| v.as_str());
        let active = status
            .map(|s| {
                self.active_statuses
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(s))
            })
            .unwrap_or(false);

        // Extract optional fields
        let multiple_or = resource
            .get("multipleOr")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let multiple_and = resource
            .get("multipleAnd")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Extract comparators
        let comparators: Option<Vec<String>> = resource
            .get("comparator")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        // Extract modifiers
        let modifiers: Option<Vec<String>> = resource
            .get("modifier")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        // Extract chains (for reference parameters)
        let chains: Option<Vec<String>> =
            resource.get("chain").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        // Extract targets (for reference parameters capability statement)
        let targets: Option<Vec<String>> =
            resource
                .get("target")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                });

        for base in &bases {
            let expression_for_base: Option<Cow<'_, str>> = expression.as_deref().map(|expr| {
                simplify_search_parameter_expression(expr, base)
                    .map(Cow::Owned)
                    .unwrap_or_else(|| Cow::Borrowed(expr))
            });

            // Upsert into search_parameters table (one row per base type)
            let row = sqlx::query(
                r#"
                INSERT INTO search_parameters (
                    code, resource_type, type, expression, url, description,
                    active, multiple_or, multiple_and, comparators, modifiers, chains, targets,
                    created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())
                ON CONFLICT (code, resource_type)
                DO UPDATE SET
                    type = EXCLUDED.type,
                    expression = EXCLUDED.expression,
                    url = EXCLUDED.url,
                    description = EXCLUDED.description,
                    active = EXCLUDED.active,
                    multiple_or = EXCLUDED.multiple_or,
                    multiple_and = EXCLUDED.multiple_and,
                    comparators = EXCLUDED.comparators,
                    modifiers = EXCLUDED.modifiers,
                    chains = EXCLUDED.chains,
                    targets = EXCLUDED.targets,
                    updated_at = NOW()
                RETURNING id
                "#,
            )
            .bind(code)
            .bind(base)
            .bind(type_.as_str())
            .bind(expression_for_base.as_deref())
            .bind(url)
            .bind(description)
            .bind(active)
            .bind(multiple_or)
            .bind(multiple_and)
            .bind(comparators.as_deref())
            .bind(modifiers.as_deref())
            .bind(chains.as_deref())
            .bind(targets.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

            let search_param_id: i32 = row
                .try_get::<i32, _>("id")
                .map_err(crate::Error::Database)?;

            // Upsert composite components (if present)
            // Resolve component code and type from definition_url at write time for performance
            sqlx::query("DELETE FROM search_parameter_components WHERE search_parameter_id = $1")
                .bind(search_param_id)
                .execute(&self.pool)
                .await
                .map_err(crate::Error::Database)?;

            if let Some(components) = resource.get("component").and_then(|v| v.as_array()) {
                for (idx, comp) in components.iter().enumerate() {
                    let definition_url = comp.get("definition").and_then(|v| v.as_str());
                    let expression = comp.get("expression").and_then(|v| v.as_str());
                    let Some(definition_url) = definition_url else {
                        continue;
                    };

                    // Resolve component code and type from the referenced SearchParameter
                    let component_row = sqlx::query(
                        "SELECT code, type FROM search_parameters WHERE url = $1 AND active = TRUE LIMIT 1"
                    )
                    .bind(definition_url)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(crate::Error::Database)?;

                    let (component_code, component_type) = match component_row {
                        Some(row) => {
                            let code: String = row
                                .try_get::<String, _>("code")
                                .map_err(crate::Error::Database)?;
                            let type_: String = row
                                .try_get::<String, _>("type")
                                .map_err(crate::Error::Database)?;
                            (Some(code), Some(type_))
                        }
                        None => {
                            tracing::warn!(
                                "Composite component definition_url '{}' not found or inactive for SearchParameter code={}",
                                definition_url,
                                code
                            );
                            (None, None)
                        }
                    };

                    sqlx::query(
                        r#"
                        INSERT INTO search_parameter_components
                        (search_parameter_id, position, definition_url, expression, component_code, component_type)
                        VALUES ($1, $2, $3, $4, $5, $6)
                        "#,
                    )
                    .bind(search_param_id)
                    .bind(idx as i32)
                    .bind(definition_url)
                    .bind(expression)
                    .bind(component_code)
                    .bind(component_type)
                    .execute(&self.pool)
                    .await
                    .map_err(crate::Error::Database)?;
                }
            }

            // Invalidate cache for this resource type
            self.indexing_service.invalidate_cache(Some(base));
        }

        self.update_parameter_version(&bases).await?;
        self.search_engine.invalidate_param_cache();

        Ok(())
    }

    async fn delete_search_parameter(&self, resource: &Value) -> Result<()> {
        let code = resource
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::Internal("SearchParameter missing code".to_string()))?;

        let bases: Vec<String> = resource
            .get("base")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .ok_or_else(|| crate::Error::Internal("SearchParameter missing base".to_string()))?;

        for base in &bases {
            sqlx::query("DELETE FROM search_parameters WHERE code = $1 AND resource_type = $2")
                .bind(code)
                .bind(base)
                .execute(&self.pool)
                .await
                .map_err(crate::Error::Database)?;

            // Invalidate cache for this resource type
            self.indexing_service.invalidate_cache(Some(base));
        }

        self.update_parameter_version(&bases).await?;
        self.search_engine.invalidate_param_cache();

        Ok(())
    }

    async fn update_parameter_version(&self, bases: &[String]) -> Result<()> {
        if bases.is_empty() {
            return Ok(());
        }

        // Determine which resource types are affected
        let mut targets = Vec::new();
        for base in bases {
            if base == "Resource" || base == "DomainResource" {
                // Base-type parameters affect all resource types - get all distinct types
                let all_types: Vec<String> = sqlx::query_scalar(
                    r#"
                    SELECT DISTINCT resource_type
                    FROM resources
                    WHERE is_current = true AND deleted = false
                    ORDER BY resource_type
                    "#,
                )
                .fetch_all(&self.pool)
                .await
                .map_err(crate::Error::Database)?;

                targets = all_types;
                break;
            }
            targets.push(base.clone());
        }

        // Update version for each affected resource type
        for resource_type in targets {
            // Compute new hash from all active search parameters for this resource type
            let params: Vec<(i32, String, String)> = sqlx::query_as(
                r#"
                SELECT id, code, type
                FROM search_parameters
                WHERE resource_type = $1 AND active = true
                ORDER BY code, type
                "#,
            )
            .bind(&resource_type)
            .fetch_all(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

            // Create hash from sorted parameter definitions using PostgreSQL MD5
            let hash_input = params
                .iter()
                .map(|(id, code, type_)| format!("{}-{}-{}", id, code, type_))
                .collect::<Vec<_>>()
                .join("|");

            let new_hash: String = sqlx::query_scalar("SELECT MD5($1)")
                .bind(&hash_input)
                .fetch_one(&self.pool)
                .await
                .map_err(crate::Error::Database)?;

            let param_count = params.len() as i32;

            // Upsert version record with incremented version_number
            sqlx::query(
                r#"
                INSERT INTO search_parameter_versions (resource_type, version_number, current_hash, param_count, updated_at)
                VALUES ($1, 1, $2, $3, NOW())
                ON CONFLICT (resource_type) DO UPDATE
                SET version_number = search_parameter_versions.version_number + 1,
                    current_hash = EXCLUDED.current_hash,
                    param_count = EXCLUDED.param_count,
                    updated_at = NOW()
                "#,
            )
            .bind(&resource_type)
            .bind(&new_hash)
            .bind(param_count)
            .execute(&self.pool)
            .await
            .map_err(crate::Error::Database)?;

            tracing::debug!(
                "Updated search parameter version for {}: hash={}, param_count={}",
                resource_type,
                new_hash,
                param_count
            );
        }

        Ok(())
    }
}

fn simplify_search_parameter_expression(expr: &str, base: &str) -> Option<String> {
    // Base-type parameters (Resource/DomainResource) may intentionally cover many resources.
    // We do not prune unions for these, but we can safely drop a redundant leading type prefix
    // (`Resource.` / `DomainResource.`) when it appears.

    let parts = split_top_level_union(expr)?;

    if base == "Resource" || base == "DomainResource" {
        let mut out = Vec::new();
        let mut changed = false;

        for part in parts {
            let candidate = strip_wrapping_parens(part).trim();
            if candidate.is_empty() {
                continue;
            }

            let rest = match candidate.strip_prefix(base) {
                Some(r) => r.trim_start(),
                None => {
                    out.push(candidate.to_string());
                    continue;
                }
            };

            if let Some(stripped) = rest.strip_prefix('.') {
                let suffix = stripped.trim_start();
                if suffix.is_empty() {
                    out.push(candidate.to_string());
                } else {
                    out.push(suffix.to_string());
                    changed = true;
                }
            } else if rest.is_empty() {
                out.push(candidate.to_string());
            } else {
                // e.g. "ResourceType" shouldn't match base "Resource"
                out.push(candidate.to_string());
            }
        }

        if !changed || out.is_empty() {
            return None;
        }

        let simplified = if out.len() == 1 {
            out.into_iter().next().unwrap()
        } else {
            out.into_iter()
                .map(|m| format!("({})", m.trim()))
                .collect::<Vec<_>>()
                .join(" | ")
        };

        return Some(simplified);
    }

    let mut matches = Vec::new();
    for part in parts {
        let candidate = strip_wrapping_parens(part).trim();
        if candidate.is_empty() {
            continue;
        }

        // Only optimize when the clause clearly targets exactly this base type.
        let rest = match candidate.strip_prefix(base) {
            Some(r) => r.trim_start(),
            None => continue,
        };

        let simplified = if let Some(stripped) = rest.strip_prefix('.') {
            let suffix = stripped.trim_start();
            if suffix.is_empty() {
                candidate.to_string()
            } else {
                suffix.to_string()
            }
        } else if rest.is_empty() {
            candidate.to_string()
        } else {
            // e.g. "PatientName" shouldn't match base "Patient"
            continue;
        };

        matches.push(simplified);
    }

    if matches.is_empty() {
        return None;
    }

    // If we didn't actually drop anything (single match and no union pruning opportunity),
    // only return a simplification if we removed the base prefix.
    let simplified = if matches.len() == 1 {
        matches.into_iter().next().unwrap()
    } else {
        matches
            .into_iter()
            .map(|m| format!("({})", m.trim()))
            .collect::<Vec<_>>()
            .join(" | ")
    };

    Some(simplified)
}

fn split_top_level_union(expr: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut start = 0usize;

    for (idx, ch) in expr.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '(' if !in_single && !in_double => depth = depth.saturating_add(1),
            ')' if !in_single && !in_double => depth = depth.saturating_sub(1),
            '|' if !in_single && !in_double && depth == 0 => {
                parts.push(&expr[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        return None;
    }

    parts.push(&expr[start..]);
    Some(parts)
}

fn strip_wrapping_parens(input: &str) -> &str {
    let mut s = input.trim();
    loop {
        if !s.starts_with('(') || !s.ends_with(')') {
            return s;
        }

        // Only strip if the outermost parens wrap the entire expression.
        let mut depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut wraps_entire = false;

        for (idx, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '(' if !in_single && !in_double => depth += 1,
                ')' if !in_single && !in_double => {
                    depth -= 1;
                    if depth == 0 {
                        wraps_entire = idx == s.len() - 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        if !wraps_entire {
            return s;
        }

        s = s[1..s.len() - 1].trim();
    }
}

#[cfg(test)]
mod tests {
    use super::simplify_search_parameter_expression;

    #[test]
    fn simplifies_top_level_union_to_target_base_multiple_clauses() {
        let expr = "(CapabilityStatement.useContext.value as Quantity) | (CapabilityStatement.useContext.value as Range) | (ValueSet.useContext.value as Quantity) | (ValueSet.useContext.value as Range)";
        let simplified = simplify_search_parameter_expression(expr, "ValueSet").unwrap();
        assert_eq!(
            simplified,
            "(useContext.value as Quantity) | (useContext.value as Range)"
        );
    }

    #[test]
    fn returns_none_for_base_types() {
        let expr = "(ValueSet.useContext.code) | (CodeSystem.useContext.code)";
        assert!(simplify_search_parameter_expression(expr, "Resource").is_none());
        assert!(simplify_search_parameter_expression(expr, "DomainResource").is_none());
    }

    #[test]
    fn returns_none_when_not_a_top_level_union() {
        let expr = "ValueSet.useContext.value as Quantity";
        assert!(simplify_search_parameter_expression(expr, "ValueSet").is_none());
    }

    #[test]
    fn does_not_split_union_inside_strings() {
        // '|' inside string literal should not be treated as union separator.
        let expr = "(ValueSet.name = 'a|b') | (CodeSystem.name = 'c|d')";
        let simplified = simplify_search_parameter_expression(expr, "ValueSet").unwrap();
        assert_eq!(simplified, "name = 'a|b'");
    }
}

#[async_trait]
impl ResourceHook for SearchParameterHook {
    async fn on_created(&self, resource: &Resource) -> Result<()> {
        if resource.resource_type != "SearchParameter" {
            return Ok(());
        }

        self.upsert_search_parameter(&resource.resource).await
    }

    async fn on_updated(&self, resource: &Resource) -> Result<()> {
        if resource.resource_type != "SearchParameter" {
            return Ok(());
        }

        self.upsert_search_parameter(&resource.resource).await
    }

    async fn on_deleted(&self, resource_type: &str, id: &str, version: i32) -> Result<()> {
        if resource_type != "SearchParameter" {
            return Ok(());
        }

        // Fetch the previous version to get the SearchParameter data
        // (the deleted version only contains minimal data)
        let previous_version = version - 1;
        let row = sqlx::query(
            "SELECT resource FROM resources
             WHERE resource_type = $1 AND id = $2 AND version_id = $3",
        )
        .bind(resource_type)
        .bind(id)
        .bind(previous_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::Error::Database)?;

        if let Some(row) = row {
            let resource: serde_json::Value = row.get("resource");
            self.delete_search_parameter(&resource).await?;
        }

        Ok(())
    }
}
