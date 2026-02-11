use super::{query_builder, QueryBuilder, SearchEngine, SearchParameters};
use crate::db::search::parameter_lookup::SearchParamCache;
use crate::runtime_config::ConfigKey;
use crate::services::search::SearchResult;
use crate::Result;
use sqlx::PgConnection;
use sqlx::PgPool;
use std::sync::Arc;

impl SearchEngine {
    /// Create a new search engine.
    pub fn new(db_pool: PgPool, search_config: crate::config::FhirSearchConfig) -> Self {
        let param_cache = Arc::new(SearchParamCache::new(db_pool.clone()));
        Self {
            db_pool,
            param_cache,
            computed_hooks: crate::hooks::computed::HookRegistry::new(),
            enable_text_search: search_config.enable_text,
            enable_content_search: search_config.enable_content,
            runtime_config_cache: None,
            search_config,
        }
    }

    pub fn new_with_runtime_config(
        db_pool: PgPool,
        search_config: crate::config::FhirSearchConfig,
        runtime_config_cache: Arc<crate::runtime_config::RuntimeConfigCache>,
    ) -> Self {
        let mut engine = Self::new(db_pool, search_config);
        engine.runtime_config_cache = Some(runtime_config_cache);
        engine
    }

    /// Clear cached search parameter definitions.
    pub fn invalidate_param_cache(&self) {
        self.param_cache.invalidate();
    }

    /// Search for resources.
    ///
    /// - If resource_type is Some, searches only that type
    /// - If resource_type is None, searches across types specified in _type parameter
    pub async fn search(
        &self,
        resource_type: Option<&str>,
        params: &SearchParameters,
        base_url: Option<&str>,
    ) -> Result<SearchResult> {
        let mut conn = self
            .db_pool
            .acquire()
            .await
            .map_err(crate::Error::Database)?;
        self.search_with_connection(&mut conn, resource_type, params, base_url)
            .await
    }

    /// Search using an existing DB connection (e.g. a transaction connection).
    pub async fn search_with_connection(
        &self,
        conn: &mut PgConnection,
        resource_type: Option<&str>,
        params: &SearchParameters,
        base_url: Option<&str>,
    ) -> Result<SearchResult> {
        let (max_count, max_total_results, max_include_depth, max_includes, default_count) =
            if let Some(cache) = &self.runtime_config_cache {
                let max_count: usize = cache.get(ConfigKey::SearchMaxCount).await;
                let max_total_results: usize = cache.get(ConfigKey::SearchMaxTotalResults).await;
                let max_include_depth: usize = cache.get(ConfigKey::SearchMaxIncludeDepth).await;
                let max_includes: usize = cache.get(ConfigKey::SearchMaxIncludes).await;
                let default_count: usize = cache.get(ConfigKey::SearchDefaultCount).await;
                (
                    max_count,
                    max_total_results,
                    max_include_depth,
                    max_includes,
                    default_count,
                )
            } else {
                (
                    self.search_config.max_count,
                    self.search_config.max_total_results,
                    self.search_config.max_include_depth,
                    self.search_config.max_includes,
                    self.search_config.default_count,
                )
            };

        // Validate search parameters against configured limits
        params.validate_limits(
            max_count,
            max_total_results,
            max_include_depth,
            max_includes,
        )?;

        // Resolve search parameters to their types
        let (mut resolved_params, mut resolved_filter, unknown_params) =
            if let Some(rt) = resource_type {
                self.resolve_search_params_type(conn, rt, params).await?
            } else {
                self.resolve_search_params_system(conn, params).await?
            };

        let searched_type_hint = resource_type.or_else(|| {
            if params.types.len() == 1 {
                Some(params.types[0].as_str())
            } else {
                None
            }
        });

        self.normalize_search_params(conn, &mut resolved_params, base_url, searched_type_hint)
            .await?;

        if let Some(f) = resolved_filter.as_mut() {
            self.normalize_filter_expr(conn, f, base_url, searched_type_hint)
                .await?;
        }

        let resolved_sort = self
            .resolve_sort_params(conn, resource_type, params)
            .await?;

        // Skip fetching resources for `_summary=count` mode.
        let should_fetch_resources = !query_builder::should_skip_main_query(params);

        let mut resources = if should_fetch_resources {
            let query = query_builder::QueryBuilder::with_resolved_params(
                resource_type,
                params,
                resolved_params.clone(),
            )
            .with_filter(resolved_filter.clone())
            .with_resolved_sort(resolved_sort.clone())
            .with_base_url(base_url)
            .with_default_count(default_count);
            self.execute_search(conn, query).await?
        } else {
            Vec::new()
        };
        if params.cursor_direction.is_reverse() {
            resources.reverse();
        }

        // Handle _include and _revinclude (skip for summary=count)
        let included = if should_fetch_resources && params.has_includes() {
            self.fetch_includes(conn, &resources, params).await?
        } else {
            Vec::new()
        };

        // Calculate total if requested
        let total = if params.should_calculate_total() {
            let query = query_builder::QueryBuilder::with_resolved_params(
                resource_type,
                params,
                resolved_params,
            )
            .with_filter(resolved_filter)
            .with_resolved_sort(resolved_sort)
            .with_base_url(base_url)
            .with_default_count(default_count);
            Some(self.count_total(conn, query).await?)
        } else {
            None
        };

        Ok(SearchResult {
            resources,
            total,
            included,
            unknown_params,
        })
    }

