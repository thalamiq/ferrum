//! Search parameter parsing and validation
//!
//! Handles parsing of FHIR search parameters including:
//! - Common parameters (_count, _offset, _sort, _total, _include, _revinclude)
//! - System search type selection (`_type`)
//! - Resource-specific search parameters including modifiers and chaining

use crate::Result;
use std::collections::HashMap;

use super::escape::split_unescaped;

/// Parsed search parameters and controls.
#[derive(Debug, Clone)]
pub struct SearchParameters {
    /// Resource-specific search parameters in request order.
    ///
    /// FHIR semantics:
    /// - Repeating the same parameter name is AND (e.g., `name=John&name=Smith`)
    /// - Comma-separated values inside a single parameter instance are OR (e.g., `name=John,Smith`)
    pub resource_params: Vec<RawSearchParam>,

    /// System-level search: resource types to include (from `_type`)
    pub types: Vec<String>,

    /// Number of results to return (default: 20)
    pub count: Option<usize>,

    /// Offset for pagination (deprecated, use cursor instead)
    pub offset: Option<usize>,

    /// Cursor for keyset pagination (base64url encoded: "timestamp,id")
    pub cursor: Option<String>,
    /// Cursor direction for keyset pagination
    pub cursor_direction: CursorDirection,

    /// Maximum total number of match resources to return across all pages
    /// Per FHIR spec 3.2.1.7.4: hint to server, only affects entry.search.mode=match
    pub max_results: Option<usize>,

    /// Sort specification (e.g., "name", "-birthdate")
    pub sort: Vec<SortParam>,

    /// How to calculate total count (none, estimate, accurate)
    pub total: TotalMode,

    /// Resources to include (_include)
    pub include: Vec<IncludeParam>,

    /// Resources to reverse include (_revinclude)
    pub revinclude: Vec<IncludeParam>,

    /// Summary mode (true, text, data, count, false)
    pub summary: Option<SummaryMode>,

    /// Elements to include in response
    pub elements: Vec<String>,

    /// Pretty print output (FHIR `_pretty`)
    pub pretty: Option<bool>,
}

/// Reverse chaining specification for _has parameter
#[derive(Debug, Clone)]
pub struct ReverseChainSpec {
    /// Resource type that refers to the searched resource (e.g., "Observation")
    pub referring_resource: String,
    /// Search parameter on referring resource that points back (e.g., "patient")
    pub referring_param: String,
    /// Filter parameter on referring resource (e.g., "code")
    pub filter_param: String,
}

/// Raw search parameter occurrence from the request.
#[derive(Debug, Clone)]
pub struct RawSearchParam {
    /// Original parameter name as provided by the client (may include modifiers/chains).
    pub raw_name: String,
    /// Base search parameter code (e.g., `name`, `birthdate`, `subject`).
    pub code: String,
    /// Optional modifier (e.g., `exact`, `contains`, `missing`).
    pub modifier: Option<String>,
    /// Optional chain specifier (e.g., `Patient.name`).
    pub chain: Option<String>,
    /// Reverse chaining specification for _has parameter
    pub reverse_chain: Option<ReverseChainSpec>,
    /// Original raw value (decoded by the HTTP layer).
    pub raw_value: String,
    /// OR values for this single occurrence (split on commas).
    pub or_values: Vec<String>,
}

/// Sort parameter
#[derive(Debug, Clone)]
pub struct SortParam {
    /// Parameter name to sort by
    pub param: String,
    /// Sort direction (true = ascending, false = descending)
    pub ascending: bool,
    /// Optional sort modifier (e.g. `text` in `_sort=code:text`)
    pub modifier: Option<String>,
}

/// Include parameter (_include or _revinclude)
#[derive(Debug, Clone)]
pub struct IncludeParam {
    /// Source resource type (`*` allowed)
    pub source_type: String,
    /// Search parameter name (`*` allowed)
    pub param: String,
    /// Target resource type (optional)
    pub target_type: Option<String>,
    /// Recursive include modifier (`:iterate`)
    pub iterate: bool,
}

/// Total count mode
#[derive(Debug, Clone, PartialEq)]
pub enum TotalMode {
    /// Don't include total
    None,
    /// Estimate total (fast)
    Estimate,
    /// Calculate accurate total
    Accurate,
}

/// Summary mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SummaryMode {
    True,
    Text,
    Data,
    Count,
    False,
}

/// Cursor direction for keyset pagination
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorDirection {
    Next,
    Prev,
    Last,
}

