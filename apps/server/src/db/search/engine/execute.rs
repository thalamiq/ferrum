use super::{query_builder, JsonValue, QueryBuilder, SearchEngine};
use crate::Result;
use sqlx::PgConnection;

impl SearchEngine {
    /// Execute search query.
    pub(super) async fn execute_search(
        &self,
        conn: &mut PgConnection,
        query: QueryBuilder,
    ) -> Result<Vec<JsonValue>> {
        let (sql, bind_values) = query.build_sql();

        let mut query_builder = sqlx::query(&sql);
        for value in bind_values {
            query_builder = match value {
                query_builder::BindValue::Text(v) => query_builder.bind(v),
                query_builder::BindValue::TextArray(vs) => query_builder.bind(vs),
            };
        }

        let rows = query_builder
            .fetch_all(&mut *conn)
            .await
            .map_err(crate::Error::Database)?;

        use sqlx::Row;
        let resources: Vec<JsonValue> = rows
            .iter()
            .filter_map(|row| row.try_get::<JsonValue, _>("resource").ok())
            .collect();

        Ok(resources)
    }

    pub(super) async fn count_total(
        &self,
        conn: &mut PgConnection,
        query: QueryBuilder,
    ) -> Result<i64> {
        let (sql, bind_values) = query.build_count_sql();

        let mut query_builder = sqlx::query_scalar::<_, i64>(&sql);
        for value in bind_values {
            query_builder = match value {
                query_builder::BindValue::Text(v) => query_builder.bind(v),
                query_builder::BindValue::TextArray(vs) => query_builder.bind(vs),
            };
        }

        let total = query_builder
            .fetch_one(&mut *conn)
            .await
            .map_err(crate::Error::Database)?;

        Ok(total)
    }
}
