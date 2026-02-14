//! Patient.age computed parameter
//!
//! The age parameter is computed at query time from Patient.birthDate.
//!
//! Index behavior:
//! - Extracts Patient.birthDate
//! - Stores it in search_date table with parameter_name='age'
//!
//! Query behavior:
//! - Transforms age queries into birthdate range queries
//! - Example: age=34 → birthdate between 1991-01-01 and 1992-01-01

use super::{IndexHook, QueryHook};
use crate::db::search::parameter_lookup::SearchParamType;
use crate::db::search::query_builder::{ResolvedParam, SearchPrefix, SearchValue};
use crate::models::Resource;
use crate::services::indexing::SearchParameter;
use crate::Result;
use chrono::{Datelike, Utc};
use ferrum_fhirpath::{Context, ToJson};

pub struct AgeIndexHook;

#[async_trait::async_trait]
impl IndexHook for AgeIndexHook {
    fn resource_type(&self) -> &'static str {
        "Patient"
    }

    fn parameter_code(&self) -> &'static str {
        "age"
    }

    async fn index(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource: &Resource,
        param: &SearchParameter,
        ctx: &Context,
        fhirpath_engine: &ferrum_fhirpath::Engine,
    ) -> Result<()> {
        use crate::services::indexing::extract_date_ranges;

        // Evaluate Patient.birthDate
        let plan = fhirpath_engine
            .compile("Patient.birthDate", Some("Patient"))
            .map_err(|e| crate::Error::FhirPath(e.to_string()))?;

        let collection = fhirpath_engine
            .evaluate(&plan, ctx)
            .map_err(|e| crate::Error::FhirPath(e.to_string()))?;

        if collection.is_empty() {
            return Ok(());
        }

        // Extract date ranges from birthDate
        let mut starts = Vec::new();
        let mut ends = Vec::new();

        for value in collection.iter().filter_map(|v| v.to_json()) {
            for (start, end) in extract_date_ranges(&value) {
                starts.push(start);
                ends.push(end);
            }
        }

        if starts.is_empty() {
            return Ok(());
        }

        // Insert into search_date with parameter_name='age'
        sqlx::query(
            r#"
            INSERT INTO search_date (
                resource_type, resource_id, version_id, parameter_name,
                start_date, end_date, entry_hash
            )
            SELECT DISTINCT ON (entry_hash)
                $1, $2, $3, $4, t.start_date, t.end_date,
                MD5($1 || $2 || $3::text || $4 || t.start_date::text || t.end_date::text) AS entry_hash
            FROM UNNEST($5::timestamptz[], $6::timestamptz[]) AS t(start_date, end_date)
            ORDER BY entry_hash
            ON CONFLICT (resource_type, resource_id, version_id, parameter_name, entry_hash)
            DO UPDATE SET
                start_date = EXCLUDED.start_date,
                end_date = EXCLUDED.end_date
            "#
        )
        .bind(&resource.resource_type)
        .bind(&resource.id)
        .bind(resource.version_id)
        .bind(&param.code) // "age"
        .bind(&starts)
        .bind(&ends)
        .execute(&mut **tx)
        .await
        .map_err(crate::Error::Database)?;

        Ok(())
    }
}

pub struct AgeQueryHook;

impl QueryHook for AgeQueryHook {
    fn resource_type(&self) -> &'static str {
        "Patient"
    }

    fn parameter_code(&self) -> &'static str {
        "age"
    }

    fn transform(&self, values: &[SearchValue]) -> Option<Vec<ResolvedParam>> {
        let today = Utc::now();
        let current_year = today.year();

        let mut date_conditions = Vec::new();

        for value in values {
            let age: i32 = value.raw.parse().ok()?;
            if age < 0 {
                tracing::warn!("Negative age value: {}", age);
                continue;
            }

            let prefix = value.prefix.unwrap_or(SearchPrefix::Eq);

            // Transform age to birthdate range
            // A person of age X was born in year (current_year - age - 1) to (current_year - age)
            match prefix {
                SearchPrefix::Eq => {
                    // age=34 → birthdate >= 1991-01-01 AND birthdate < 1992-01-01
                    let start_year = current_year - age - 1;
                    let end_year = current_year - age;

                    date_conditions.push(SearchValue {
                        raw: format!("{}-01-01", start_year),
                        prefix: Some(SearchPrefix::Ge),
                    });
                    date_conditions.push(SearchValue {
                        raw: format!("{}-01-01", end_year + 1),
                        prefix: Some(SearchPrefix::Lt),
                    });
                }
                SearchPrefix::Gt => {
                    // age>34 → birthdate < 1992-01-01
                    let year = current_year - age;
                    date_conditions.push(SearchValue {
                        raw: format!("{}-01-01", year),
                        prefix: Some(SearchPrefix::Lt),
                    });
                }
                SearchPrefix::Ge => {
                    // age>=34 → birthdate < 1993-01-01
                    let year = current_year - age + 1;
                    date_conditions.push(SearchValue {
                        raw: format!("{}-01-01", year),
                        prefix: Some(SearchPrefix::Lt),
                    });
                }
                SearchPrefix::Lt => {
                    // age<34 → birthdate >= 1992-01-01
                    let year = current_year - age;
                    date_conditions.push(SearchValue {
                        raw: format!("{}-01-01", year),
                        prefix: Some(SearchPrefix::Ge),
                    });
                }
                SearchPrefix::Le => {
                    // age<=34 → birthdate >= 1991-01-01
                    let year = current_year - age - 1;
                    date_conditions.push(SearchValue {
                        raw: format!("{}-01-01", year),
                        prefix: Some(SearchPrefix::Ge),
                    });
                }
                _ => {
                    tracing::warn!("Unsupported prefix for age: {:?}", prefix);
                    continue;
                }
            }
        }

        if date_conditions.is_empty() {
            return None;
        }

        // Return resolved parameter querying search_date with parameter_name='age'
        Some(vec![ResolvedParam {
            raw_name: "age".to_string(),
            code: "age".to_string(),
            param_type: SearchParamType::Date,
            modifier: None,
            chain: None,
            values: date_conditions,
            composite: None,
            reverse_chain: None,
            chain_metadata: None,
        }])
    }
}
