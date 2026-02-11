//! SQL query builder for FHIR searches.
//!
//! Builds SQL queries from resolved FHIR search parameters, including:
//! - Base resource queries
//! - Search parameter filters (FHIR AND/OR semantics)
//! - Sorting and pagination
//! - Compartment restrictions

use super::parameter_lookup::SearchParamType;
use super::params::{CursorDirection, SearchParameters, SummaryMode};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

mod bind;
mod claueses;
mod filter;

use bind::{push_text, push_text_array};
pub(crate) use claueses::ParsedReferenceQuery;
pub(crate) use filter::{FilterAtom, FilterAtomKind, FilterChainStep, FilterExpr};

pub(crate) fn parse_reference_query_value(
    raw: &str,
    base_url: Option<&str>,
) -> Option<ParsedReferenceQuery> {
    claueses::parse_reference_query_value(raw, base_url)
}

pub(crate) fn hierarchy_parent_param_for_type(resource_type: &str) -> Option<&'static str> {
    match resource_type {
        // Strict hierarchies (circular Reference to same resource type):
        // - Location.partOf
        // - Organization.partOf
        // - Task.partOf
        "Location" | "Organization" => Some("partof"),
        "Task" => Some("part-of"),
        _ => None,
    }
}

/// Bind values for `sqlx` queries.
#[derive(Debug, Clone)]
pub enum BindValue {
    Text(String),
    TextArray(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchPrefix {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Sa,
    Eb,
    Ap,
}

impl SearchPrefix {
    pub fn parse_prefix(value: &str) -> (Option<Self>, &str) {
        // Prefixes only apply when they are immediately at the start of the string.
        let candidates = [
            ("eq", Self::Eq),
            ("ne", Self::Ne),
            ("gt", Self::Gt),
            ("lt", Self::Lt),
            ("ge", Self::Ge),
            ("le", Self::Le),
            ("sa", Self::Sa),
            ("eb", Self::Eb),
            ("ap", Self::Ap),
        ];
        for (s, p) in candidates {
            if let Some(rest) = value.strip_prefix(s) {
                return (Some(p), rest);
            }
        }
        (None, value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchModifier {
    Missing,
    Exact,
    Contains,
    Text,
    Not,
    Below,
    Above,
    Iterate,
    Identifier,
    OfType,
    In,
    NotIn,
    CodeText,
    TextAdvanced,
    /// Dynamic resource type modifier for reference parameters (e.g., :Patient)
    TypeModifier(String),
}

impl SearchModifier {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "missing" => Some(Self::Missing),
            "exact" => Some(Self::Exact),
            "contains" => Some(Self::Contains),
            "text" => Some(Self::Text),
            "not" => Some(Self::Not),
            "below" => Some(Self::Below),
            "above" => Some(Self::Above),
            "iterate" => Some(Self::Iterate),
            "identifier" => Some(Self::Identifier),
            "of-type" => Some(Self::OfType),
            "in" => Some(Self::In),
            "not-in" => Some(Self::NotIn),
            "code-text" => Some(Self::CodeText),
            "text-advanced" => Some(Self::TextAdvanced),
            _ => None,
        }
    }
}

/// Check if a string is a valid FHIR resource type name
pub fn is_valid_resource_type(s: &str) -> bool {
    // FHIR resource types start with uppercase letter and are PascalCase
    // This is a simplified check - in production you might want a complete list
    if s.is_empty() {
        return false;
    }
    let first = s.chars().next().unwrap();
    first.is_ascii_uppercase() && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Validate if a modifier is valid for a given parameter type per FHIR spec
pub fn is_modifier_valid_for_type(param_type: &SearchParamType, modifier: &SearchModifier) -> bool {
    match modifier {
        // :missing is valid for single-element search types (FHIR 3.2.1.5.5.9).
        SearchModifier::Missing => matches!(
            param_type,
            SearchParamType::Date
                | SearchParamType::Number
                | SearchParamType::Quantity
                | SearchParamType::Reference
                | SearchParamType::String
                | SearchParamType::Token
                | SearchParamType::Uri
        ),

        // :exact is valid for string, text, content
        SearchModifier::Exact => matches!(
            param_type,
            SearchParamType::String | SearchParamType::Text | SearchParamType::Content
        ),

        // :contains is valid for string, uri, reference
        SearchModifier::Contains => matches!(
            param_type,
            SearchParamType::String | SearchParamType::Uri | SearchParamType::Reference
        ),

        // :text is valid for string, reference, token
        SearchModifier::Text => matches!(
            param_type,
            SearchParamType::String | SearchParamType::Reference | SearchParamType::Token
        ),

        // :not is valid for token
        SearchModifier::Not => matches!(param_type, SearchParamType::Token),

        // :above and :below are valid for reference, token, uri
        SearchModifier::Above | SearchModifier::Below => matches!(
            param_type,
            SearchParamType::Reference | SearchParamType::Token | SearchParamType::Uri
        ),

        // :identifier is valid for reference only
        SearchModifier::Identifier => matches!(param_type, SearchParamType::Reference),

        // :of-type is valid for token (specifically Identifier type)
        SearchModifier::OfType => matches!(param_type, SearchParamType::Token),

        // :in and :not-in are valid for token
        SearchModifier::In | SearchModifier::NotIn => matches!(param_type, SearchParamType::Token),

        // :code-text is valid for reference, token
        SearchModifier::CodeText => matches!(
            param_type,
            SearchParamType::Reference | SearchParamType::Token
        ),

        // :text-advanced is valid for reference, token
        SearchModifier::TextAdvanced => matches!(
            param_type,
            SearchParamType::Reference | SearchParamType::Token
        ),

        // :iterate is not directly a search parameter modifier (used for _include/_revinclude)
        SearchModifier::Iterate => false,

        // [type] modifier is valid for reference only
        SearchModifier::TypeModifier(_) => matches!(param_type, SearchParamType::Reference),
    }
}

#[derive(Debug, Clone)]
pub struct SearchValue {
    pub raw: String,
    pub prefix: Option<SearchPrefix>,
}

#[derive(Debug, Clone)]
pub struct CompositeParamMeta {
    pub components: Vec<CompositeComponentMeta>,
}

#[derive(Debug, Clone)]
pub struct CompositeComponentMeta {
    pub code: String,
    pub param_type: SearchParamType,
}

/// Metadata for chained parameter search (e.g., subject.name=peter)
#[derive(Debug, Clone)]
pub struct ChainMetadata {
    /// Target resource types the base reference can point to (e.g., ["Patient", "Group"])
    pub target_types: Vec<String>,
    /// Chained parameter code (e.g., "name")
    pub param_code: String,
    /// Chained parameter type (e.g., String)
    pub param_type: SearchParamType,
    /// Optional modifier on the chained parameter
    pub modifier: Option<SearchModifier>,
}

/// Resolved search parameter occurrence for filtering.
#[derive(Debug, Clone)]
pub struct ResolvedParam {
    pub raw_name: String,
    pub code: String,
    pub param_type: SearchParamType,
    pub modifier: Option<SearchModifier>,
    pub chain: Option<String>,
    /// OR values for this occurrence
    pub values: Vec<SearchValue>,
    pub composite: Option<CompositeParamMeta>,
    pub reverse_chain: Option<crate::db::search::params::ReverseChainSpec>,
    /// Metadata for chained parameter (when chain is not _in/_list)
    pub chain_metadata: Option<ChainMetadata>,
}

/// Decode cursor from base64url format: "timestamp,id"
fn decode_cursor(cursor: &str) -> Option<(String, String)> {
    let decoded = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let parts: Vec<&str> = decoded_str.splitn(2, ',').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Encode cursor to base64url format: "timestamp,id"
pub fn encode_cursor(timestamp: &str, id: &str) -> String {
    let raw = format!("{},{}", timestamp, id);
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

/// Query builder for FHIR searches.
#[derive(Debug)]
pub struct QueryBuilder {
    resource_type: Option<String>,
    params: SearchParameters,
    default_count: usize,
    compartment: Option<CompartmentFilter>,
    resolved_params: Vec<ResolvedParam>,
    filter: Option<FilterExpr>,
    resolved_sort: Vec<ResolvedSort>,
    /// Request base URL (scheme://host[/path]) used to resolve local absolute references.
    base_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ResolvedSortKey {
    Id,
    LastUpdated,
    Param {
        code: String,
        param_type: SearchParamType,
        modifier: Option<SearchModifier>,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedSort {
    pub key: ResolvedSortKey,
    pub ascending: bool,
}

/// Compartment filter.
#[derive(Debug, Clone)]
pub struct CompartmentFilter {
    pub compartment_type: String,
    pub compartment_id: String,
    /// Restrict to these resource types (used for compartment search without explicit resource type).
    pub allowed_types: Vec<String>,
    /// Membership parameter names (per CompartmentDefinition).
    pub parameter_names: Vec<String>,
}

impl QueryBuilder {
    pub fn new(resource_type: Option<&str>, params: &SearchParameters) -> Self {
        Self::with_resolved_params(resource_type, params, Vec::new())
    }

    pub fn with_resolved_params(
        resource_type: Option<&str>,
        params: &SearchParameters,
        resolved_params: Vec<ResolvedParam>,
    ) -> Self {
        Self {
            resource_type: resource_type.map(|s| s.to_string()),
            params: params.clone(),
            default_count: 20,
            compartment: None,
            resolved_params,
            filter: None,
            resolved_sort: Vec::new(),
            base_url: None,
        }
    }

    pub fn with_default_count(mut self, default_count: usize) -> Self {
        self.default_count = default_count;
        self
    }

    pub fn new_compartment(
        compartment: CompartmentFilter,
        resource_type: Option<&str>,
        params: &SearchParameters,
        resolved_params: Vec<ResolvedParam>,
    ) -> Self {
        Self {
            resource_type: resource_type.map(|s| s.to_string()),
            params: params.clone(),
            default_count: 20,
            compartment: Some(compartment),
            resolved_params,
            filter: None,
            resolved_sort: Vec::new(),
            base_url: None,
        }
    }

    pub fn with_filter(mut self, filter: Option<FilterExpr>) -> Self {
        self.filter = filter;
        self
    }

    pub fn with_resolved_sort(mut self, resolved_sort: Vec<ResolvedSort>) -> Self {
        self.resolved_sort = resolved_sort;
        self
    }

    pub fn with_base_url(mut self, base_url: Option<&str>) -> Self {
        self.base_url = base_url.map(|s| s.trim_end_matches('/').to_string());
        self
    }

    pub fn build_sql(&self) -> (String, Vec<BindValue>) {
        let mut sql = String::from(
            "SELECT r.resource FROM resources r WHERE r.is_current = true AND r.deleted = false",
        );
        let mut bind_params = Vec::new();

        let searched_type_hint = self.resource_type.as_deref().or_else(|| {
            if self.params.types.len() == 1 {
                Some(self.params.types[0].as_str())
            } else {
                None
            }
        });

        self.push_resource_type_filters(&mut sql, &mut bind_params);
        self.push_compartment_filter(&mut sql, &mut bind_params);

        for resolved in &self.resolved_params {
            let clause = claueses::build_param_clause(
                resolved,
                &mut bind_params,
                self.base_url.as_deref(),
                searched_type_hint,
            );
            if let Some(clause) = clause {
                sql.push_str(" AND ");
                sql.push_str(&clause);
            }
        }

        if let Some(filter) = &self.filter {
            let clause = filter.build_sql(
                &mut bind_params,
                self.base_url.as_deref(),
                searched_type_hint,
                "r",
            );
            sql.push_str(" AND ");
            sql.push_str(&clause);
        }

        // Cursor-based pagination
        if self.params.cursor_direction != CursorDirection::Last {
            if let Some(cursor) = &self.params.cursor {
                if let Some((timestamp, id)) = decode_cursor(cursor) {
                    let ts_idx = push_text(&mut bind_params, timestamp);
                    let id_idx = push_text(&mut bind_params, id);
                    let cmp = match self.params.cursor_direction {
                        CursorDirection::Prev => ">",
                        _ => "<",
                    };
                    sql.push_str(&format!(
                        " AND (r.last_updated, r.id) {} (${}::timestamptz, ${})",
                        cmp, ts_idx, id_idx
                    ));
                }
            }
        }

        self.push_order_by(&mut sql, &mut bind_params);

        // Pagination limit
        sql.push_str(&format!(
            " LIMIT {}",
            self.params.effective_count_with_default(self.default_count)
        ));

        (sql, bind_params)
    }

    pub fn build_count_sql(&self) -> (String, Vec<BindValue>) {
        let mut sql = String::from(
            "SELECT COUNT(*) FROM resources r WHERE r.is_current = true AND r.deleted = false",
        );
        let mut bind_params = Vec::new();

        let searched_type_hint = self.resource_type.as_deref().or_else(|| {
            if self.params.types.len() == 1 {
                Some(self.params.types[0].as_str())
            } else {
                None
            }
        });

        self.push_resource_type_filters(&mut sql, &mut bind_params);
        self.push_compartment_filter(&mut sql, &mut bind_params);

        for resolved in &self.resolved_params {
            let clause = claueses::build_param_clause(
                resolved,
                &mut bind_params,
                self.base_url.as_deref(),
                searched_type_hint,
            );
            if let Some(clause) = clause {
                sql.push_str(" AND ");
                sql.push_str(&clause);
            }
        }

        if let Some(filter) = &self.filter {
            let clause = filter.build_sql(
                &mut bind_params,
                self.base_url.as_deref(),
                searched_type_hint,
                "r",
            );
            sql.push_str(" AND ");
            sql.push_str(&clause);
        }

        (sql, bind_params)
    }

    fn push_resource_type_filters(&self, sql: &mut String, bind_params: &mut Vec<BindValue>) {
        if let Some(ref rt) = self.resource_type {
            let idx = push_text(bind_params, rt.clone());
            sql.push_str(&format!(" AND r.resource_type = ${}", idx));
            return;
        }

        if !self.params.types.is_empty() {
            let idx = push_text_array(bind_params, self.params.types.clone());
            sql.push_str(&format!(" AND r.resource_type = ANY(${})", idx));
        }
    }

    fn push_compartment_filter(&self, sql: &mut String, bind_params: &mut Vec<BindValue>) {
        let Some(comp) = &self.compartment else {
            return;
        };

        if !comp.allowed_types.is_empty() && self.resource_type.is_none() {
            let idx = push_text_array(bind_params, comp.allowed_types.clone());
            sql.push_str(&format!(" AND r.resource_type = ANY(${})", idx));
        }

        // Check if parameter_names contains the special "{def}" value
        let has_def = comp.parameter_names.iter().any(|p| p == "{def}");

        // Filter out "{def}" to get only real search parameter names
        let real_param_names: Vec<String> = comp
            .parameter_names
            .iter()
            .filter(|p| *p != "{def}")
            .cloned()
            .collect();

        let comp_type_idx = push_text(bind_params, comp.compartment_type.clone());
        let comp_id_idx = push_text(bind_params, comp.compartment_id.clone());

        // Build compartment membership condition
        if has_def && !real_param_names.is_empty() {
            // Both {def} and real params: (resource IS compartment) OR (params match)
            let param_names_idx = push_text_array(bind_params, real_param_names);
            sql.push_str(&format!(
                " AND ((r.resource_type = ${} AND r.id = ${}) OR EXISTS (SELECT 1 FROM search_reference sr WHERE sr.resource_type = r.resource_type AND sr.resource_id = r.id AND sr.version_id = r.version_id AND sr.target_type = ${} AND sr.target_id = ${} AND sr.parameter_name = ANY(${})))",
                comp_type_idx, comp_id_idx, comp_type_idx, comp_id_idx, param_names_idx
            ));
        } else if has_def {
            // Only {def}: resource must BE the compartment resource
            sql.push_str(&format!(
                " AND r.resource_type = ${} AND r.id = ${}",
                comp_type_idx, comp_id_idx
            ));
        } else if !real_param_names.is_empty() {
            // Only real params: check search_reference
            let param_names_idx = push_text_array(bind_params, real_param_names);
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM search_reference sr WHERE sr.resource_type = r.resource_type AND sr.resource_id = r.id AND sr.version_id = r.version_id AND sr.target_type = ${} AND sr.target_id = ${} AND sr.parameter_name = ANY(${}))",
                comp_type_idx, comp_id_idx, param_names_idx
            ));
        } else {
            // No membership definition for this compartment/type: return no results rather than
            // accidentally treating this as an unscoped search.
            sql.push_str(" AND 1=0");
        }
    }

    fn push_order_by(&self, sql: &mut String, bind_params: &mut Vec<BindValue>) {
        let mut order_by = Vec::new();
        let reverse_paging = self.params.cursor_direction.is_reverse();

        for s in &self.resolved_sort {
            let dir = if s.ascending ^ reverse_paging {
                "ASC"
            } else {
                "DESC"
            };
            match &s.key {
                ResolvedSortKey::Id => order_by.push(format!("r.id {dir}")),
                ResolvedSortKey::LastUpdated => order_by.push(format!("r.last_updated {dir}")),
                ResolvedSortKey::Param {
                    code,
                    param_type,
                    modifier,
                } => {
                    let name_idx = push_text(bind_params, code.clone());
                    let expr = sort_expr_for_param(
                        param_type.clone(),
                        modifier.as_ref(),
                        name_idx,
                        self.base_url.as_deref(),
                        bind_params,
                    );
                    order_by.push(format!("{expr} {dir} NULLS LAST"));
                }
            }
        }

        if order_by.is_empty() {
            let dir = if reverse_paging { "ASC" } else { "DESC" };
            sql.push_str(&format!(" ORDER BY r.last_updated {dir}, r.id {dir}"));
            return;
        }

        // Ensure deterministic ordering for pagination.
        if !order_by.iter().any(|o| o.contains("r.id")) {
            let dir = if reverse_paging { "ASC" } else { "DESC" };
            order_by.push(format!("r.id {dir}"));
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_by.join(", "));
    }
}

fn sort_expr_for_param(
    param_type: SearchParamType,
    modifier: Option<&SearchModifier>,
    name_idx: usize,
    base_url: Option<&str>,
    bind_params: &mut Vec<BindValue>,
) -> String {
    match param_type {
        SearchParamType::String => format!(
            "(SELECT COALESCE(MIN(NULLIF(ss.value_normalized,'')), MIN(lower(ss.value))) FROM search_string ss WHERE ss.resource_type = r.resource_type AND ss.resource_id = r.id AND ss.version_id = r.version_id AND ss.parameter_name = ${})",
            name_idx
        ),
        SearchParamType::Token => {
            if matches!(modifier, Some(SearchModifier::Text)) {
                format!(
                    "(SELECT COALESCE(MIN(lower(st.display)), MIN(NULLIF(st.code_ci,'')), MIN(lower(st.code))) FROM search_token st WHERE st.resource_type = r.resource_type AND st.resource_id = r.id AND st.version_id = r.version_id AND st.parameter_name = ${})",
                    name_idx
                )
            } else {
                format!(
                    "(SELECT COALESCE(MIN(NULLIF(st.code_ci,'')), MIN(lower(st.code))) FROM search_token st WHERE st.resource_type = r.resource_type AND st.resource_id = r.id AND st.version_id = r.version_id AND st.parameter_name = ${})",
                    name_idx
                )
            }
        }
        SearchParamType::Reference => {
            // Sorts consider local references; external absolute references are treated as missing.
            let local_pred = if let Some(base) = base_url {
                let pattern = format!("{}/%", base.trim_end_matches('/'));
                let idx = push_text(bind_params, pattern);
                format!(
                    "(sr.reference_kind = 'relative' OR (sr.reference_kind = 'absolute' AND sr.target_url LIKE ${}))",
                    idx
                )
            } else {
                "(sr.reference_kind = 'relative')".to_string()
            };
            if matches!(modifier, Some(SearchModifier::Text)) {
                format!(
                    "(SELECT COALESCE(MIN(lower(sr.display)), MIN(lower(sr.target_id))) FROM search_reference sr WHERE sr.resource_type = r.resource_type AND sr.resource_id = r.id AND sr.version_id = r.version_id AND sr.parameter_name = ${} AND {})",
                    name_idx, local_pred
                )
            } else {
                format!(
                    "(SELECT MIN(lower(sr.target_id)) FROM search_reference sr WHERE sr.resource_type = r.resource_type AND sr.resource_id = r.id AND sr.version_id = r.version_id AND sr.parameter_name = ${} AND {})",
                    name_idx, local_pred
                )
            }
        }
        SearchParamType::Date => format!(
            "(SELECT MIN(sd.start_date) FROM search_date sd WHERE sd.resource_type = r.resource_type AND sd.resource_id = r.id AND sd.version_id = r.version_id AND sd.parameter_name = ${})",
            name_idx
        ),
        SearchParamType::Number => format!(
            "(SELECT MIN(sn.value) FROM search_number sn WHERE sn.resource_type = r.resource_type AND sn.resource_id = r.id AND sn.version_id = r.version_id AND sn.parameter_name = ${})",
            name_idx
        ),
        SearchParamType::Quantity => format!(
            "(SELECT MIN(sq.value) FROM search_quantity sq WHERE sq.resource_type = r.resource_type AND sq.resource_id = r.id AND sq.version_id = r.version_id AND sq.parameter_name = ${})",
            name_idx
        ),
        SearchParamType::Uri => format!(
            "(SELECT MIN(su.value) FROM search_uri su WHERE su.resource_type = r.resource_type AND su.resource_id = r.id AND su.version_id = r.version_id AND su.parameter_name = ${})",
            name_idx
        ),
        _ => "NULL".to_string(),
    }
}

pub(crate) use claueses::{parse_composite_tuple, validate_composite_component_value};

/// Convert raw occurrences into `ResolvedParam` values using type information.
///
/// This is used by the search engine when resolving against the database.
pub fn resolve_values_for_type(
    param_type: SearchParamType,
    modifier: Option<&SearchModifier>,
    raw_values: &[String],
) -> Vec<SearchValue> {
    if matches!(modifier, Some(SearchModifier::Missing)) {
        return raw_values
            .first()
            .map(|v| {
                vec![SearchValue {
                    raw: v.clone(),
                    prefix: None,
                }]
            })
            .unwrap_or_default();
    }

    raw_values
        .iter()
        .map(|v| match param_type {
            SearchParamType::Number
            | SearchParamType::Date
            | SearchParamType::Quantity
            | SearchParamType::Special => {
                let (p, rest) = SearchPrefix::parse_prefix(v);
                SearchValue {
                    raw: rest.to_string(),
                    prefix: p,
                }
            }
            _ => SearchValue {
                raw: v.clone(),
                prefix: None,
            },
        })
        .collect()
}

pub fn should_skip_main_query(params: &SearchParameters) -> bool {
    matches!(params.summary, Some(SummaryMode::Count))
}

#[cfg(test)]
mod tests {
    use super::claueses::{parse_reference_query_value, ParsedReferenceQuery};
    use super::*;
    use crate::db::search::params::SearchParameters;

    fn empty_params() -> SearchParameters {
        SearchParameters::from_items(&[]).unwrap()
    }

    fn build_sql(resolved: ResolvedParam, base_url: Option<&str>) -> String {
        build_sql_for_type(Some("Observation"), resolved, base_url)
    }

    fn build_sql_and_binds(
        resolved: ResolvedParam,
        base_url: Option<&str>,
    ) -> (String, Vec<BindValue>) {
        let params = empty_params();
        QueryBuilder::with_resolved_params(Some("Observation"), &params, vec![resolved])
            .with_base_url(base_url)
            .build_sql()
    }

    fn build_sql_for_type(
        resource_type: Option<&str>,
        resolved: ResolvedParam,
        base_url: Option<&str>,
    ) -> String {
        let params = empty_params();
        QueryBuilder::with_resolved_params(resource_type, &params, vec![resolved])
            .with_base_url(base_url)
            .build_sql()
            .0
    }

    #[test]
    fn parse_reference_query_supports_relative_absolute_canonical() {
        let base = Some("http://example.org/fhir");

        match parse_reference_query_value("Patient/123", base).unwrap() {
            ParsedReferenceQuery::Relative { typ, id, version } => {
                assert_eq!(typ.as_deref(), Some("Patient"));
                assert_eq!(id, "123");
                assert_eq!(version, None);
            }
            _ => panic!("expected Relative"),
        }

        match parse_reference_query_value("Patient/123/_history/1", base).unwrap() {
            ParsedReferenceQuery::Relative { typ, id, version } => {
                assert_eq!(typ.as_deref(), Some("Patient"));
                assert_eq!(id, "123");
                assert_eq!(version.as_deref(), Some("1"));
            }
            _ => panic!("expected Relative versioned"),
        }

        match parse_reference_query_value("http://example.org/fhir/Patient/123", base).unwrap() {
            ParsedReferenceQuery::Absolute {
                is_local,
                typ,
                id,
                version,
                ..
            } => {
                assert!(is_local);
                assert_eq!(typ.as_deref(), Some("Patient"));
                assert_eq!(id.as_deref(), Some("123"));
                assert_eq!(version, None);
            }
            _ => panic!("expected Absolute local"),
        }

        match parse_reference_query_value("http://other.org/fhir/Patient/123", base).unwrap() {
            ParsedReferenceQuery::Absolute { is_local, .. } => {
                assert!(!is_local);
            }
            _ => panic!("expected Absolute external"),
        }

        match parse_reference_query_value("http://example.org/canon|1.2.3", base).unwrap() {
            ParsedReferenceQuery::Canonical { url, version } => {
                assert_eq!(url, "http://example.org/canon");
                assert_eq!(version, "1.2.3");
            }
            _ => panic!("expected Canonical"),
        }
    }

    #[test]
    fn absolute_non_versioned_does_not_match_versioned() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "subject".to_string(),
                code: "subject".to_string(),
                param_type: SearchParamType::Reference,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "http://example.org/fhir/Patient/123".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );
        assert!(sql.contains("sp.target_version_id = ''"));
    }

    #[test]
    fn relative_non_versioned_allows_versioned_matches() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "subject".to_string(),
                code: "subject".to_string(),
                param_type: SearchParamType::Reference,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "Patient/123".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );
        assert!(!sql.contains("sp.target_version_id = ''"));
    }

    #[test]
    fn identifier_modifier_uses_token_table() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "subject:identifier".to_string(),
                code: "subject".to_string(),
                param_type: SearchParamType::Reference,
                modifier: Some(SearchModifier::Identifier),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://acme.org/fhir/identifier/mrn|123456".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );
        assert!(sql.contains("FROM search_token st"));
        assert!(sql.contains("st.parameter_name"));
    }

    #[test]
    fn reference_above_uses_recursive_hierarchy_cte() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "location:above".to_string(),
                code: "location".to_string(),
                param_type: SearchParamType::Reference,
                modifier: Some(SearchModifier::Above),
                chain: None,
                values: vec![SearchValue {
                    raw: "Location/A101".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );
        assert!(sql.contains("WITH RECURSIVE hier"));
        assert!(sql.contains("FROM search_reference sr"));
        assert!(sql.contains("sr.parameter_name"));
        assert!(sql.contains("sp.target_id IN"));
    }

    #[test]
    fn reference_below_uses_recursive_hierarchy_cte() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "location:below".to_string(),
                code: "location".to_string(),
                param_type: SearchParamType::Reference,
                modifier: Some(SearchModifier::Below),
                chain: None,
                values: vec![SearchValue {
                    raw: "Location/A".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );
        assert!(sql.contains("WITH RECURSIVE hier"));
        assert!(sql.contains("FROM search_reference sr"));
        assert!(sql.contains("INNER JOIN hier h ON h.id = sr.target_id"));
        assert!(sql.contains("sp.target_id IN"));
    }

    #[test]
    fn reference_contains_uses_ancestor_and_descendant_ctes() {
        let sql = build_sql_for_type(
            Some("Task"),
            ResolvedParam {
                raw_name: "part-of:contains".to_string(),
                code: "part-of".to_string(),
                param_type: SearchParamType::Reference,
                modifier: Some(SearchModifier::Contains),
                chain: None,
                values: vec![SearchValue {
                    raw: "Task/T1A".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );

        assert!(sql.contains("WITH RECURSIVE"));
        assert!(sql.contains("above(id)"));
        assert!(sql.contains("below(id)"));
        assert!(sql.contains("r.id IN"));
    }

    #[test]
    fn reference_canonical_above_uses_version_comparison() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "instantiates-canonical:above".to_string(),
                code: "instantiates-canonical".to_string(),
                param_type: SearchParamType::Reference,
                modifier: Some(SearchModifier::Above),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://example.org/canon|1.2.3".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("sp.reference_kind = 'canonical'"));
        assert!(sql.contains("string_to_array(sp.canonical_version"));
        assert!(sql.contains("sp.canonical_version ~"));
    }

    #[test]
    fn missing_modifier_valid_only_for_allowed_types() {
        assert!(is_modifier_valid_for_type(
            &SearchParamType::String,
            &SearchModifier::Missing
        ));
        assert!(is_modifier_valid_for_type(
            &SearchParamType::Token,
            &SearchModifier::Missing
        ));
        assert!(is_modifier_valid_for_type(
            &SearchParamType::Reference,
            &SearchModifier::Missing
        ));
        assert!(!is_modifier_valid_for_type(
            &SearchParamType::Composite,
            &SearchModifier::Missing
        ));
        assert!(!is_modifier_valid_for_type(
            &SearchParamType::Special,
            &SearchModifier::Missing
        ));
    }

    #[test]
    fn uri_above_uses_segment_boundary_matching() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "url:above".to_string(),
                code: "url".to_string(),
                param_type: SearchParamType::Uri,
                modifier: Some(SearchModifier::Above),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://acme.org/fhir/ValueSet/123/_history/5".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("rtrim(sp.value, '/')"));
        assert!(sql.contains("LIKE rtrim(sp.value, '/') || '/%'"));
    }

    #[test]
    fn uri_below_uses_segment_boundary_matching() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "url:below".to_string(),
                code: "url".to_string(),
                param_type: SearchParamType::Uri,
                modifier: Some(SearchModifier::Below),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://acme.org/fhir".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("rtrim(sp.value, '/')"));
        assert!(sql.contains("rtrim(sp.value, '/') LIKE"));
        assert!(sql.contains("|| '/%'"));
    }

    #[test]
    fn absolute_url_also_matches_canonical_url() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "instantiates-canonical".to_string(),
                code: "instantiates-canonical".to_string(),
                param_type: SearchParamType::Reference,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "http://example.org/canon".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            Some("http://example.org/fhir"),
        );
        assert!(sql.contains("sp.canonical_url"));
    }

    #[test]
    fn string_default_uses_normalized_column() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "given".to_string(),
                code: "given".to_string(),
                param_type: SearchParamType::String,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "Évê".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("sp.value_normalized"));
        assert!(sql.contains("sp.value ILIKE"));
    }

    #[test]
    fn string_contains_uses_normalized_column() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "given:contains".to_string(),
                code: "given".to_string(),
                param_type: SearchParamType::String,
                modifier: Some(SearchModifier::Contains),
                chain: None,
                values: vec![SearchValue {
                    raw: "Évê".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("sp.value_normalized"));
    }

    #[test]
    fn string_exact_uses_raw_value_only() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "given:exact".to_string(),
                code: "given".to_string(),
                param_type: SearchParamType::String,
                modifier: Some(SearchModifier::Exact),
                chain: None,
                values: vec![SearchValue {
                    raw: "Eve".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("sp.value ="));
        assert!(!sql.contains("sp.value_normalized"));
    }

    #[test]
    fn date_eq_uses_containment_semantics() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "date".to_string(),
                code: "date".to_string(),
                param_type: SearchParamType::Date,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "2013".to_string(),
                    prefix: Some(SearchPrefix::Eq),
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("sp.start_date >="));
        assert!(sql.contains("sp.end_date <="));
    }

    #[test]
    fn number_gt_uses_parameter_upper_bound() {
        let (_sql, binds) = build_sql_and_binds(
            ResolvedParam {
                raw_name: "value".to_string(),
                code: "value".to_string(),
                param_type: SearchParamType::Number,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "2.0".to_string(),
                    prefix: Some(SearchPrefix::Gt),
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        let last = binds.last().expect("bind values");
        let BindValue::Text(v) = last else {
            panic!("expected Text bind");
        };
        assert_eq!(v, "2.05");
    }

    #[test]
    fn last_updated_le_uses_range_end_exclusive() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "_lastUpdated".to_string(),
                code: "_lastUpdated".to_string(),
                param_type: SearchParamType::Special,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "2013".to_string(),
                    prefix: Some(SearchPrefix::Le),
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("r.last_updated <"));
        assert!(!sql.contains("r.last_updated <="));
    }

    #[test]
    fn token_default_uses_code_ci_with_fallback() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "gender".to_string(),
                code: "gender".to_string(),
                param_type: SearchParamType::Token,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "MALE".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("sp.code_ci"));
        assert!(sql.contains("ILIKE"));
    }

    #[test]
    fn token_not_uses_not_exists_set_semantics() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "gender:not".to_string(),
                code: "gender".to_string(),
                param_type: SearchParamType::Token,
                modifier: Some(SearchModifier::Not),
                chain: None,
                values: vec![SearchValue {
                    raw: "male".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("NOT EXISTS"));
        assert!(sql.contains("FROM search_token st"));
    }

    #[test]
    fn token_of_type_uses_correlated_identifier_table() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "identifier:of-type".to_string(),
                code: "identifier".to_string(),
                param_type: SearchParamType::Token,
                modifier: Some(SearchModifier::OfType),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://terminology.hl7.org/CodeSystem/v2-0203|MR|446053".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("FROM search_token_identifier si"));
        assert!(sql.contains("si.type_code_ci"));
        assert!(sql.contains("si.value_ci"));
    }

    #[test]
    fn token_in_modifier_uses_valueset_expansion_join() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "code:in".to_string(),
                code: "code".to_string(),
                param_type: SearchParamType::Token,
                modifier: Some(SearchModifier::In),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://example.org/fhir/ValueSet/my-codes".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("valueset_expansions"));
        assert!(sql.contains("valueset_expansion_concepts"));
        assert!(sql.contains("vec.system = sp.system"));
        assert!(sql.contains("vec.code = sp.code"));
        assert!(!sql.contains("NOT EXISTS"));
    }

    #[test]
    fn token_not_in_modifier_uses_not_exists_set_semantics() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "code:not-in".to_string(),
                code: "code".to_string(),
                param_type: SearchParamType::Token,
                modifier: Some(SearchModifier::NotIn),
                chain: None,
                values: vec![SearchValue {
                    raw: "http://example.org/fhir/ValueSet/my-codes".to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("NOT EXISTS"));
        assert!(sql.contains("valueset_expansions"));
        assert!(sql.contains("valueset_expansion_concepts"));
        assert!(sql.contains("FROM search_token sp"));
    }

    #[test]
    fn parse_composite_tuple_respects_escaped_dollar() {
        let parts = parse_composite_tuple("a\\$b$c", 2).unwrap();
        assert_eq!(parts, vec!["a\\$b", "c"]);
    }

    #[test]
    fn composite_param_builds_exists_over_search_composite() {
        let sql = build_sql(
            ResolvedParam {
                raw_name: "code-value-quantity".to_string(),
                code: "code-value-quantity".to_string(),
                param_type: SearchParamType::Composite,
                modifier: None,
                chain: None,
                values: vec![SearchValue {
                    raw: "loinc|12907-2$ge150|http://unitsofmeasure.org|mmol/L".to_string(),
                    prefix: None,
                }],
                composite: Some(CompositeParamMeta {
                    components: vec![
                        CompositeComponentMeta {
                            code: "code".to_string(),
                            param_type: SearchParamType::Token,
                        },
                        CompositeComponentMeta {
                            code: "value-quantity".to_string(),
                            param_type: SearchParamType::Quantity,
                        },
                    ],
                }),
                reverse_chain: None,
                chain_metadata: None,
            },
            None,
        );
        assert!(sql.contains("FROM search_composite sc"));
        assert!(sql.contains("sc.components->0"));
        assert!(sql.contains("sc.components->1"));
    }
}