    /// Search within a compartment.
    ///
    /// Restricts search to resources accessible within the specified compartment
    pub async fn search_compartment(
        &self,
        compartment_type: &str,
        compartment_id: &str,
        resource_type: Option<&str>,
        params: &SearchParameters,
        base_url: Option<&str>,
    ) -> Result<SearchResult> {
        let mut conn = self
            .db_pool
            .acquire()
            .await
            .map_err(crate::Error::Database)?;
        self.search_compartment_with_connection(
            &mut conn,
            compartment_type,
            compartment_id,
            resource_type,
            params,
            base_url,
        )
        .await
    }

    pub async fn search_compartment_with_connection(
        &self,
        conn: &mut PgConnection,
        compartment_type: &str,
        compartment_id: &str,
        resource_type: Option<&str>,
        params: &SearchParameters,
        base_url: Option<&str>,
    ) -> Result<SearchResult> {
        let (max_count, max_total_results, max_include_depth, max_includes, default_count) =
            if let Some(cache) = &self.runtime_config_cache {
                let max_count: usize = cache.get(ConfigKey::SearchMaxCount).await;
                let max_total_results: usize = cache.get(ConfigKey::SearchMaxTotalResults).await;
                let max_include_depth: usize = cache.get(ConfigKey::SearchMaxIncludeDepth).await;
                let max_includes: usize = cache.get(ConfigKey::SearchMaxIncludes).await;
                let default_count: usize = cache.get(ConfigKey::SearchDefaultCount).await;
                (
                    max_count,
                    max_total_results,
                    max_include_depth,
                    max_includes,
                    default_count,
                )
            } else {
                (
                    self.search_config.max_count,
                    self.search_config.max_total_results,
                    self.search_config.max_include_depth,
                    self.search_config.max_includes,
                    self.search_config.default_count,
                )
            };

        // Validate search parameters against configured limits
        params.validate_limits(
            max_count,
            max_total_results,
            max_include_depth,
            max_includes,
        )?;

        // Validate that the compartment resource exists
        // Per FHIR spec, compartment searches should return empty if compartment doesn't exist
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM resources WHERE resource_type = $1 AND id = $2 AND NOT deleted)"
        )
        .bind(compartment_type)
        .bind(compartment_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(crate::Error::Database)?;

        if !exists {
            // Return empty search result (per spec: same as if compartment has no resources)
            return Ok(SearchResult {
                resources: Vec::new(),
                total: Some(0),
                included: Vec::new(),
                unknown_params: Vec::new(),
            });
        }

        // Resolve search parameters to their types (for tracking unknown params)
        let (mut resolved_params, mut resolved_filter, unknown_params) =
            if let Some(rt) = resource_type {
                self.resolve_search_params_type(conn, rt, params).await?
            } else {
                self.resolve_search_params_system(conn, params).await?
            };

        let searched_type_hint = resource_type.or_else(|| {
            if params.types.len() == 1 {
                Some(params.types[0].as_str())
            } else {
                None
            }
        });

        self.normalize_search_params(conn, &mut resolved_params, base_url, searched_type_hint)
            .await?;

        if let Some(f) = resolved_filter.as_mut() {
            self.normalize_filter_expr(conn, f, base_url, searched_type_hint)
                .await?;
        }

        let resolved_sort = self
            .resolve_sort_params(conn, resource_type, params)
            .await?;

        let compartment = self
            .load_compartment_filter(conn, compartment_type, compartment_id, resource_type)
            .await?;

        let should_fetch_resources = !query_builder::should_skip_main_query(params);

        let resources = if should_fetch_resources {
            let query = QueryBuilder::new_compartment(
                compartment.clone(),
                resource_type,
                params,
                resolved_params.clone(),
            )
            .with_filter(resolved_filter.clone())
            .with_resolved_sort(resolved_sort.clone())
            .with_base_url(base_url)
            .with_default_count(default_count);
            self.execute_search(conn, query).await?
        } else {
            Vec::new()
        };

        let included = if should_fetch_resources && params.has_includes() {
            self.fetch_includes(conn, &resources, params).await?
        } else {
            Vec::new()
        };

        let total = if params.should_calculate_total() {
            let query =
                QueryBuilder::new_compartment(compartment, resource_type, params, resolved_params)
                    .with_filter(resolved_filter)
                    .with_resolved_sort(resolved_sort)
                    .with_base_url(base_url)
                    .with_default_count(default_count);
            Some(self.count_total(conn, query).await?)
        } else {
            None
        };

        Ok(SearchResult {
            resources,
            total,
            included,
            unknown_params,
        })
    }
}
