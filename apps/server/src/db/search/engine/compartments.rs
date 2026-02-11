use super::{query_builder, SearchEngine};
use crate::Result;
use sqlx::PgConnection;
use std::collections::HashSet;

impl SearchEngine {
    pub(super) async fn load_compartment_filter(
        &self,
        conn: &mut PgConnection,
        compartment_type: &str,
        compartment_id: &str,
        resource_type: Option<&str>,
    ) -> Result<query_builder::CompartmentFilter> {
        if let Some(rt) = resource_type {
            // Single resource type: load parameter names for this specific type
            let row: Option<(Vec<String>,)> = sqlx::query_as(
                r#"
                SELECT parameter_names
                FROM compartment_memberships
                WHERE compartment_type = $1 AND resource_type = $2
                "#,
            )
            .bind(compartment_type)
            .bind(rt)
            .fetch_optional(&mut *conn)
            .await
            .map_err(crate::Error::Database)?;

            let parameter_names = row.map(|(names,)| names).unwrap_or_default();

            return Ok(query_builder::CompartmentFilter {
                compartment_type: compartment_type.to_string(),
                compartment_id: compartment_id.to_string(),
                allowed_types: vec![rt.to_string()],
                parameter_names,
            });
        }

        // All resource types: load all types and collect all unique parameter names
        let rows: Vec<(String, Vec<String>)> = sqlx::query_as(
            r#"
            SELECT resource_type, parameter_names
            FROM compartment_memberships
            WHERE compartment_type = $1
            "#,
        )
        .bind(compartment_type)
        .fetch_all(&mut *conn)
        .await
        .map_err(crate::Error::Database)?;

        let mut allowed_types = Vec::new();
        let mut seen_params = HashSet::new();

        for (rt, params) in rows {
            allowed_types.push(rt);
            seen_params.extend(params);
        }

        let parameter_names: Vec<String> = seen_params.into_iter().collect();

        Ok(query_builder::CompartmentFilter {
            compartment_type: compartment_type.to_string(),
            compartment_id: compartment_id.to_string(),
            allowed_types,
            parameter_names,
        })
    }
}
