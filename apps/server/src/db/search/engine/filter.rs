use crate::db::search::parameter_lookup::SearchParamType;
use crate::db::search::params;
use crate::db::search::query_builder::{
    FilterAtom, FilterAtomKind, FilterChainStep, FilterExpr, ResolvedParam, SearchModifier,
    SearchPrefix, SearchValue,
};
use crate::Result;
use futures::future::BoxFuture;
use futures::FutureExt;
use sqlx::PgConnection;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FilterOp {
    Eq,
    Ne,
    Co,
    Sw,
    Ew,
    Gt,
    Lt,
    Ge,
    Le,
    Ap,
    Sa,
    Eb,
    Pr,
    Po,
    Ss,
    Sb,
    In,
    Ni,
    Re,
}

impl FilterOp {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "eq" => Some(Self::Eq),
            "ne" => Some(Self::Ne),
            "co" => Some(Self::Co),
            "sw" => Some(Self::Sw),
            "ew" => Some(Self::Ew),
            "gt" => Some(Self::Gt),
            "lt" => Some(Self::Lt),
            "ge" => Some(Self::Ge),
            "le" => Some(Self::Le),
            "ap" => Some(Self::Ap),
            "sa" => Some(Self::Sa),
            "eb" => Some(Self::Eb),
            "pr" => Some(Self::Pr),
            "po" => Some(Self::Po),
            "ss" => Some(Self::Ss),
            "sb" => Some(Self::Sb),
            "in" => Some(Self::In),
            "ni" => Some(Self::Ni),
            "re" => Some(Self::Re),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum FilterValue {
    JsonString(String),
    Token(String),
}

#[derive(Debug, Clone)]
pub(super) struct ParamPathSegment {
    pub name: String,
    pub filter: Option<Box<FilterExprAst>>,
}

#[derive(Debug, Clone)]
pub(super) enum FilterPath {
    ParamPath(Vec<ParamPathSegment>),
    Has(crate::db::search::params::ReverseChainSpec),
}

#[derive(Debug, Clone)]
pub(super) enum FilterExprAst {
    And(Box<FilterExprAst>, Box<FilterExprAst>),
    Or(Box<FilterExprAst>, Box<FilterExprAst>),
    Not(Box<FilterExprAst>),
    Test {
        path: FilterPath,
        op: FilterOp,
        value: FilterValue,
    },
}

pub(super) fn parse_filter(input: &str) -> Result<FilterExprAst> {
    let mut p = Parser::new(input);
    let expr = p.parse_filter()?;
    p.skip_ws();
    if !p.is_eof() {
        return Err(crate::Error::Validation(format!(
            "Unexpected trailing input in _filter: '{}'",
            p.remaining()
        )));
    }
    Ok(expr)
}

impl super::SearchEngine {
    pub(super) async fn resolve_filter_expression(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        raw: &str,
    ) -> Result<FilterExpr> {
        let ast = parse_filter(raw)?;
        self.resolve_filter_ast(conn, resource_type, ast).await
    }