impl CursorDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "next" => Some(Self::Next),
            "prev" | "previous" => Some(Self::Prev),
            "last" => Some(Self::Last),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Prev => "prev",
            Self::Last => "last",
        }
    }

    pub fn is_reverse(self) -> bool {
        matches!(self, Self::Prev | Self::Last)
    }
}

impl SearchParameters {
    /// Parse search parameters from query string parameters.
    ///
    /// Note: a `HashMap` cannot preserve parameter occurrence boundaries; use
    /// `from_items` for spec-correct AND/OR semantics.
    pub fn from_params(params: &HashMap<String, Vec<String>>) -> Result<Self> {
        let mut items = Vec::new();
        for (k, vs) in params {
            for v in vs {
                items.push((k.clone(), v.clone()));
            }
        }
        Self::from_items(&items)
    }

    /// Parse search parameters from ordered (key, value) items.
    pub fn from_items(items: &[(String, String)]) -> Result<Self> {
        let mut resource_params = Vec::new();
        let mut types = Vec::new();
        let mut count = None;
        let mut offset = None;
        let mut cursor = None;
        let mut cursor_direction = CursorDirection::Next;
        let mut cursor_direction_set = false;
        let mut max_results = None;
        let mut sort = Vec::new();
        let mut total = TotalMode::Accurate;
        let mut include = Vec::new();
        let mut revinclude = Vec::new();
        let mut summary = None;
        let mut elements = Vec::new();
        let mut pretty = None;

        for (key, value) in items {
            match key.as_str() {
                "_count" => {
                    let parsed: usize = value.parse().map_err(|_| {
                        crate::Error::Validation(format!("Invalid _count value: {}", value))
                    })?;

                    // Per FHIR spec 3.2.1.7.3: _count=0 SHALL be treated as _summary=count
                    if parsed == 0 {
                        summary = Some(SummaryMode::Count);
                    }

                    count = Some(parsed);
                }
                "_offset" => {
                    let parsed: usize = value.parse().map_err(|_| {
                        crate::Error::Validation(format!("Invalid _offset value: {}", value))
                    })?;
                    offset = Some(parsed);
                }
                "_cursor" => {
                    cursor = Some(value.clone());
                }
                "_cursor_direction" => {
                    if cursor_direction_set {
                        return Err(crate::Error::Validation(
                            "Search result parameter '_cursor_direction' must not appear more than once"
                                .to_string(),
                        ));
                    }
                    cursor_direction_set = true;
                    cursor_direction = CursorDirection::parse(value).ok_or_else(|| {
                        crate::Error::Validation(format!(
                            "Invalid _cursor_direction value: {}",
                            value
                        ))
                    })?;
                }
                "_maxresults" => {
                    let parsed: usize = value.parse().map_err(|_| {
                        crate::Error::Validation(format!("Invalid _maxresults value: {}", value))
                    })?;
                    max_results = Some(parsed);
                }
                "_sort" => {
                    if !sort.is_empty() {
                        // Per spec (3.2.1.7), result parameters (except _include/_revinclude)
                        // SHOULD only appear once; we treat repeats as an error.
                        return Err(crate::Error::Validation(
                            "Search result parameter '_sort' must not appear more than once"
                                .to_string(),
                        ));
                    }
                    sort = Self::parse_sort(value)?;
                }
                "_total" => {
                    total = match value.as_str() {
                        "none" => TotalMode::None,
                        "estimate" => TotalMode::Estimate,
                        "accurate" => TotalMode::Accurate,
                        _ => {
                            return Err(crate::Error::Validation(format!(
                                "Invalid _total value: {}",
                                value
                            )));
                        }
                    };
                }
                "_include" | "_include:iterate" => {
                    let iterate_key = key.as_str().ends_with(":iterate");
                    for include_str in split_unescaped(value, ',') {
                        if let Some(param) = Self::parse_include(include_str.trim()) {
                            let mut param = param;
                            if iterate_key {
                                param.iterate = true;
                            }
                            include.push(param);
                        }
                    }
                }
                "_revinclude" | "_revinclude:iterate" => {
                    let iterate_key = key.as_str().ends_with(":iterate");
                    for include_str in split_unescaped(value, ',') {
                        if let Some(param) = Self::parse_include(include_str.trim()) {
                            let mut param = param;
                            if iterate_key {
                                param.iterate = true;
                            }
                            revinclude.push(param);
                        }
                    }
                }
                k if k.starts_with("_include:") => {
                    return Err(crate::Error::Validation(format!(
                        "Unsupported _include modifier: {}",
                        k
                    )));
                }
                k if k.starts_with("_revinclude:") => {
                    return Err(crate::Error::Validation(format!(
                        "Unsupported _revinclude modifier: {}",
                        k
                    )));
                }
                "_summary" => {
                    summary = Some(match value.as_str() {
                        "true" => SummaryMode::True,
                        "text" => SummaryMode::Text,
                        "data" => SummaryMode::Data,
                        "count" => SummaryMode::Count,
                        "false" => SummaryMode::False,
                        _ => {
                            return Err(crate::Error::Validation(format!(
                                "Invalid _summary value: {}",
                                value
                            )));
                        }
                    });
                }
                "_elements" => {
                    elements.extend(
                        split_unescaped(value, ',')
                            .into_iter()
                            .map(|s| s.trim().to_string()),
                    );
                }
                "_pretty" => {
                    let parsed: bool = value.parse().map_err(|_| {
                        crate::Error::Validation(format!("Invalid _pretty value: {}", value))
                    })?;
                    pretty = Some(parsed);
                }
                "_format" => {
                    // Result parameter used for content negotiation (handled at the HTTP layer).
                }
                "_filter" => {
                    // `_filter` values are expressions and must not be split on commas.
                    // See FHIR R5 3.2.3.
                    resource_params.push(RawSearchParam {
                        raw_name: key.clone(),
                        code: "_filter".to_string(),
                        modifier: None,
                        chain: None,
                        reverse_chain: None,
                        raw_value: value.clone(),
                        or_values: vec![value.clone()],
                    });
                }
                k if k.starts_with("_filter:") => {
                    return Err(crate::Error::Validation(format!(
                        "Unsupported _filter modifier: {}",
                        k
                    )));
                }
                "_type" => {
                    types.extend(
                        split_unescaped(value, ',')
                            .into_iter()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()),
                    );
                }
                _ => {
                    // Resource-specific search parameter occurrence
                    let (code, modifier, chain, reverse_chain) = parse_parameter_name(key);
                    let or_values = split_unescaped(value, ',')
                        .into_iter()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>();

                    resource_params.push(RawSearchParam {
                        raw_name: key.clone(),
                        code,
                        modifier,
                        chain,
                        reverse_chain,
                        raw_value: value.clone(),
                        or_values,
                    });
                }
            }
        }

