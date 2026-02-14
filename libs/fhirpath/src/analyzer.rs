use crate::ast::AstNode;
use crate::error::{Error, Result};
use crate::functions::FunctionRegistry;
use crate::hir::{HirBinaryOperator, HirNode, HirTypeOperator, HirUnaryOperator, PathSegmentHir};
use crate::types::{Cardinality, ExprType, TypeId, TypeRegistry};
use crate::value::Value;
use crate::variables::VariableRegistry;
use rust_decimal::prelude::ToPrimitive;
use std::sync::{Arc, Mutex};
use ferrum_context::FhirContext;

pub struct Analyzer {
    type_registry: Arc<TypeRegistry>,
    function_registry: Arc<FunctionRegistry>,
    variable_registry: Arc<Mutex<VariableRegistry>>,
}

impl Analyzer {
    pub fn new(
        type_registry: Arc<TypeRegistry>,
        function_registry: Arc<FunctionRegistry>,
        variable_registry: Arc<Mutex<VariableRegistry>>,
    ) -> Self {
        Self {
            type_registry,
            function_registry,
            variable_registry,
        }
    }

    const MAX_RECURSION_DEPTH: usize = 200;

    #[allow(dead_code)]
    pub fn analyze(&self, ast: AstNode) -> Result<HirNode> {
        // For top-level analysis, we don't know the base type yet
        // It will be determined at runtime from the context
        self.analyze_node(ast, None, None, 0)
    }

    /// Semantic analysis with known base type: AST → HIR
    ///
    /// When base_type_name is provided, the compiler will validate field accesses
    /// against the StructureDefinition for that type.
    pub fn analyze_with_type(
        &self,
        ast: AstNode,
        base_type_name: Option<String>,
    ) -> Result<HirNode> {
        // If we have a type name, try to get the TypeId for it (for System types)
        let base_type_id = base_type_name
            .as_ref()
            .and_then(|name| self.type_registry.get_type_id_by_name(name));

        self.analyze_node(ast, base_type_id, base_type_name, 0)
    }