    fn resolve_filter_ast<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        resource_type: &'a str,
        ast: FilterExprAst,
    ) -> BoxFuture<'a, Result<FilterExpr>> {
        async move {
            match ast {
                FilterExprAst::And(a, b) => Ok(FilterExpr::And(
                    Box::new(self.resolve_filter_ast(conn, resource_type, *a).await?),
                    Box::new(self.resolve_filter_ast(conn, resource_type, *b).await?),
                )),
                FilterExprAst::Or(a, b) => Ok(FilterExpr::Or(
                    Box::new(self.resolve_filter_ast(conn, resource_type, *a).await?),
                    Box::new(self.resolve_filter_ast(conn, resource_type, *b).await?),
                )),
                FilterExprAst::Not(inner) => Ok(FilterExpr::Not(Box::new(
                    self.resolve_filter_ast(conn, resource_type, *inner).await?,
                ))),
                FilterExprAst::Test { path, op, value } => {
                    self.resolve_filter_test(conn, resource_type, path, op, value)
                        .await
                }
            }
        }
        .boxed()
    }

    async fn resolve_filter_test(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        path: FilterPath,
        op: FilterOp,
        value: FilterValue,
    ) -> Result<FilterExpr> {
        match path {
            FilterPath::Has(spec) => {
                self.resolve_filter_has(conn, resource_type, spec, op, value)
                    .await
            }
            FilterPath::ParamPath(segments) => {
                self.resolve_filter_param_path(conn, resource_type, segments, op, value)
                    .await
            }
        }
    }

    async fn resolve_filter_has(
        &self,
        conn: &mut PgConnection,
        _resource_type: &str,
        spec: params::ReverseChainSpec,
        op: FilterOp,
        value: FilterValue,
    ) -> Result<FilterExpr> {
        // Resolve filter parameter type (on the referring resource).
        let Some(def) = self
            .param_cache
            .get_param_with_conn(conn, &spec.referring_resource, &spec.filter_param)
            .await?
        else {
            return Err(crate::Error::Validation(format!(
                "Unknown _has filter parameter '{}.{}'",
                spec.referring_resource, spec.filter_param
            )));
        };

        let (kind, maybe_wrap_not) =
            self.build_atom_for_param(&spec.filter_param, def.param_type.clone(), op, value)?;

        let inner = FilterExpr::Atom(FilterAtom {
            chain: Vec::new(),
            kind,
        });

        let inner = if maybe_wrap_not {
            FilterExpr::Not(Box::new(inner))
        } else {
            inner
        };

        Ok(FilterExpr::Has {
            spec,
            filter: Box::new(inner),
        })
    }

    async fn resolve_filter_param_path(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        segments: Vec<ParamPathSegment>,
        op: FilterOp,
        value: FilterValue,
    ) -> Result<FilterExpr> {
        if segments.is_empty() {
            return Err(crate::Error::Validation(
                "Empty path in _filter expression".to_string(),
            ));
        }

        if op == FilterOp::Po
            && segments.len() == 1
            && matches!(
                segments.last().map(|s| s.name.as_str()),
                Some("_lastUpdated")
            )
        {
            let raw = match value {
                FilterValue::JsonString(s) => s,
                FilterValue::Token(s) => s,
            };
            return Ok(FilterExpr::Atom(FilterAtom {
                chain: Vec::new(),
                kind: FilterAtomKind::LastUpdatedOverlaps { value: raw },
            }));
        }

        let resolved_path = self
            .resolve_param_path(conn, resource_type, &segments)
            .await?;

        let (leaf_kind, wrap_not) = match resolved_path.leaf {
            LeafResolution::BuiltIn(code) => {
                let (modifier, raw_value) =
                    map_filter_op_to_builtin_modifier_and_value(op, &value)?;
                let raw = params::RawSearchParam {
                    raw_name: code.clone(),
                    code: code.clone(),
                    modifier,
                    chain: None,
                    reverse_chain: None,
                    raw_value: raw_value.clone(),
                    or_values: vec![raw_value],
                };

                let Some(resolved) = self.resolve_builtin_param(&raw)? else {
                    return Err(crate::Error::Validation(format!(
                        "Unsupported built-in parameter '{}' in _filter",
                        code
                    )));
                };

                (FilterAtomKind::Standard(resolved), false)
            }
            LeafResolution::Def { code, param_type } => {
                self.build_atom_for_param(&code, param_type, op, value)?
            }
        };

        let expr = FilterExpr::Atom(FilterAtom {
            chain: resolved_path.chain,
            kind: leaf_kind,
        });

        Ok(if wrap_not {
            FilterExpr::Not(Box::new(expr))
        } else {
            expr
        })
    }

    async fn resolve_param_path(
        &self,
        conn: &mut PgConnection,
        resource_type: &str,
        segments: &[ParamPathSegment],
    ) -> Result<ResolvedPath> {
        resolve_param_path_inner(self, conn, vec![resource_type.to_string()], segments).await
    }

    fn build_atom_for_param(
        &self,
        code: &str,
        param_type: SearchParamType,
        op: FilterOp,
        value: FilterValue,
    ) -> Result<(FilterAtomKind, bool)> {
        let raw = match &value {
            FilterValue::JsonString(s) => s.clone(),
            FilterValue::Token(s) => s.clone(),
        };

        if op == FilterOp::Pr {
            let desired_present = parse_filter_bool(&value)?;
            let desired_missing = if desired_present { "false" } else { "true" };
            let rp = ResolvedParam {
                raw_name: code.to_string(),
                code: code.to_string(),
                param_type,
                modifier: Some(SearchModifier::Missing),
                chain: None,
                values: vec![SearchValue {
                    raw: desired_missing.to_string(),
                    prefix: None,
                }],
                composite: None,
                reverse_chain: None,
                chain_metadata: None,
            };
            return Ok((FilterAtomKind::Standard(rp), false));
        }

        match param_type {
            SearchParamType::String => match op {
                FilterOp::Eq => Ok((
                    FilterAtomKind::StringEq {
                        code: code.to_string(),
                        value: raw,
                    },
                    false,
                )),
                FilterOp::Ne => Ok((
                    FilterAtomKind::StringEq {
                        code: code.to_string(),
                        value: raw,
                    },
                    true,
                )),
                FilterOp::Co => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::Contains),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Sw => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Ew => Ok((
                    FilterAtomKind::StringEndsWith {
                        code: code.to_string(),
                        value: raw,
                    },
                    false,
                )),
                _ => Err(crate::Error::Validation(format!(
                    "_filter operator '{:?}' is not supported for string parameters",
                    op
                ))),
            },
            SearchParamType::Number | SearchParamType::Quantity => {
                let prefix = match op {
                    FilterOp::Eq => None,
                    FilterOp::Ne => Some(SearchPrefix::Ne),
                    FilterOp::Gt => Some(SearchPrefix::Gt),
                    FilterOp::Lt => Some(SearchPrefix::Lt),
                    FilterOp::Ge => Some(SearchPrefix::Ge),
                    FilterOp::Le => Some(SearchPrefix::Le),
                    FilterOp::Sa => Some(SearchPrefix::Sa),
                    FilterOp::Eb => Some(SearchPrefix::Eb),
                    FilterOp::Ap => Some(SearchPrefix::Ap),
                    _ => {
                        return Err(crate::Error::Validation(format!(
                            "_filter operator '{:?}' is not supported for {:?} parameters",
                            op, param_type
                        )));
                    }
                };
                Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                ))
            }
            SearchParamType::Date => {
                if op == FilterOp::Po {
                    return Ok((
                        FilterAtomKind::DateOverlaps {
                            code: code.to_string(),
                            value: raw,
                        },
                        false,
                    ));
                }

                let prefix = match op {
                    FilterOp::Eq => None,
                    FilterOp::Ne => Some(SearchPrefix::Ne),
                    FilterOp::Gt => Some(SearchPrefix::Gt),
                    FilterOp::Lt => Some(SearchPrefix::Lt),
                    FilterOp::Ge => Some(SearchPrefix::Ge),
                    FilterOp::Le => Some(SearchPrefix::Le),
                    FilterOp::Sa => Some(SearchPrefix::Sa),
                    FilterOp::Eb => Some(SearchPrefix::Eb),
                    FilterOp::Ap => Some(SearchPrefix::Ap),
                    _ => {
                        return Err(crate::Error::Validation(format!(
                            "_filter operator '{:?}' is not supported for date parameters",
                            op
                        )));
                    }
                };

                Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                ))
            }
            SearchParamType::Token => match op {
                FilterOp::Eq => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Ne => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::Not),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Ss => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::Below),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Sb => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::Above),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::In => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::In),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Ni => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::NotIn),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                _ => Err(crate::Error::Validation(format!(
                    "_filter operator '{:?}' is not supported for token parameters",
                    op
                ))),
            },
            SearchParamType::Reference => match op {
                FilterOp::Re => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                _ => Err(crate::Error::Validation(format!(
                    "_filter operator '{:?}' is not supported for reference parameters",
                    op
                ))),
            },
            SearchParamType::Uri => match op {
                FilterOp::Eq => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Co => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::Contains),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                _ => Err(crate::Error::Validation(format!(
                    "_filter operator '{:?}' is not supported for uri parameters",
                    op
                ))),
            },
            SearchParamType::Text | SearchParamType::Content => match op {
                FilterOp::Co | FilterOp::Sw => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: None,
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                FilterOp::Eq => Ok((
                    FilterAtomKind::Standard(ResolvedParam {
                        raw_name: code.to_string(),
                        code: code.to_string(),
                        param_type,
                        modifier: Some(SearchModifier::Exact),
                        chain: None,
                        values: vec![SearchValue { raw, prefix: None }],
                        composite: None,
                        reverse_chain: None,
                        chain_metadata: None,
                    }),
                    false,
                )),
                _ => Err(crate::Error::Validation(format!(
                    "_filter operator '{:?}' is not supported for {:?} parameters",
                    op, param_type
                ))),
            },
            SearchParamType::Composite | SearchParamType::Special => Err(crate::Error::Validation(
                "_filter is not supported for composite/special parameters".to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
enum LeafResolution {
    BuiltIn(String),
    Def {
        code: String,
        param_type: SearchParamType,
    },
}

#[derive(Debug, Clone)]
struct ResolvedPath {
    chain: Vec<FilterChainStep>,
    leaf: LeafResolution,
    valid_types: Vec<String>,
}

fn resolve_param_path_inner<'a>(
    engine: &'a super::SearchEngine,
    conn: &'a mut PgConnection,
    current_types: Vec<String>,
    segments: &'a [ParamPathSegment],
) -> BoxFuture<'a, Result<ResolvedPath>> {
    async move {
        let Some((first, rest)) = segments.split_first() else {
            return Err(crate::Error::Validation(
                "Empty parameter path in _filter".to_string(),
            ));
        };

        if rest.is_empty() && first.name.starts_with('_') {
            if first.filter.is_some() {
                return Err(crate::Error::Validation(
                    "Element-scoped '[...]' filters cannot be applied to a terminal path segment"
                        .to_string(),
                ));
            }
            return Ok(ResolvedPath {
                chain: Vec::new(),
                leaf: LeafResolution::BuiltIn(first.name.clone()),
                valid_types: current_types,
            });
        }

        if rest.is_empty() {
            if first.filter.is_some() {
                return Err(crate::Error::Validation(
                    "Element-scoped '[...]' filters cannot be applied to a terminal path segment"
                        .to_string(),
                ));
            }
            let code = first.name.as_str();

            let mut def: Option<crate::db::search::parameter_lookup::SearchParamDef> = None;
            let mut valid_types = Vec::new();

            for rt in &current_types {
                let Some(next) = engine
                    .param_cache
                    .get_param_with_conn(conn, rt, code)
                    .await?
                else {
                    continue;
                };
                if let Some(prev) = &def {
                    let compatible = prev.param_type == next.param_type
                        && prev.multiple_and == next.multiple_and
                        && prev.multiple_or == next.multiple_or
                        && prev.modifiers == next.modifiers
                        && prev.comparators == next.comparators
                        && prev.components.len() == next.components.len();
                    if !compatible {
                        return Err(crate::Error::Validation(format!(
                            "Ambiguous chained parameter '{}' has incompatible definitions across target types",
                            code
                        )));
                    }
                } else {
                    def = Some(next);
                }
                valid_types.push(rt.clone());
            }

            let def = def.ok_or_else(|| {
                crate::Error::Validation(format!(
                    "Unknown search parameter '{}' in _filter",
                    code
                ))
            })?;

            return Ok(ResolvedPath {
                chain: Vec::new(),
                leaf: LeafResolution::Def {
                    code: code.to_string(),
                    param_type: def.param_type,
                },
                valid_types,
            });
        }

            // Resolve a reference step and recurse into its allowed targets.
        let ref_code = first.name.as_str();
        let mut next_types = Vec::new();
        let mut seen = HashSet::<String>::new();
        let mut defs_by_type =
            HashMap::<String, crate::db::search::parameter_lookup::SearchParamDef>::new();
        let mut valid_current_types = Vec::new();

        for rt in &current_types {
            let Some(def) = engine
                .param_cache
                .get_param_with_conn(conn, rt, ref_code)
                .await?
            else {
                continue;
            };
            if def.param_type != SearchParamType::Reference {
                return Err(crate::Error::Validation(format!(
                    "Chained _filter path requires reference parameter at '{}'",
                    ref_code
                )));
            }
            if def.targets.is_empty() {
                return Err(crate::Error::Validation(format!(
                    "Reference parameter '{}.{}' has no declared targets; chaining is not supported",
                    rt, ref_code
                )));
            }
            for t in &def.targets {
                if seen.insert(t.clone()) {
                    next_types.push(t.clone());
                }
            }
            defs_by_type.insert(rt.clone(), def);
            valid_current_types.push(rt.clone());
        }

        if valid_current_types.is_empty() {
            return Err(crate::Error::Validation(format!(
                "Unknown search parameter '{}.{}' in _filter",
                current_types.first().cloned().unwrap_or_default(),
                ref_code
            )));
        }

        let mut inner = resolve_param_path_inner(engine, conn, next_types, rest).await?;

        // Restrict this step's join types to the types that are actually relevant for the remainder.
        let valid_next_types = inner.valid_types.clone();
        inner.valid_types = valid_current_types
            .into_iter()
            .filter(|rt| {
                defs_by_type
                    .get(rt)
                    .map(|d| d.targets.iter().any(|t| valid_next_types.contains(t)))
                    .unwrap_or(false)
            })
            .collect();

        let step_filter = match first.filter.clone() {
            None => None,
            Some(ast) => {
                if valid_next_types.len() != 1 {
                    return Err(crate::Error::Validation(format!(
                        "Element-scoped '[...]' filters are only supported when '{}' resolves to a single target type",
                        ref_code
                    )));
                }
                let target_type = valid_next_types[0].as_str();
                let resolved = engine.resolve_filter_ast(conn, target_type, *ast).await?;
                Some(Box::new(resolved))
            }
        };

        inner.chain.insert(
            0,
            FilterChainStep {
                reference_param: ref_code.to_string(),
                target_types: valid_next_types,
                filter: step_filter,
            },
        );

        Ok(inner)
    }
    .boxed()
}