        Ok(Self {
            resource_params,
            types,
            count,
            offset,
            cursor,
            cursor_direction,
            max_results,
            sort,
            total,
            include,
            revinclude,
            summary,
            elements,
            pretty,
        })
    }

    /// Parse sort parameter (e.g., "name" or "-birthdate")
    fn parse_sort(value: &str) -> Result<Vec<SortParam>> {
        let mut out = Vec::new();
        for raw in split_unescaped(value, ',') {
            let mut s = raw.trim();
            if s.is_empty() {
                continue;
            }

            let mut ascending = true;
            if let Some(rest) = s.strip_prefix('-') {
                ascending = false;
                s = rest;
            }

            let parts: Vec<&str> = s.split(':').collect();
            let param = parts.first().copied().unwrap_or("").trim();
            if param.is_empty() {
                continue;
            }

            let mut modifier: Option<String> = None;
            let mut tail_dir: Option<&str> = None;
            if parts.len() >= 2 {
                let last = parts[parts.len() - 1].trim();
                if last.eq_ignore_ascii_case("asc") || last.eq_ignore_ascii_case("desc") {
                    tail_dir = Some(last);
                }
            }
            if let Some(dir) = tail_dir {
                ascending = dir.eq_ignore_ascii_case("asc");
            }

            // Any remaining single suffix (besides asc/desc) is treated as a sort modifier.
            let modifier_part_count = parts.len().saturating_sub(1 + tail_dir.is_some() as usize);
            if modifier_part_count > 1 {
                return Err(crate::Error::Validation(format!(
                    "Invalid _sort value (too many ':' segments): {}",
                    raw
                )));
            }
            if modifier_part_count == 1 {
                let m = parts[1].trim();
                if !m.is_empty()
                    && !m.eq_ignore_ascii_case("asc")
                    && !m.eq_ignore_ascii_case("desc")
                {
                    modifier = Some(m.to_ascii_lowercase());
                }
            }

            out.push(SortParam {
                param: param.to_string(),
                ascending,
                modifier,
            });
        }
        Ok(out)
    }

    /// Parse include parameter.
    ///
    /// Supported formats:
    /// - `*`
    /// - `[source]:*`
    /// - `[source]:[param]`
    /// - `[source]:[param]:[target]`
    /// - ... with optional `:iterate` suffix
    fn parse_include(value: &str) -> Option<IncludeParam> {
        if value.is_empty() {
            return None;
        }

        let mut value = value.trim().to_string();
        let mut iterate = false;
        if let Some(stripped) = value.strip_suffix(":iterate") {
            iterate = true;
            value = stripped.trim_end_matches(':').to_string();
        }

        if value == "*" {
            return Some(IncludeParam {
                source_type: "*".to_string(),
                param: "*".to_string(),
                target_type: None,
                iterate,
            });
        }

        let parts: Vec<&str> = value.split(':').collect();
        match parts.len() {
            2 => Some(IncludeParam {
                source_type: parts[0].to_string(),
                param: parts[1].to_string(),
                target_type: None,
                iterate,
            }),
            3 => Some(IncludeParam {
                source_type: parts[0].to_string(),
                param: parts[1].to_string(),
                target_type: Some(parts[2].to_string()).filter(|s| !s.is_empty()),
                iterate,
            }),
            _ => None,
        }
    }

    /// Check if any includes are requested
    pub fn has_includes(&self) -> bool {
        !self.include.is_empty() || !self.revinclude.is_empty()
    }

    /// Check if total should be calculated
    pub fn should_calculate_total(&self) -> bool {
        self.total != TotalMode::None || matches!(self.summary, Some(SummaryMode::Count))
    }

    /// Get effective count (with default and _maxresults cap)
    /// Per FHIR spec: _maxresults caps the number of match resources returned
    /// Note: The default_count should be passed from the search config
    pub fn effective_count(&self) -> usize {
        self.effective_count_with_default(20)
    }

    /// Get effective count with configurable default
    pub fn effective_count_with_default(&self, default_count: usize) -> usize {
        let base_count = self.count.unwrap_or(default_count);

        // Apply _maxresults cap if present
        if let Some(max) = self.max_results {
            base_count.min(max)
        } else {
            base_count
        }
    }

    /// Get effective offset (with default)
    pub fn effective_offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }

    /// Validate search parameters against configured limits
    pub fn validate_limits(
        &self,
        max_count: usize,
        max_total_results: usize,
        max_include_depth: usize,
        max_includes: usize,
    ) -> crate::Result<()> {
        // Check _count limit
        if let Some(count) = self.count {
            if count > max_count {
                return Err(crate::Error::TooCostly(format!(
                    "_count={} exceeds maximum allowed count of {}. Use pagination or reduce _count.",
                    count, max_count
                )));
            }
        }

        // Check _maxresults limit
        if let Some(max_results) = self.max_results {
            if max_results > max_total_results {
                return Err(crate::Error::TooCostly(format!(
                    "_maxresults={} exceeds maximum allowed total results of {}",
                    max_results, max_total_results
                )));
            }
        }

        // Check include/revinclude count
        let total_includes = self.include.len() + self.revinclude.len();
        if total_includes > max_includes {
            return Err(crate::Error::TooCostly(format!(
                "Total number of _include and _revinclude parameters ({}) exceeds maximum of {}",
                total_includes, max_includes
            )));
        }

        // Check iterate depth (prevent infinite recursion)
        let max_depth_includes = self
            .include
            .iter()
            .chain(self.revinclude.iter())
            .filter(|inc| inc.iterate)
            .count();

        if max_depth_includes > max_include_depth {
            return Err(crate::Error::TooCostly(format!(
                "Number of :iterate includes ({}) exceeds maximum depth of {}. This prevents potential infinite recursion.",
                max_depth_includes, max_include_depth
            )));
        }

        Ok(())
    }
}

