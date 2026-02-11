//! Per-parameter search index insertion helpers.

use crate::db::search::string_normalization::{
    normalize_casefold_strip_combining, normalize_string_for_search,
};
use crate::models::Resource;
use crate::Result;
use zunder_fhirpath::{Collection as FhirPathCollection, ToJson};

use super::text::{extract_all_textual_content, extract_narrative_text};
use super::IndexingService;
use super::{
    extract_date_ranges, extract_identifier_of_type_rows, extract_numbers, extract_quantity_values,
    extract_reference_identifier_tokens, extract_reference_values, extract_strings, extract_tokens,
};

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct InsertStats {
    /// Number of candidate rows for the primary search table.
    pub(super) rows: usize,
    /// Number of candidate rows for an auxiliary table (e.g. `search_token_identifier`).
    pub(super) aux_rows: usize,
}

impl IndexingService {
    pub(super) async fn insert_text_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
    ) -> Result<()> {
        let content = extract_narrative_text(&resource.resource);
        let content = content.trim();
        if content.is_empty() {
            return Ok(());
        }

        sqlx::query(
            "INSERT INTO search_text (resource_type, resource_id, version_id, parameter_name, content)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name)
             DO UPDATE SET content = EXCLUDED.content",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(content)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(())
    }

    pub(super) async fn insert_content_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
    ) -> Result<()> {
        let content = extract_all_textual_content(&resource.resource);
        let content = content.trim();
        if content.is_empty() {
            return Ok(());
        }

        sqlx::query(
            "INSERT INTO search_content (resource_type, resource_id, version_id, parameter_name, content)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name)
             DO UPDATE SET content = EXCLUDED.content",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(content)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(())
    }

    pub(super) async fn insert_string_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        // Store FULL values for FHIR spec compliance (exact matching required).
        // The database uses functional indexes (LEFT(value, 300)) to avoid btree size limits
        // while maintaining full value storage. See migration 002_fix_search_indexes.sql
        let mut raw_values: Vec<String> = Vec::new();
        let mut normalized_values: Vec<String> = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for s in extract_strings(&value) {
                let normalized = normalize_string_for_search(&s);
                raw_values.push(s);
                normalized_values.push(normalized);
            }
        }

        if raw_values.is_empty() {
            return Ok(InsertStats::default());
        }

        let rows = raw_values.len();
        let insert_start = std::time::Instant::now();
        let result = sqlx::query(
            "INSERT INTO search_string (resource_type, resource_id, version_id, parameter_name, value, value_normalized, entry_hash)
             SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.value, t.value_normalized,
                    MD5($1 || $2 || $3::text || $4 || t.value) AS entry_hash
             FROM UNNEST($5::text[], $6::text[]) AS t(value, value_normalized)
             ORDER BY entry_hash
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
             DO UPDATE SET
                 value = EXCLUDED.value,
                 value_normalized = EXCLUDED.value_normalized",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(&raw_values)
        .bind(&normalized_values)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        let insert_time = insert_start.elapsed();
        if insert_time.as_millis() > 100 {
            tracing::warn!(
                "Slow INSERT to search_string for {}/{}.{}: {:?} (rows={}, affected={})",
                resource.resource_type,
                resource.id,
                param_code,
                insert_time,
                rows,
                result.rows_affected()
            );
        }

        Ok(InsertStats { rows, aux_rows: 0 })
    }

    pub(super) async fn insert_token_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        // Store FULL token values for FHIR spec compliance (codes must match exactly).
        // Database uses functional indexes (LEFT(code, 300), etc.) to avoid btree limits.
        // See migration 002_fix_search_indexes.sql
        let mut id_type_systems: Vec<Option<String>> = Vec::new();
        let mut id_type_codes: Vec<String> = Vec::new();
        let mut id_type_codes_ci: Vec<String> = Vec::new();
        let mut id_values: Vec<String> = Vec::new();
        let mut id_values_ci: Vec<String> = Vec::new();

        let mut token_systems: Vec<Option<String>> = Vec::new();
        let mut token_codes: Vec<String> = Vec::new();
        let mut token_codes_ci: Vec<String> = Vec::new();
        let mut token_displays: Vec<Option<String>> = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for row in extract_identifier_of_type_rows(&value) {
                let type_code = match row.type_code {
                    Some(code) if !code.is_empty() => code,
                    _ => continue,
                };
                let type_system = match row.type_system {
                    Some(system) if !system.is_empty() => Some(system),
                    _ => None,
                };
                let value = match row.value {
                    Some(value) if !value.is_empty() => value,
                    _ => continue,
                };
                let type_code_ci = type_code.to_lowercase();
                let value_ci = value.to_lowercase();

                id_type_systems.push(type_system);
                id_type_codes.push(type_code);
                id_type_codes_ci.push(type_code_ci);
                id_values.push(value);
                id_values_ci.push(value_ci);
            }

            for token in extract_tokens(&value) {
                let code = match token.code {
                    Some(code) if !code.is_empty() => code,
                    _ => continue,
                };
                let system = match token.system {
                    Some(system) if !system.is_empty() => Some(system),
                    _ => None,
                };
                let display = match token.display {
                    Some(display) if !display.is_empty() => Some(display),
                    _ => None,
                };
                let code_ci = code.to_lowercase();

                token_systems.push(system);
                token_codes.push(code);
                token_codes_ci.push(code_ci);
                token_displays.push(display);
            }
        }

        if !id_type_codes.is_empty() {
            sqlx::query(
                "INSERT INTO search_token_identifier (resource_type, resource_id, version_id, parameter_name, type_system, type_code, type_code_ci, value, value_ci, entry_hash)
                 SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.type_system, t.type_code, t.type_code_ci, t.value, t.value_ci,
                        MD5($1 || $2 || $3::text || $4 || COALESCE(t.type_system, '') || t.type_code || t.value) AS entry_hash
                 FROM UNNEST($5::text[], $6::text[], $7::text[], $8::text[], $9::text[])
                     AS t(type_system, type_code, type_code_ci, value, value_ci)
                 ORDER BY entry_hash
                 ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
                 DO UPDATE SET
                     type_system = EXCLUDED.type_system,
                     type_code = EXCLUDED.type_code,
                     type_code_ci = EXCLUDED.type_code_ci,
                     value = EXCLUDED.value,
                     value_ci = EXCLUDED.value_ci",
            )
            .bind(&resource.resource_type)
            .bind(&resource.id)
            .bind(resource.version_id)
            .bind(param_code)
            .bind(&id_type_systems)
            .bind(&id_type_codes)
            .bind(&id_type_codes_ci)
            .bind(&id_values)
            .bind(&id_values_ci)
            .execute(&mut **tx)
            .await
            .map_err(crate::Error::Database)?;
        }

        if !token_codes.is_empty() {
            sqlx::query(
                "INSERT INTO search_token (resource_type, resource_id, version_id, parameter_name, system, code, code_ci, display, entry_hash)
                 SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.system, t.code, t.code_ci, t.display,
                        MD5($1 || $2 || $3::text || $4 || COALESCE(t.system, '') || t.code) AS entry_hash
                 FROM UNNEST($5::text[], $6::text[], $7::text[], $8::text[])
                     AS t(system, code, code_ci, display)
                 ORDER BY entry_hash
                 ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
                 DO UPDATE SET
                     system = EXCLUDED.system,
                     code = EXCLUDED.code,
                     code_ci = EXCLUDED.code_ci,
                     display = EXCLUDED.display",
            )
            .bind(&resource.resource_type)
            .bind(&resource.id)
            .bind(resource.version_id)
            .bind(param_code)
            .bind(&token_systems)
            .bind(&token_codes)
            .bind(&token_codes_ci)
            .bind(&token_displays)
            .execute(&mut **tx)
            .await
            .map_err(crate::Error::Database)?;
        }

        Ok(InsertStats {
            rows: token_codes.len(),
            aux_rows: id_type_codes.len(),
        })
    }

    pub(super) async fn insert_date_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        let mut starts = Vec::new();
        let mut ends = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for (start, end) in extract_date_ranges(&value) {
                starts.push(start);
                ends.push(end);
            }
        }

        if starts.is_empty() {
            return Ok(InsertStats::default());
        }

        let rows = starts.len();
        sqlx::query(
            "INSERT INTO search_date (resource_type, resource_id, version_id, parameter_name, start_date, end_date, entry_hash)
             SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.start_date, t.end_date,
                    MD5($1 || $2 || $3::text || $4 || t.start_date::text || t.end_date::text) AS entry_hash
             FROM UNNEST($5::timestamptz[], $6::timestamptz[]) AS t(start_date, end_date)
             ORDER BY entry_hash
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
             DO UPDATE SET
                 start_date = EXCLUDED.start_date,
                 end_date = EXCLUDED.end_date",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(&starts)
        .bind(&ends)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(InsertStats { rows, aux_rows: 0 })
    }

    pub(super) async fn insert_number_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        let mut number_strs: Vec<String> = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for num in extract_numbers(&value) {
                number_strs.push(num.to_string());
            }
        }

        if number_strs.is_empty() {
            return Ok(InsertStats::default());
        }

        let rows = number_strs.len();
        sqlx::query(
            "INSERT INTO search_number (resource_type, resource_id, version_id, parameter_name, value, entry_hash)
             SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.value::numeric,
                    MD5($1 || $2 || $3::text || $4 || t.value) AS entry_hash
             FROM UNNEST($5::text[]) AS t(value)
             ORDER BY entry_hash
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
             DO UPDATE SET
                 value = EXCLUDED.value",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(&number_strs)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(InsertStats { rows, aux_rows: 0 })
    }

    pub(super) async fn insert_quantity_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        let mut value_strs: Vec<String> = Vec::new();
        let mut systems: Vec<Option<String>> = Vec::new();
        let mut codes: Vec<Option<String>> = Vec::new();
        let mut units: Vec<Option<String>> = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for quantity in extract_quantity_values(&value) {
                // Skip if no code and no unit - nothing searchable
                if quantity.code.is_none() && quantity.unit.is_none() {
                    continue;
                }

                value_strs.push(quantity.value.to_string());
                systems.push(quantity.system);
                codes.push(quantity.code);
                units.push(quantity.unit);
            }
        }

        if value_strs.is_empty() {
            return Ok(InsertStats::default());
        }

        let rows = value_strs.len();
        // Per FHIR spec, index both code and unit to support different search semantics:
        // - system+code searches: match only against code (precise)
        // - ||code searches: match against BOTH code and unit (flexible)
        sqlx::query(
            "INSERT INTO search_quantity (resource_type, resource_id, version_id, parameter_name, value, system, code, unit, entry_hash)
             SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.value::numeric, t.system, t.code, t.unit,
                    MD5($1 || $2 || $3::text || $4 || t.value || COALESCE(t.system, '') || COALESCE(t.code, '')) AS entry_hash
             FROM UNNEST($5::text[], $6::text[], $7::text[], $8::text[]) AS t(value, system, code, unit)
             ORDER BY entry_hash
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
             DO UPDATE SET
                 value = EXCLUDED.value,
                 system = EXCLUDED.system,
                 code = EXCLUDED.code,
                 unit = EXCLUDED.unit",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(&value_strs)
        .bind(&systems)
        .bind(&codes)
        .bind(&units)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(InsertStats { rows, aux_rows: 0 })
    }

    pub(super) async fn insert_reference_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        let mut reference_kinds: Vec<String> = Vec::new();
        let mut target_types: Vec<String> = Vec::new();
        let mut target_ids: Vec<String> = Vec::new();
        let mut target_version_ids: Vec<String> = Vec::new();
        let mut target_urls: Vec<String> = Vec::new();
        let mut canonical_urls: Vec<String> = Vec::new();
        let mut canonical_versions: Vec<String> = Vec::new();
        let mut displays: Vec<Option<String>> = Vec::new();

        let mut token_systems: Vec<Option<String>> = Vec::new();
        let mut token_codes: Vec<String> = Vec::new();
        let mut token_codes_ci: Vec<String> = Vec::new();
        let mut token_displays: Vec<Option<String>> = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for reference in extract_reference_values(&value) {
                reference_kinds.push(reference.reference_kind.as_str().to_string());
                target_types.push(reference.target_type);
                target_ids.push(reference.target_id);
                target_version_ids.push(reference.target_version_id);
                target_urls.push(reference.target_url);
                canonical_urls.push(reference.canonical_url);
                canonical_versions.push(reference.canonical_version);
                displays.push(reference.display);
            }

            // Support `:identifier` modifier for reference parameters by indexing Reference.identifier
            // values into the token table under the *same* parameter code.
            for token in extract_reference_identifier_tokens(&value) {
                let code = match token.code {
                    Some(code) if !code.is_empty() => code,
                    _ => continue,
                };
                let system = match token.system {
                    Some(system) if !system.is_empty() => Some(system),
                    _ => None,
                };
                let code_ci = code.to_lowercase();
                token_systems.push(system);
                token_codes.push(code);
                token_codes_ci.push(code_ci);
                token_displays.push(None);
            }
        }

        if !reference_kinds.is_empty() {
            let insert_start = std::time::Instant::now();
            let result = sqlx::query(
                "INSERT INTO search_reference (
                    resource_type,
                    resource_id,
                    version_id,
                    parameter_name,
                    reference_kind,
                    target_type,
                    target_id,
                    target_version_id,
                    target_url,
                    canonical_url,
                    canonical_version,
                    display,
                    entry_hash
                 )
                 SELECT DISTINCT ON (entry_hash)
                    $1, $2, $3, $4,
                    t.reference_kind,
                    t.target_type,
                    t.target_id,
                    t.target_version_id,
                    t.target_url,
                    t.canonical_url,
                    t.canonical_version,
                    t.display,
                    MD5($1 || $2 || $3::text || $4 || t.reference_kind || t.target_type || t.target_id ||
                        COALESCE(t.target_version_id, '') || COALESCE(t.target_url, '') ||
                        COALESCE(t.canonical_url, '') || COALESCE(t.canonical_version, '')) AS entry_hash
                 FROM UNNEST(
                    $5::text[], $6::text[], $7::text[], $8::text[],
                    $9::text[], $10::text[], $11::text[], $12::text[]
                 ) AS t(
                    reference_kind, target_type, target_id, target_version_id,
                    target_url, canonical_url, canonical_version, display
                 )
                 ORDER BY entry_hash
                 ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
                 DO UPDATE SET
                     reference_kind = EXCLUDED.reference_kind,
                     target_type = EXCLUDED.target_type,
                     target_id = EXCLUDED.target_id,
                     target_version_id = EXCLUDED.target_version_id,
                     target_url = EXCLUDED.target_url,
                     canonical_url = EXCLUDED.canonical_url,
                     canonical_version = EXCLUDED.canonical_version,
                     display = EXCLUDED.display",
            )
            .bind(&resource.resource_type)
            .bind(&resource.id)
            .bind(resource.version_id)
            .bind(param_code)
            .bind(&reference_kinds)
            .bind(&target_types)
            .bind(&target_ids)
            .bind(&target_version_ids)
            .bind(&target_urls)
            .bind(&canonical_urls)
            .bind(&canonical_versions)
            .bind(&displays)
            .execute(&mut **tx)
            .await
            .map_err(crate::Error::Database)?;

            let insert_time = insert_start.elapsed();
            if insert_time.as_millis() > 100 {
                tracing::warn!(
                    "Slow INSERT to search_reference for {}/{}.{}: {:?} (rows={}, affected={})",
                    resource.resource_type,
                    resource.id,
                    param_code,
                    insert_time,
                    reference_kinds.len(),
                    result.rows_affected()
                );
            }
        }

        if !token_codes.is_empty() {
            sqlx::query(
                "INSERT INTO search_token (resource_type, resource_id, version_id, parameter_name, system, code, code_ci, display, entry_hash)
                 SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.system, t.code, t.code_ci, t.display,
                        MD5($1 || $2 || $3::text || $4 || COALESCE(t.system, '') || t.code) AS entry_hash
                 FROM UNNEST($5::text[], $6::text[], $7::text[], $8::text[])
                     AS t(system, code, code_ci, display)
                 ORDER BY entry_hash
                 ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
                 DO UPDATE SET
                     system = EXCLUDED.system,
                     code = EXCLUDED.code,
                     code_ci = EXCLUDED.code_ci,
                     display = EXCLUDED.display",
            )
            .bind(&resource.resource_type)
            .bind(&resource.id)
            .bind(resource.version_id)
            .bind(param_code)
            .bind(&token_systems)
            .bind(&token_codes)
            .bind(&token_codes_ci)
            .bind(&token_displays)
            .execute(&mut **tx)
            .await
            .map_err(crate::Error::Database)?;
        }

        Ok(InsertStats {
            rows: reference_kinds.len(),
            aux_rows: token_codes.len(),
        })
    }

    pub(super) async fn insert_uri_values(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param_code: &str,
        values: &FhirPathCollection,
    ) -> Result<InsertStats> {
        // Store FULL URIs for FHIR spec compliance (canonical URLs, references must match exactly).
        // Database uses functional index (LEFT(value, 300)) to avoid btree limits.
        // See migration 002_fix_search_indexes.sql
        let mut raw_values: Vec<String> = Vec::new();
        let mut normalized_values: Vec<String> = Vec::new();

        for value in values.iter().filter_map(|v| v.to_json()) {
            for s in extract_strings(&value) {
                let normalized = normalize_casefold_strip_combining(&s);
                raw_values.push(s);
                normalized_values.push(normalized);
            }
        }

        if raw_values.is_empty() {
            return Ok(InsertStats::default());
        }

        let rows = raw_values.len();
        sqlx::query(
            "INSERT INTO search_uri (resource_type, resource_id, version_id, parameter_name, value, value_normalized, entry_hash)
             SELECT DISTINCT ON (entry_hash) $1, $2, $3, $4, t.value, t.value_normalized,
                    MD5($1 || $2 || $3::text || $4 || t.value) AS entry_hash
             FROM UNNEST($5::text[], $6::text[]) AS t(value, value_normalized)
             ORDER BY entry_hash
             ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
             DO UPDATE SET
                 value = EXCLUDED.value,
                 value_normalized = EXCLUDED.value_normalized",
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(param_code)
        .bind(&raw_values)
        .bind(&normalized_values)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(InsertStats { rows, aux_rows: 0 })
    }
}