fn parse_filter_bool(v: &FilterValue) -> Result<bool> {
    let raw = match v {
        FilterValue::JsonString(s) => s.as_str(),
        FilterValue::Token(s) => s.as_str(),
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(crate::Error::Validation(format!(
            "Expected true|false in _filter expression, got '{}'",
            raw
        ))),
    }
}

fn map_filter_op_to_builtin_modifier_and_value(
    op: FilterOp,
    value: &FilterValue,
) -> Result<(Option<String>, String)> {
    let raw = match value {
        FilterValue::JsonString(s) => s.clone(),
        FilterValue::Token(s) => s.clone(),
    };

    match op {
        FilterOp::Eq | FilterOp::Co | FilterOp::Sw | FilterOp::Ew | FilterOp::Re => Ok((None, raw)),
        FilterOp::Pr => {
            let desired_present = parse_filter_bool(value)?;
            let desired_missing = if desired_present { "false" } else { "true" };
            Ok((Some("missing".to_string()), desired_missing.to_string()))
        }
        FilterOp::Ne => Ok((Some("not".to_string()), raw)),
        FilterOp::Gt => Ok((None, format!("gt{}", raw))),
        FilterOp::Lt => Ok((None, format!("lt{}", raw))),
        FilterOp::Ge => Ok((None, format!("ge{}", raw))),
        FilterOp::Le => Ok((None, format!("le{}", raw))),
        FilterOp::Sa => Ok((None, format!("sa{}", raw))),
        FilterOp::Eb => Ok((None, format!("eb{}", raw))),
        FilterOp::Ap => Ok((None, format!("ap{}", raw))),
        FilterOp::Ss => Ok((Some("below".to_string()), raw)),
        FilterOp::Sb => Ok((Some("above".to_string()), raw)),
        FilterOp::In => Ok((Some("in".to_string()), raw)),
        FilterOp::Ni => Ok((Some("not-in".to_string()), raw)),
        FilterOp::Po => Err(crate::Error::Validation(
            "_filter operator 'po' is only supported for date parameters".to_string(),
        )),
    }
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn consume_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek_char(), Some(c) if c.is_whitespace()) {
            self.consume_char();
        }
    }

    fn eat_keyword(&mut self, kw: &str) -> bool {
        let save = self.pos;
        self.skip_ws();
        let Some(word) = self.try_parse_name() else {
            self.pos = save;
            return false;
        };
        if word.eq_ignore_ascii_case(kw) {
            true
        } else {
            self.pos = save;
            false
        }
    }

    fn expect_char(&mut self, expected: char) -> Result<()> {
        self.skip_ws();
        match self.consume_char() {
            Some(c) if c == expected => Ok(()),
            _ => Err(crate::Error::Validation(format!(
                "Expected '{}' in _filter expression",
                expected
            ))),
        }
    }

    fn parse_filter(&mut self) -> Result<FilterExprAst> {
        // Left-associative evaluation for `and` / `or` with no precedence (FHIR R5 3.2.3).
        let mut left = self.parse_term()?;
        loop {
            if self.eat_keyword("and") {
                let right = self.parse_term()?;
                left = FilterExprAst::And(Box::new(left), Box::new(right));
                continue;
            }
            if self.eat_keyword("or") {
                let right = self.parse_term()?;
                left = FilterExprAst::Or(Box::new(left), Box::new(right));
                continue;
            }
            break;
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<FilterExprAst> {
        self.skip_ws();

        if self.eat_keyword("not") {
            self.expect_char('(')?;
            let inner = self.parse_filter()?;
            self.expect_char(')')?;
            return Ok(FilterExprAst::Not(Box::new(inner)));
        }

        if matches!(self.peek_char(), Some('(')) {
            self.expect_char('(')?;
            let inner = self.parse_filter()?;
            self.expect_char(')')?;
            return Ok(inner);
        }

        self.parse_test()
    }

    fn parse_test(&mut self) -> Result<FilterExprAst> {
        let path = self.parse_param_value()?;
        self.require_ws("after filter path")?;
        let op_word = self.parse_name("comparison operator")?;
        let Some(op) = FilterOp::parse(&op_word) else {
            return Err(crate::Error::Validation(format!(
                "Unknown _filter operator '{}'",
                op_word
            )));
        };
        self.require_ws("after filter operator")?;
        let value = self.parse_comp_value()?;
        Ok(FilterExprAst::Test { path, op, value })
    }

    fn require_ws(&mut self, ctx: &str) -> Result<()> {
        let before = self.pos;
        self.skip_ws();
        if self.pos == before {
            return Err(crate::Error::Validation(format!(
                "Expected whitespace {} in _filter expression",
                ctx
            )));
        }
        Ok(())
    }

    fn parse_param_value(&mut self) -> Result<FilterPath> {
        self.skip_ws();

        if self.remaining().starts_with("_has:") {
            self.pos += "_has:".len();
            let referring_resource = self.parse_name("resource type after _has:")?;
            self.expect_raw_char(':')?;
            let referring_param = self.parse_name("reference parameter after _has")?;
            self.expect_raw_char(':')?;
            let filter_param = self.parse_name("filter parameter after _has")?;
            return Ok(FilterPath::Has(
                crate::db::search::params::ReverseChainSpec {
                    referring_resource,
                    referring_param,
                    filter_param,
                },
            ));
        }

        let mut segments = Vec::new();
        let first = self.parse_name("parameter name")?;
        segments.push(ParamPathSegment {
            name: first,
            filter: None,
        });

        loop {
            match self.peek_char() {
                Some('[') => {
                    self.consume_char();
                    let f = self.parse_filter()?;
                    self.expect_char(']')?;
                    if let Some(last) = segments.last_mut() {
                        if last.filter.is_some() {
                            return Err(crate::Error::Validation(
                                "Duplicate '[...]' filter on parameter path segment".to_string(),
                            ));
                        }
                        last.filter = Some(Box::new(f));
                    }
                    self.expect_raw_char('.')?;
                    if self.remaining().starts_with("_has:") {
                        return Err(crate::Error::Validation(
                            "Chaining into '_has:...' is not supported in _filter paths"
                                .to_string(),
                        ));
                    }
                    let next = self.parse_name("chained parameter name")?;
                    segments.push(ParamPathSegment {
                        name: next,
                        filter: None,
                    });
                    continue;
                }
                Some('.') => {
                    self.consume_char();
                    if self.remaining().starts_with("_has:") {
                        return Err(crate::Error::Validation(
                            "Chaining into '_has:...' is not supported in _filter paths"
                                .to_string(),
                        ));
                    }
                    let next = self.parse_name("chained parameter name")?;
                    segments.push(ParamPathSegment {
                        name: next,
                        filter: None,
                    });
                    continue;
                }
                _ => break,
            }
        }

        Ok(FilterPath::ParamPath(segments))
    }

    fn parse_comp_value(&mut self) -> Result<FilterValue> {
        self.skip_ws();
        match self.peek_char() {
            Some('"') => {
                let s = self.parse_json_string()?;
                Ok(FilterValue::JsonString(s))
            }
            Some(_) => {
                let tok = self.parse_token_value()?;
                Ok(FilterValue::Token(tok))
            }
            None => Err(crate::Error::Validation(
                "Expected value in _filter expression".to_string(),
            )),
        }
    }

    fn parse_json_string(&mut self) -> Result<String> {
        self.skip_ws();
        let start = self.pos;
        if self.consume_char() != Some('"') {
            return Err(crate::Error::Validation(
                "Expected JSON string in _filter expression".to_string(),
            ));
        }

        let mut escaped = false;
        while let Some(c) = self.consume_char() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' => escaped = true,
                '"' => {
                    let end = self.pos;
                    let raw = &self.input[start..end];
                    return serde_json::from_str::<String>(raw).map_err(|_| {
                        crate::Error::Validation(format!("Invalid JSON string in _filter: {}", raw))
                    });
                }
                _ => {}
            }
        }

        Err(crate::Error::Validation(
            "Unterminated JSON string in _filter expression".to_string(),
        ))
    }

    fn parse_token_value(&mut self) -> Result<String> {
        self.skip_ws();
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() || c == ')' || c == ']' {
                break;
            }
            self.consume_char();
        }
        let tok = self.input[start..self.pos].to_string();
        if tok.trim().is_empty() {
            return Err(crate::Error::Validation(
                "Empty token value in _filter expression".to_string(),
            ));
        }
        Ok(tok)
    }

    fn parse_name(&mut self, ctx: &str) -> Result<String> {
        self.skip_ws();
        self.try_parse_name().ok_or_else(|| {
            crate::Error::Validation(format!("Expected {} in _filter expression", ctx))
        })
    }

    fn try_parse_name(&mut self) -> Option<String> {
        self.skip_ws();
        let start = self.pos;
        let first = self.peek_char()?;
        if !(first == '_' || first.is_ascii_alphabetic()) {
            return None;
        }
        self.consume_char();
        while let Some(c) = self.peek_char() {
            if c == '_' || c == '-' || c.is_ascii_alphanumeric() {
                self.consume_char();
            } else {
                break;
            }
        }
        Some(self.input[start..self.pos].to_string())
    }

    fn expect_raw_char(&mut self, expected: char) -> Result<()> {
        match self.consume_char() {
            Some(c) if c == expected => Ok(()),
            _ => Err(crate::Error::Validation(format!(
                "Expected '{}' in _filter expression",
                expected
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_and_or_left_to_right() {
        let f = parse_filter("a eq 1 and b eq 2 or c eq 3").unwrap();
        // Left-to-right: ((a and b) or c)
        match f {
            FilterExprAst::Or(_, _) => {}
            _ => panic!("expected Or at top"),
        }
    }

    #[test]
    fn parses_not_parenthesized() {
        let f = parse_filter("not(a eq 1)").unwrap();
        match f {
            FilterExprAst::Not(inner) => match *inner {
                FilterExprAst::Test { .. } => {}
                _ => panic!("expected test"),
            },
            _ => panic!("expected not"),
        }
    }

    #[test]
    fn parses_has_specifier() {
        let f = parse_filter("_has:Observation:patient:code eq http://loinc.org|1234-5").unwrap();
        match f {
            FilterExprAst::Test { path, op, .. } => {
                assert_eq!(op, FilterOp::Eq);
                match path {
                    FilterPath::Has(spec) => {
                        assert_eq!(spec.referring_resource, "Observation");
                        assert_eq!(spec.referring_param, "patient");
                        assert_eq!(spec.filter_param, "code");
                    }
                    _ => panic!("expected has path"),
                }
            }
            _ => panic!("expected test"),
        }
    }

    #[test]
    fn parses_element_scoped_filter() {
        let f = parse_filter(r#"patient[gender eq female].name co "pet""#).unwrap();
        match f {
            FilterExprAst::Test { path, op, value } => {
                assert_eq!(op, FilterOp::Co);
                assert!(matches!(value, FilterValue::JsonString(s) if s == "pet"));
                match path {
                    FilterPath::ParamPath(segs) => {
                        assert_eq!(segs.len(), 2);
                        assert_eq!(segs[0].name, "patient");
                        assert!(segs[0].filter.is_some());
                        assert_eq!(segs[1].name, "name");
                        assert!(segs[1].filter.is_none());

                        let inner = segs[0].filter.as_ref().unwrap();
                        match &**inner {
                            FilterExprAst::Test { path, op, value } => {
                                assert_eq!(*op, FilterOp::Eq);
                                assert!(matches!(path, FilterPath::ParamPath(_)));
                                assert!(matches!(value, FilterValue::Token(s) if s == "female"));
                            }
                            _ => panic!("expected inner test"),
                        }
                    }
                    _ => panic!("expected param path"),
                }
            }
            _ => panic!("expected test"),
        }
    }
}