fn parse_parameter_name(
    param_name: &str,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<ReverseChainSpec>,
) {
    // Mirrors the Python implementation semantics in `old-fhir-server/app/db/search/query_parser.py`.
    // This is intentionally permissive; type-based validation happens later.

    // Handle chaining syntax (FHIR 3.2.1.6.5):
    // Standard chaining: `subject.name=peter`, `subject:Patient.name=peter`
    // Membership chaining: `patient._in=Group/104`, `ingredient._in:not=List/105`
    if let Some((base_with_modifier, chain_part)) = param_name.split_once('.') {
        // Check for membership chaining which has special modifier handling
        if chain_part.starts_with("_in") || chain_part.starts_with("_list") {
            // Membership chaining: parse modifier from chain part
            let parts: Vec<&str> = chain_part.split(':').collect();
            let chain = Some(parts[0].to_string());
            let modifier = parts.get(1).map(|s| s.to_string());

            // For membership chaining, base should not have type modifier
            if base_with_modifier.contains(':') {
                // If there's a colon in base for membership chaining, treat whole thing as unknown
                // by falling through to standard colon parsing
            } else {
                return (base_with_modifier.to_string(), modifier, chain, None);
            }
        } else {
            // Standard chaining: parse type modifier from base
            let (base, modifier) = if let Some((b, m)) = base_with_modifier.split_once(':') {
                (b.to_string(), Some(m.to_string()))
            } else {
                (base_with_modifier.to_string(), None)
            };

            // Chain part is everything after the first dot
            let chain = Some(chain_part.to_string());
            return (base, modifier, chain, None);
        }
    }

    let parts: Vec<&str> = param_name.split(':').collect();
    let base_name = parts[0].to_string();

    if parts.len() == 1 {
        return (base_name, None, None, None);
    }

    // Check for _has reverse chaining: _has:<referring_resource>:<referring_param>:<filter_param>
    if base_name == "_has" && parts.len() == 4 {
        let reverse_chain = ReverseChainSpec {
            referring_resource: parts[1].to_string(),
            referring_param: parts[2].to_string(),
            filter_param: parts[3].to_string(),
        };
        return ("_has".to_string(), None, None, Some(reverse_chain));
    }

    // Known modifier names (lowercase).
    const MODIFIERS: &[&str] = &[
        "missing",
        "exact",
        "contains",
        "text",
        "not",
        "code-text",
        "text-advanced",
        "in",
        "below",
        "above",
        "not-in",
        "of-type",
        "identifier",
        "iterate",
    ];

    let is_modifier = |s: &str| MODIFIERS.contains(&s.to_ascii_lowercase().as_str());
    let looks_like_chain = |s: &str| s.contains('.');
    let looks_like_type_modifier = |s: &str| {
        // FHIR resource types start with uppercase letter and are PascalCase.
        if s.is_empty() {
            return false;
        }
        let first = s.chars().next().unwrap_or('a');
        first.is_ascii_uppercase() && s.chars().all(|c| c.is_ascii_alphanumeric())
    };

    let mut modifier: Option<String> = None;
    let mut chain: Option<String> = None;

    match parts.len() {
        2 => {
            if is_modifier(parts[1]) {
                modifier = Some(parts[1].to_ascii_lowercase());
            } else if looks_like_type_modifier(parts[1]) && !looks_like_chain(parts[1]) {
                // Reference [type] modifier, e.g. `subject:Patient=23`.
                modifier = Some(parts[1].to_string());
            } else {
                chain = Some(parts[1].to_string());
            }
        }
        3 => {
            if is_modifier(parts[1]) {
                modifier = Some(parts[1].to_ascii_lowercase());
                chain = Some(parts[2].to_string());
            } else if is_modifier(parts[2]) {
                chain = Some(parts[1].to_string());
                modifier = Some(parts[2].to_ascii_lowercase());
            } else {
                chain = Some(parts[1..].join(":"));
            }
        }
        _ => {
            if is_modifier(parts[parts.len() - 1]) {
                modifier = Some(parts[parts.len() - 1].to_ascii_lowercase());
                chain = Some(parts[1..parts.len() - 1].join(":"));
            } else {
                chain = Some(parts[1..].join(":"));
            }
        }
    }

    (base_name, modifier, chain, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_items_preserves_and_or_semantics() {
        let items = vec![
            ("name".to_string(), "John,Smith".to_string()),
            ("name".to_string(), "Alice".to_string()),
        ];
        let params = SearchParameters::from_items(&items).unwrap();
        assert_eq!(params.resource_params.len(), 2);
        assert_eq!(params.resource_params[0].code, "name");
        assert_eq!(params.resource_params[0].or_values, vec!["John", "Smith"]);
        assert_eq!(params.resource_params[1].or_values, vec!["Alice"]);
    }

    #[test]
    fn from_items_or_splitting_ignores_escaped_commas() {
        let items = vec![("name".to_string(), "John\\,Smith,Alice".to_string())];
        let params = SearchParameters::from_items(&items).unwrap();
        assert_eq!(params.resource_params.len(), 1);
        assert_eq!(
            params.resource_params[0].or_values,
            vec!["John\\,Smith", "Alice"]
        );
    }

    #[test]
    fn from_items_parses_system_search_types() {
        let items = vec![("_type".to_string(), "Patient,Observation".to_string())];
        let params = SearchParameters::from_items(&items).unwrap();
        assert_eq!(params.types, vec!["Patient", "Observation"]);
    }

    #[test]
    fn include_parsing_supports_iterate_and_wildcards() {
        let items = vec![
            (
                "_include".to_string(),
                "Observation:subject:Patient:iterate".to_string(),
            ),
            ("_revinclude".to_string(), "*".to_string()),
        ];
        let params = SearchParameters::from_items(&items).unwrap();
        assert_eq!(params.include.len(), 1);
        assert!(params.include[0].iterate);
        assert_eq!(params.include[0].source_type, "Observation");
        assert_eq!(params.include[0].param, "subject");
        assert_eq!(params.include[0].target_type.as_deref(), Some("Patient"));

        assert_eq!(params.revinclude.len(), 1);
        assert_eq!(params.revinclude[0].source_type, "*");
        assert_eq!(params.revinclude[0].param, "*");
    }

    #[test]
    fn include_parsing_supports_iterate_modifier_in_param_name() {
        let items = vec![
            (
                "_include:iterate".to_string(),
                "Patient:link:RelatedPerson".to_string(),
            ),
            (
                "_revinclude:iterate".to_string(),
                "Observation:subject:Patient".to_string(),
            ),
        ];
        let params = SearchParameters::from_items(&items).unwrap();
        assert_eq!(params.include.len(), 1);
        assert!(params.include[0].iterate);
        assert_eq!(params.include[0].source_type, "Patient");
        assert_eq!(params.include[0].param, "link");
        assert_eq!(
            params.include[0].target_type.as_deref(),
            Some("RelatedPerson")
        );

        assert_eq!(params.revinclude.len(), 1);
        assert!(params.revinclude[0].iterate);
        assert_eq!(params.revinclude[0].source_type, "Observation");
        assert_eq!(params.revinclude[0].param, "subject");
        assert_eq!(params.revinclude[0].target_type.as_deref(), Some("Patient"));
    }

    #[test]
    fn parse_parameter_name_recognizes_reference_type_modifier() {
        // subject:Patient - type modifier only
        let (code, modifier, chain, reverse_chain) = parse_parameter_name("subject:Patient");
        assert_eq!(code, "subject");
        assert_eq!(modifier.as_deref(), Some("Patient"));
        assert!(chain.is_none());
        assert!(reverse_chain.is_none());

        // subject:Patient.name - type modifier + chaining
        // Per FHIR spec 3.2.1.6.5: :Patient restricts reference type, .name is the chain
        let (code, modifier, chain, reverse_chain) = parse_parameter_name("subject:Patient.name");
        assert_eq!(code, "subject");
        assert_eq!(modifier.as_deref(), Some("Patient"));
        assert_eq!(chain.as_deref(), Some("name"));
        assert!(reverse_chain.is_none());
    }

    #[test]
    fn parse_parameter_name_supports_membership_chaining_syntax() {
        let (code, modifier, chain, reverse_chain) = parse_parameter_name("patient._in");
        assert_eq!(code, "patient");
        assert_eq!(chain.as_deref(), Some("_in"));
        assert!(modifier.is_none());
        assert!(reverse_chain.is_none());

        let (code, modifier, chain, reverse_chain) = parse_parameter_name("ingredient._in:not");
        assert_eq!(code, "ingredient");
        assert_eq!(chain.as_deref(), Some("_in"));
        assert_eq!(modifier.as_deref(), Some("not"));
        assert!(reverse_chain.is_none());
    }

    #[test]
    fn sort_must_not_repeat() {
        let items = vec![
            ("_sort".to_string(), "_id".to_string()),
            ("_sort".to_string(), "_lastUpdated".to_string()),
        ];
        let err = SearchParameters::from_items(&items).unwrap_err();
        assert!(err.to_string().contains("_sort"));
    }

    #[test]
    fn filter_parameter_preserves_commas() {
        let items = vec![(
            "_filter".to_string(),
            "name co \"a,b\" and id eq 1,2".to_string(),
        )];
        let params = SearchParameters::from_items(&items).unwrap();
        assert_eq!(params.resource_params.len(), 1);
        assert_eq!(params.resource_params[0].code, "_filter");
        assert_eq!(
            params.resource_params[0].or_values,
            vec!["name co \"a,b\" and id eq 1,2"]
        );
    }
}