    /// Analyze an AST node recursively
    ///
    /// * `base_type_id`: TypeId for System types (String, Integer, etc.)
    /// * `base_type_name`: Type name for validation (works for both System and FHIR types)
    /// * `depth`: Current recursion depth
    #[allow(clippy::only_used_in_recursion)]
    fn analyze_node(
        &self,
        ast: AstNode,
        base_type_id: Option<crate::types::TypeId>,
        base_type_name: Option<String>,
        depth: usize,
    ) -> Result<HirNode> {
        if depth > Self::MAX_RECURSION_DEPTH {
            return Err(Error::TypeError(format!(
                "AST too deeply nested (max depth: {})",
                Self::MAX_RECURSION_DEPTH
            )));
        }
        let next_depth = depth + 1;
        match ast {
            // ============================================
            // Literals
            // ============================================
            AstNode::NullLiteral => Ok(HirNode::Literal {
                value: Value::empty(),
                ty: ExprType::empty(),
            }),
            AstNode::BooleanLiteral(b) => Ok(HirNode::Literal {
                value: Value::boolean(b),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::Boolean, Cardinality::ONE_TO_ONE),
            }),
            AstNode::StringLiteral(s) => Ok(HirNode::Literal {
                value: Value::string(s),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::String, Cardinality::ONE_TO_ONE),
            }),
            AstNode::IntegerLiteral(i) => Ok(HirNode::Literal {
                value: Value::integer(i),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::Integer, Cardinality::ONE_TO_ONE),
            }),
            AstNode::NumberLiteral(d) => Ok(HirNode::Literal {
                value: Value::decimal(d),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::Decimal, Cardinality::ONE_TO_ONE),
            }),
            AstNode::LongNumberLiteral(i) => Ok(HirNode::Literal {
                value: Value::integer(i),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::Integer, Cardinality::ONE_TO_ONE),
            }),
            AstNode::DateLiteral(d, precision) => Ok(HirNode::Literal {
                value: Value::date_with_precision(d, precision),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::Date, Cardinality::ONE_TO_ONE),
            }),
            AstNode::DateTimeLiteral(dt, precision, timezone_offset) => {
                // Convert FixedOffset to Utc, but preserve whether an offset was specified.
                use chrono::Utc;
                let dt_utc = dt.with_timezone(&Utc);
                Ok(HirNode::Literal {
                    value: Value::datetime_with_precision_and_offset(
                        dt_utc,
                        precision,
                        timezone_offset,
                    ),
                    ty: self
                        .type_registry
                        .expr_from_system_type(TypeId::DateTime, Cardinality::ONE_TO_ONE),
                })
            }
            AstNode::TimeLiteral(t, precision) => Ok(HirNode::Literal {
                value: Value::time_with_precision(t, precision),
                ty: self
                    .type_registry
                    .expr_from_system_type(TypeId::Time, Cardinality::ONE_TO_ONE),
            }),
            AstNode::QuantityLiteral { value, unit } => {
                // Create quantity value with unit
                fn normalize_quantity_unit(unit: &str) -> &str {
                    match unit {
                        "year" | "years" => "year",
                        "month" | "months" => "month",
                        "week" | "weeks" => "week",
                        "day" | "days" => "day",
                        "hour" | "hours" => "hour",
                        "minute" | "minutes" => "minute",
                        "second" | "seconds" => "second",
                        "millisecond" | "milliseconds" => "millisecond",
                        other => other,
                    }
                }

                let unit_str = unit
                    .map(|u| normalize_quantity_unit(u.as_str()).to_string())
                    .unwrap_or_else(|| "1".to_string());
                Ok(HirNode::Literal {
                    value: Value::quantity(value, Arc::from(unit_str.as_str())),
                    ty: self
                        .type_registry
                        .expr_from_system_type(TypeId::Quantity, Cardinality::ONE_TO_ONE),
                })
            }
            AstNode::CollectionLiteral { elements } => {
                // Collection literal: {expr, expr, ...}
                // Convert to a union of all elements: expr | expr | ...
                if elements.is_empty() {
                    // Empty collection: {}
                    Ok(HirNode::Literal {
                        value: Value::empty(),
                        ty: ExprType::empty(),
                    })
                } else {
                    // Build union expression: elem1 | elem2 | ...
                    let mut union_node = elements[0].clone();
                    for elem in elements.iter().skip(1) {
                        union_node = AstNode::UnionExpression {
                            left: Box::new(union_node),
                            right: Box::new(elem.clone()),
                        };
                    }
                    // Analyze the union expression
                    self.analyze_node(union_node, base_type_id, base_type_name.clone(), next_depth)
                }
            }

            // ============================================
            // Terms (unwrap and recurse)
            // ============================================
            AstNode::TermExpression { term } => {
                self.analyze_node(*term, base_type_id, base_type_name.clone(), next_depth)
            }
            AstNode::InvocationTerm { invocation } => self.analyze_node(
                *invocation,
                base_type_id,
                base_type_name.clone(),
                next_depth,
            ),
            AstNode::LiteralTerm { literal } => {
                self.analyze_node(*literal, base_type_id, base_type_name.clone(), next_depth)
            }
            AstNode::ParenthesizedTerm { expression } => self.analyze_node(
                *expression,
                base_type_id,
                base_type_name.clone(),
                next_depth,
            ),
            AstNode::ExternalConstantTerm { constant } => {
                // External constants are resolved at runtime via context variables
                let var_id = {
                    let mut registry = self.variable_registry.lock().unwrap();
                    registry.resolve(&constant)
                };
                Ok(HirNode::Variable {
                    var_id,
                    ty: ExprType::unknown().with_cardinality(Cardinality::ZERO_TO_ONE),
                    name: Some(Arc::from(constant)),
                })
            }

            // ============================================
            // Invocations
            // ============================================
            AstNode::MemberInvocation { identifier } => {
                // Structural-only: identifiers resolve as path navigation from $this.
                // All semantic validation and type inference happens in the dedicated type pass.
                let base_node = HirNode::Variable {
                    var_id: 0, // $this
                    ty: ExprType::unknown().with_cardinality(Cardinality::ZERO_TO_ONE),
                    name: None,
                };

                Ok(HirNode::Path {
                    base: Box::new(base_node),
                    segments: vec![PathSegmentHir::Field(identifier)],
                    result_ty: ExprType::unknown(),
                })
            }
            AstNode::FunctionInvocation {
                function_name,
                parameters,
            } => {
                let func_id = self
                    .function_registry
                    .resolve(&function_name)
                    .ok_or_else(|| Error::FunctionNotFound(function_name.clone()))?;

                // Validate argument count
                self.function_registry
                    .validate_args(func_id, parameters.len())
                    .map_err(Error::InvalidOperation)?;

                // Analyze parameters
                // Special handling for is() and as() functions: convert identifier arguments to strings
                let args: Result<Vec<HirNode>> = if function_name == "is"
                    || function_name == "as"
                    || function_name == "ofType"
                {
                    parameters
                        .into_iter()
                        .map(|p| {
                            // Helper to extract identifier from AST node (handles both simple and qualified identifiers)
                            fn extract_identifier(ast: &AstNode) -> Option<String> {
                                match ast {
                                    AstNode::InvocationTerm { invocation } => {
                                        if let AstNode::MemberInvocation { identifier } =
                                            invocation.as_ref()
                                        {
                                            Some(identifier.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    AstNode::TermExpression { term } => extract_identifier(term),
                                    // Handle qualified identifiers like System.Integer
                                    AstNode::InvocationExpression {
                                        expression,
                                        invocation,
                                    } => {
                                        // Get the left side (namespace)
                                        let left = extract_identifier(expression.as_ref())?;
                                        // Get the right side (type name)
                                        if let AstNode::MemberInvocation { identifier } =
                                            invocation.as_ref()
                                        {
                                            Some(format!("{}.{}", left, identifier))
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                }
                            }

                            // If parameter is an identifier, convert to string literal
                            if let Some(identifier) = extract_identifier(&p) {
                                return Ok(HirNode::Literal {
                                    value: crate::value::Value::string(identifier),
                                    ty: self.type_registry.expr_from_system_type(
                                        TypeId::String,
                                        Cardinality::ONE_TO_ONE,
                                    ),
                                });
                            }
                            // Otherwise, analyze normally
                            self.analyze_node(p, None, None, next_depth)
                        })
                        .collect()
                } else {
                    parameters
                        .into_iter()
                        .map(|p| self.analyze_node(p, None, None, next_depth))
                        .collect()
                };
                let args = args?;

                // Structural-only: type pass assigns a concrete return type.
                let result_ty = ExprType::unknown();

                // Special handling for where/select (higher-order functions)
                // Note: These should normally be called as methods (collection.where()),
                // but we handle standalone calls here for completeness
                if function_name == "where" {
                    if args.len() != 1 {
                        return Err(Error::InvalidOperation(
                            "where() requires exactly 1 argument".into(),
                        ));
                    }
                    // Analyze predicate
                    let predicate_hir = args[0].clone();
                    // Create Where node with $this as collection
                    return Ok(HirNode::Where {
                        collection: Box::new(HirNode::Variable {
                            var_id: 0, // $this
                            ty: ExprType::unknown().with_cardinality(Cardinality::ZERO_TO_ONE),
                            name: None,
                        }),
                        predicate_hir: Box::new(predicate_hir),
                        predicate_plan_id: 0, // Will be set during codegen
                        result_ty,
                    });
                }

                if function_name == "select" {
                    if args.len() != 1 {
                        return Err(Error::InvalidOperation(
                            "select() requires exactly 1 argument".into(),
                        ));
                    }
                    // Analyze projection
                    let projection_hir = args[0].clone();
                    return Ok(HirNode::Select {
                        collection: Box::new(HirNode::Variable {
                            var_id: 0, // $this
                            ty: ExprType::unknown().with_cardinality(Cardinality::ZERO_TO_ONE),
                            name: None,
                        }),
                        projection_hir: Box::new(projection_hir),
                        projection_plan_id: 0, // Will be set during codegen
                        result_ty,
                    });
                }

                Ok(HirNode::FunctionCall {
                    func_id,
                    args,
                    result_ty,
                })
            }
            AstNode::ThisInvocation => {
                Ok(HirNode::Variable {
                    var_id: 0, // $this
                    ty: ExprType::unknown().with_cardinality(Cardinality::ZERO_TO_ONE),
                    name: None,
                })
            }
            AstNode::IndexInvocation => {
                Ok(HirNode::Variable {
                    var_id: 1, // $index
                    ty: self
                        .type_registry
                        .expr_from_system_type(TypeId::Integer, Cardinality::ZERO_TO_ONE),
                    name: None,
                })
            }
            AstNode::TotalInvocation => {
                Ok(HirNode::Variable {
                    var_id: 2, // $total
                    ty: self
                        .type_registry
                        .expr_from_system_type(TypeId::Integer, Cardinality::ZERO_TO_ONE),
                    name: None,
                })
            }

            // ============================================
            // Path Navigation
            // ============================================
            AstNode::InvocationExpression {
                expression,
                invocation,
            } => {
                // Guard: unary minus directly applied to a literal with an immediate convertsToInteger/Decimal call
                // should be invalid unless parenthesized (per HL7 tests: -1.convertsToInteger is invalid, (-1).convertsToInteger is valid).
                let negative_base = matches!(
                    expression.as_ref(),
                    AstNode::PolarityExpression {
                        operator: crate::ast::PolarityOperator::Minus,
                        ..
                    }
                ) || is_negative_numeric_literal(expression.as_ref());

                if negative_base {
                    if let AstNode::FunctionInvocation { function_name, .. } = invocation.as_ref() {
                        if function_name == "convertsToInteger"
                            || function_name == "convertsToDecimal"
                        {
                            return Err(Error::InvalidOperation(
                                "Invalid operation: negative literal invocation requires parentheses"
                                    .into(),
                            ));
                        }
                    }
                }

                // Not a FHIR type name or type check passed - proceed normally
                let base_hir = self.analyze_node(
                    *expression,
                    base_type_id,
                    base_type_name.clone(),
                    next_depth,
                )?;

                // Extract path segments from invocation
                let mut segments = Vec::new();

                match *invocation {
                    AstNode::MemberInvocation { identifier } => {
                        segments.push(PathSegmentHir::Field(identifier));
                    }
                    AstNode::FunctionInvocation {
                        function_name,
                        parameters,
                    } => {
                        // Special handling: exists() with predicate behaves like where()+notEmpty
                        if function_name == "exists" {
                            if parameters.len() > 1 {
                                return Err(Error::InvalidOperation(
                                    "exists() accepts at most 1 argument".into(),
                                ));
                            }
                            let predicate_hir = if let Some(param) = parameters.first() {
                                Some(Box::new(self.analyze_node(
                                    param.clone(),
                                    None,
                                    None,
                                    next_depth,
                                )?))
                            } else {
                                None
                            };
                            return Ok(HirNode::Exists {
                                collection: Box::new(base_hir),
                                predicate_hir,
                                predicate_plan_id: 0,
                                result_ty: self.type_registry.expr_from_system_type(
                                    TypeId::Boolean,
                                    Cardinality::ONE_TO_ONE,
                                ),
                            });
                        }

                        // Special handling: all() is higher-order (predicate evaluated per item)
                        if function_name == "all" {
                            if parameters.len() != 1 {
                                return Err(Error::InvalidOperation(
                                    "all() requires exactly 1 argument".into(),
                                ));
                            }
                            let predicate_hir =
                                self.analyze_node(parameters[0].clone(), None, None, next_depth)?;
                            return Ok(HirNode::All {
                                collection: Box::new(base_hir),
                                predicate_hir: Box::new(predicate_hir),
                                predicate_plan_id: 0,
                                result_ty: self.type_registry.expr_from_system_type(
                                    TypeId::Boolean,
                                    Cardinality::ONE_TO_ONE,
                                ),
                            });
                        }

                        // Special handling for higher-order functions (where, select)
                        if function_name == "where" {
                            if parameters.len() != 1 {
                                return Err(Error::InvalidOperation(
                                    "where() requires exactly 1 argument".into(),
                                ));
                            }
                            // Analyze the predicate expression (will be compiled as subplan in codegen)
                            let predicate_hir =
                                self.analyze_node(parameters[0].clone(), None, None, next_depth)?;

                            // Create Where node - predicate_plan_id will be set during codegen
                            return Ok(HirNode::Where {
                                collection: Box::new(base_hir),
                                predicate_hir: Box::new(predicate_hir),
                                predicate_plan_id: 0, // Placeholder, set in codegen
                                result_ty: ExprType::unknown(),
                            });
                        }

                        if function_name == "select" {
                            if parameters.len() != 1 {
                                return Err(Error::InvalidOperation(
                                    "select() requires exactly 1 argument".into(),
                                ));
                            }
                            // Analyze the projection expression (will be compiled as subplan in codegen)
                            let projection_hir =
                                self.analyze_node(parameters[0].clone(), None, None, next_depth)?;

                            // Create Select node - projection_plan_id will be set during codegen
                            return Ok(HirNode::Select {
                                collection: Box::new(base_hir),
                                projection_hir: Box::new(projection_hir),
                                projection_plan_id: 0, // Placeholder, set in codegen
                                result_ty: ExprType::unknown(),
                            });
                        }

                        if function_name == "repeat" {
                            if parameters.len() != 1 {
                                return Err(Error::InvalidOperation(
                                    "repeat() requires exactly 1 argument".into(),
                                ));
                            }
                            // Analyze the projection expression (will be compiled as subplan in codegen)
                            let projection_hir =
                                self.analyze_node(parameters[0].clone(), None, None, next_depth)?;

                            // Create Repeat node - projection_plan_id will be set during codegen
                            return Ok(HirNode::Repeat {
                                collection: Box::new(base_hir),
                                projection_hir: Box::new(projection_hir),
                                projection_plan_id: 0, // Placeholder, set in codegen
                                result_ty: ExprType::unknown(),
                            });
                        }

                        if function_name == "aggregate" {
                            if parameters.is_empty() || parameters.len() > 2 {
                                return Err(Error::InvalidOperation(
                                    "aggregate() requires 1 or 2 arguments".into(),
                                ));
                            }
                            // Analyze the aggregator expression (will be compiled as subplan in codegen)
                            let aggregator_hir =
                                self.analyze_node(parameters[0].clone(), None, None, next_depth)?;

                            // Analyze optional init_value expression
                            let init_value_hir = if parameters.len() == 2 {
                                Some(Box::new(self.analyze_node(
                                    parameters[1].clone(),
                                    None,
                                    None,
                                    next_depth,
                                )?))
                            } else {
                                None
                            };

                            // Create Aggregate node - aggregator_plan_id will be set during codegen
                            return Ok(HirNode::Aggregate {
                                collection: Box::new(base_hir),
                                aggregator_hir: Box::new(aggregator_hir),
                                init_value_hir,
                                aggregator_plan_id: 0, // Placeholder, set in codegen
                                result_ty: ExprType::unknown(),
                            });
                        }

                        // Regular function call in path context (e.g., 1.not())
                        // Create MethodCall to preserve the base expression
                        let func_id = self
                            .function_registry
                            .resolve(&function_name)
                            .ok_or_else(|| Error::FunctionNotFound(function_name.clone()))?;

                        // Validate argument count
                        self.function_registry
                            .validate_args(func_id, parameters.len())
                            .map_err(Error::InvalidOperation)?;

                        // Analyze parameters
                        // Special handling for is() and as() functions: convert identifier arguments to strings
                        let args: Result<Vec<HirNode>> = if function_name == "is"
                            || function_name == "as"
                            || function_name == "ofType"
                        {
                            parameters
                                .into_iter()
                                .map(|p| {
                                    // Helper to extract identifier from AST node (handles both simple and qualified identifiers)
                                    fn extract_identifier(ast: &AstNode) -> Option<String> {
                                        match ast {
                                            AstNode::InvocationTerm { invocation } => {
                                                if let AstNode::MemberInvocation { identifier } =
                                                    invocation.as_ref()
                                                {
                                                    Some(identifier.clone())
                                                } else {
                                                    None
                                                }
                                            }
                                            AstNode::TermExpression { term } => {
                                                extract_identifier(term)
                                            }
                                            // Handle qualified identifiers like System.Integer
                                            AstNode::InvocationExpression {
                                                expression,
                                                invocation,
                                            } => {
                                                // Get the left side (namespace)
                                                let left = extract_identifier(expression.as_ref())?;
                                                // Get the right side (type name)
                                                if let AstNode::MemberInvocation { identifier } =
                                                    invocation.as_ref()
                                                {
                                                    Some(format!("{}.{}", left, identifier))
                                                } else {
                                                    None
                                                }
                                            }
                                            _ => None,
                                        }
                                    }

                                    // If parameter is an identifier, convert to string literal
                                    if let Some(identifier) = extract_identifier(&p) {
                                        return Ok(HirNode::Literal {
                                            value: crate::value::Value::string(identifier),
                                            ty: self.type_registry.expr_from_system_type(
                                                TypeId::String,
                                                Cardinality::ONE_TO_ONE,
                                            ),
                                        });
                                    }
                                    // Otherwise, analyze normally
                                    self.analyze_node(p, None, None, next_depth)
                                })
                                .collect()
                        } else {
                            parameters
                                .into_iter()
                                .map(|p| self.analyze_node(p, None, None, next_depth))
                                .collect()
                        };
                        let args = args?;

                        // Structural-only: type pass assigns a concrete return type.
                        let result_ty = ExprType::unknown();

                        // Create MethodCall with base preserved
                        return Ok(HirNode::MethodCall {
                            base: Box::new(base_hir),
                            func_id,
                            args,
                            result_ty,
                        });
                    }
                    AstNode::ThisInvocation
                    | AstNode::IndexInvocation
                    | AstNode::TotalInvocation => {
                        // These are handled separately
                        return self.analyze_node(
                            *invocation,
                            base_type_id,
                            base_type_name.clone(),
                            next_depth,
                        );
                    }
                    _ => {
                        // Other invocations not supported in path context
                        return Err(Error::InvalidOperation(
                            "Unsupported invocation in path".into(),
                        ));
                    }
                }

                // Structural-only: type pass assigns a concrete result type.
                let result_ty = ExprType::unknown();

                Ok(HirNode::Path {
                    base: Box::new(base_hir),
                    segments,
                    result_ty,
                })
            }

            AstNode::IndexerExpression { collection, index } => {
                let collection_hir = self.analyze_node(
                    *collection,
                    base_type_id,
                    base_type_name.clone(),
                    next_depth,
                )?;

                // Extract constant index value if present
                fn extract_index(ast: &AstNode) -> Option<usize> {
                    match ast {
                        AstNode::IntegerLiteral(i) | AstNode::LongNumberLiteral(i) => {
                            if *i < 0 {
                                None
                            } else {
                                Some(*i as usize)
                            }
                        }
                        AstNode::NumberLiteral(d) => {
                            use rust_decimal::Decimal;
                            if d.is_sign_negative() || d.fract() != Decimal::ZERO {
                                None
                            } else {
                                d.to_usize()
                            }
                        }
                        AstNode::LiteralTerm { literal } => extract_index(literal),
                        AstNode::ParenthesizedTerm { expression } => extract_index(expression),
                        AstNode::TermExpression { term } => extract_index(term),
                        AstNode::InvocationTerm { invocation } => extract_index(invocation),
                        AstNode::PolarityExpression {
                            operator,
                            expression,
                        } => {
                            let val = extract_index(expression)?;
                            match operator {
                                crate::ast::PolarityOperator::Plus => Some(val),
                                crate::ast::PolarityOperator::Minus => None, // Negative indices invalid
                            }
                        }
                        _ => None,
                    }
                }

                let idx_value = extract_index(&index).ok_or_else(|| {
                    Error::InvalidOperation(
                        "Indexer requires a non-negative integer literal".into(),
                    )
                })?;

                // Result type is element type of collection
                let result_ty = collection_hir
                    .result_type()
                    .unwrap_or_else(ExprType::unknown);

                // For now, treat as path navigation with index
                Ok(HirNode::Path {
                    base: Box::new(collection_hir),
                    segments: vec![PathSegmentHir::Index(idx_value)],
                    result_ty,
                })
            }

            // ============================================
            // Binary Operators
            // ============================================
            AstNode::EqualityExpression {
                left,
                operator,
                right,
            } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let op = match operator {
                    crate::ast::EqualityOperator::Equal => HirBinaryOperator::Eq,
                    crate::ast::EqualityOperator::NotEqual => HirBinaryOperator::Ne,
                    crate::ast::EqualityOperator::Equivalent => HirBinaryOperator::Equivalent,
                    crate::ast::EqualityOperator::NotEquivalent => HirBinaryOperator::NotEquivalent,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0, // TODO: Resolve implementation based on operand types
                    result_ty,
                })
            }
            AstNode::InequalityExpression {
                left,
                operator,
                right,
            } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let op = match operator {
                    crate::ast::InequalityOperator::LessThan => HirBinaryOperator::Lt,
                    crate::ast::InequalityOperator::LessThanOrEqual => HirBinaryOperator::Le,
                    crate::ast::InequalityOperator::GreaterThan => HirBinaryOperator::Gt,
                    crate::ast::InequalityOperator::GreaterThanOrEqual => HirBinaryOperator::Ge,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::AdditiveExpression {
                left,
                operator,
                right,
            } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let op = match operator {
                    crate::ast::AdditiveOperator::Plus => HirBinaryOperator::Add,
                    crate::ast::AdditiveOperator::Minus => HirBinaryOperator::Sub,
                    crate::ast::AdditiveOperator::Concat => HirBinaryOperator::Concat,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::MultiplicativeExpression {
                left,
                operator,
                right,
            } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let op = match operator {
                    crate::ast::MultiplicativeOperator::Multiply => HirBinaryOperator::Mul,
                    crate::ast::MultiplicativeOperator::Divide => HirBinaryOperator::Div,
                    crate::ast::MultiplicativeOperator::Div => HirBinaryOperator::DivInt,
                    crate::ast::MultiplicativeOperator::Mod => HirBinaryOperator::Mod,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::AndExpression { left, right } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op: HirBinaryOperator::And,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::OrExpression {
                left,
                operator,
                right,
            } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let op = match operator {
                    crate::ast::OrOperator::Or => HirBinaryOperator::Or,
                    crate::ast::OrOperator::Xor => HirBinaryOperator::Xor,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::ImpliesExpression { left, right } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op: HirBinaryOperator::Implies,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::UnionExpression { left, right } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op: HirBinaryOperator::Union,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }
            AstNode::MembershipExpression {
                left,
                operator,
                right,
            } => {
                let left_hir =
                    self.analyze_node(*left, base_type_id, base_type_name.clone(), next_depth)?;
                let right_hir =
                    self.analyze_node(*right, base_type_id, base_type_name.clone(), next_depth)?;
                let op = match operator {
                    crate::ast::MembershipOperator::In => HirBinaryOperator::In,
                    crate::ast::MembershipOperator::Contains => HirBinaryOperator::Contains,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::BinaryOp {
                    op,
                    left: Box::new(left_hir),
                    right: Box::new(right_hir),
                    impl_id: 0,
                    result_ty,
                })
            }

            // ============================================
            // Unary Operators
            // ============================================
            AstNode::PolarityExpression {
                operator,
                expression,
            } => {
                // Guard invalid unparenthesized negative literal invocations: e.g., -1.convertsToInteger()
                if operator == crate::ast::PolarityOperator::Minus {
                    if let AstNode::InvocationExpression {
                        expression: inner_base,
                        invocation: inner_inv,
                    } = expression.as_ref()
                    {
                        let is_literal = matches!(
                            inner_base.as_ref(),
                            AstNode::LiteralTerm { .. }
                                | AstNode::IntegerLiteral(_)
                                | AstNode::NumberLiteral(_)
                                | AstNode::LongNumberLiteral(_)
                        );
                        if let AstNode::FunctionInvocation { function_name, .. } =
                            inner_inv.as_ref()
                        {
                            if is_literal
                                && (function_name == "convertsToInteger"
                                    || function_name == "convertsToDecimal")
                            {
                                return Err(Error::InvalidOperation(
                                    "Invalid operation: negative literal invocation requires parentheses"
                                        .into(),
                                ));
                            }
                        }
                    }
                }
                let expr_hir = self.analyze_node(
                    *expression,
                    base_type_id,
                    base_type_name.clone(),
                    next_depth,
                )?;
                let op = match operator {
                    crate::ast::PolarityOperator::Plus => HirUnaryOperator::Plus,
                    crate::ast::PolarityOperator::Minus => HirUnaryOperator::Minus,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::UnaryOp {
                    op,
                    expr: Box::new(expr_hir),
                    result_ty,
                })
            }

            // ============================================
            // Type Operations
            // ============================================
            AstNode::TypeExpression {
                expression,
                operator,
                type_specifier,
            } => {
                let expr_hir = self.analyze_node(
                    *expression,
                    base_type_id,
                    base_type_name.clone(),
                    next_depth,
                )?;
                let op = match operator {
                    crate::ast::TypeOperator::Is => HirTypeOperator::Is,
                    crate::ast::TypeOperator::As => HirTypeOperator::As,
                };
                let result_ty = ExprType::unknown();

                Ok(HirNode::TypeOp {
                    op,
                    expr: Box::new(expr_hir),
                    type_specifier: type_specifier.to_string(),
                    result_ty,
                })
            }
        }
    }
}

fn is_negative_numeric_literal(ast: &AstNode) -> bool {
    match ast {
        AstNode::LiteralTerm { literal } => is_negative_numeric_literal(literal),
        AstNode::IntegerLiteral(v) => *v < 0,
        AstNode::NumberLiteral(v) => v.is_sign_negative(),
        AstNode::LongNumberLiteral(v) => *v < 0,
        _ => false,
    }
}

pub fn is_fhir_type(context: &Arc<dyn FhirContext>, type_name: &str) -> bool {
    matches!(
        context.get_core_structure_definition_by_type(type_name),
        Ok(Some(_))
    )
}
