//! Search parameter lookup and metadata

use crate::Result;
use sqlx::{PgConnection, PgPool};
use std::collections::HashMap;
use std::str::FromStr;

/// Search parameter type
#[derive(Debug, Clone, PartialEq)]
pub enum SearchParamType {
    String,
    Number,
    Date,
    Token,
    Reference,
    Quantity,
    Uri,
    Text,
    Content,
    Composite,
    Special,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSearchParamTypeError;

impl FromStr for SearchParamType {
    type Err = ParseSearchParamTypeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "string" => Ok(Self::String),
            "number" => Ok(Self::Number),
            "date" => Ok(Self::Date),
            "token" => Ok(Self::Token),
            "reference" => Ok(Self::Reference),
            "quantity" => Ok(Self::Quantity),
            "uri" => Ok(Self::Uri),
            "text" => Ok(Self::Text),
            "content" => Ok(Self::Content),
            "composite" => Ok(Self::Composite),
            "special" => Ok(Self::Special),
            _ => Err(ParseSearchParamTypeError),
        }
    }
}

impl SearchParamType {
    /// Parse from string, returns None for invalid values
    /// Prefer using `FromStr` trait for better error handling
    pub fn try_from_str(s: &str) -> Option<Self> {
        Self::from_str(s).ok()
    }

    pub fn table_name(&self) -> &'static str {
        match self {
            Self::String => "search_string",
            Self::Number => "search_number",
            Self::Date => "search_date",
            Self::Token => "search_token",
            Self::Reference => "search_reference",
            Self::Quantity => "search_quantity",
            Self::Uri => "search_uri",
            Self::Text => "search_text",
            Self::Content => "search_content",
            Self::Composite => panic!("Composite params need special handling"),
            Self::Special => panic!("Special params need special handling"),
        }
    }
}

/// Search parameter definition
#[derive(Debug, Clone)]
pub struct SearchParamDef {
    pub id: i32,
    pub code: String,
    pub resource_type: String,
    pub param_type: SearchParamType,
    pub expression: Option<String>,
    pub url: Option<String>,
    pub multiple_or: bool,
    pub multiple_and: bool,
    pub comparators: Vec<String>,
    pub modifiers: Vec<String>,
    pub chains: Vec<String>,
    pub targets: Vec<String>,
    pub components: Vec<CompositeComponentDef>,
}

#[derive(Debug, Clone)]
pub struct CompositeComponentDef {
    pub position: i32,
    pub definition_url: String,
    pub expression: Option<String>,
    pub component_code: String,
    pub component_type: SearchParamType,
}

/// Cache for search parameter definitions
pub struct SearchParamCache {
    db_pool: PgPool,
    // Cache: (resource_type, code) -> SearchParamDef
    cache: std::sync::RwLock<HashMap<(String, String), SearchParamDef>>,
}

impl SearchParamCache {
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            cache: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Clear all cached search parameter definitions.
    pub fn invalidate(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    pub async fn get_param_with_conn(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        code: &str,
    ) -> Result<Option<SearchParamDef>> {
        let key = (resource_type.to_string(), code.to_string());

        tracing::debug!("Looking up search param: {}.{}", resource_type, code);

        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(def) = cache.get(&key) {
                tracing::debug!("Found {}.{} in cache", resource_type, code);
                return Ok(Some(def.clone()));
            }
        }

        tracing::debug!(
            "Cache miss for {}.{}, querying database",
            resource_type,
            code
        );
        let types_to_try = [resource_type, "DomainResource", "Resource"];
        for rt in types_to_try {
            tracing::debug!("Trying to find param {} for type {}", code, rt);
            if let Some(def) = self.query_param_with_conn(conn, rt, code).await? {
                tracing::debug!("Found param {} for type {}", code, rt);
                {
                    let mut cache = self.cache.write().unwrap();
                    cache.insert(key, def.clone());
                }
                return Ok(Some(def));
            }
        }

        tracing::debug!("Param {}.{} not found", resource_type, code);
        Ok(None)
    }

    /// Lookup search parameter definition
    pub async fn get_param(
        &self,
        resource_type: &str,
        code: &str,
    ) -> Result<Option<SearchParamDef>> {
        let mut conn = self
            .db_pool
            .acquire()
            .await
            .map_err(crate::Error::Database)?;
        self.get_param_with_conn(&mut conn, resource_type, code)
            .await
    }

    async fn query_param_with_conn(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        code: &str,
    ) -> Result<Option<SearchParamDef>> {
        let row: Option<(
            i32,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            bool,
            bool,
            Option<Vec<String>>,
            Option<Vec<String>>,
            Option<Vec<String>>,
            Option<Vec<String>>,
        )> = sqlx::query_as(
            r#"
            SELECT id, code, resource_type, type, expression, url,
                   multiple_or, multiple_and,
                   comparators, modifiers, chains, targets
            FROM search_parameters
            WHERE resource_type = $1 AND code = $2 AND active = true
            LIMIT 1
            "#,
        )
        .bind(resource_type)
        .bind(code)
        .fetch_optional(&mut *conn)
        .await
        .map_err(crate::Error::Database)?;

        let Some((
            id,
            code,
            resource_type,
            param_type_str,
            expression,
            url,
            multiple_or,
            multiple_and,
            comparators,
            modifiers,
            chains,
            targets,
        )) = row
        else {
            return Ok(None);
        };

        let Some(param_type) = SearchParamType::try_from_str(&param_type_str) else {
            return Ok(None);
        };

        let mut def = SearchParamDef {
            id,
            code,
            resource_type,
            param_type,
            expression,
            url,
            multiple_or,
            multiple_and,
            comparators: comparators.unwrap_or_default(),
            modifiers: modifiers
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
            chains: chains.unwrap_or_default(),
            targets: targets.unwrap_or_default(),
            components: Vec::new(),
        };

        if def.param_type == SearchParamType::Composite {
            def.components = self
                .load_composite_components_with_conn(conn, def.id)
                .await?;
        }

        Ok(Some(def))
    }

    async fn load_composite_components_with_conn(
        &self,
        conn: &mut PgConnection,
        search_parameter_id: i32,
    ) -> Result<Vec<CompositeComponentDef>> {
        let rows: Vec<(i32, String, Option<String>, String, String)> = sqlx::query_as(
            r#"
            SELECT c.position, c.definition_url, c.expression, d.code, d.type
            FROM search_parameter_components c
            INNER JOIN search_parameters d ON d.url = c.definition_url AND d.active = true
            WHERE c.search_parameter_id = $1
            ORDER BY c.position ASC
            "#,
        )
        .bind(search_parameter_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(crate::Error::Database)?;

        let mut out = Vec::new();
        for (position, definition_url, expression, component_code, component_type_str) in rows {
            let Some(component_type) = SearchParamType::try_from_str(&component_type_str) else {
                return Err(crate::Error::Internal(format!(
                    "Unsupported component type '{}' for composite search parameter id {}",
                    component_type_str, search_parameter_id
                )));
            };
            if component_type == SearchParamType::Composite {
                return Err(crate::Error::Validation(
                    "Composite search parameters must not reference composite components"
                        .to_string(),
                ));
            }
            out.push(CompositeComponentDef {
                position,
                definition_url,
                expression,
                component_code,
                component_type,
            });
        }

        Ok(out)
    }
}
