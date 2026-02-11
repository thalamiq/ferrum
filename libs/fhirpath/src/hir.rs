//! High-level Intermediate Representation (HIR)
//!
//! HIR is a semantically enriched representation that:
//! - Resolves all names (functions, variables)
//! - Attaches static type + cardinality (`ExprType`)
//! - Normalizes operations
//! - Prepares for code generation

use crate::types::ExprType;
use std::sync::Arc;

/// HIR node - semantically analyzed and type-annotated
#[derive(Debug, Clone)]
pub enum HirNode {
    /// Typed literal
    Literal {
        value: crate::value::Value,
        ty: ExprType,
    },

    /// Path navigation with known types
    Path {
        base: Box<HirNode>,
        segments: Vec<PathSegmentHir>,
        result_ty: ExprType,
    },

    /// Function call with resolved signature
    FunctionCall {
        func_id: FunctionId,
        args: Vec<HirNode>,
        result_ty: ExprType,
    },

    /// Method call (function call with explicit base, e.g., `1.not()`)
    MethodCall {
        base: Box<HirNode>,
        func_id: FunctionId,
        args: Vec<HirNode>,
        result_ty: ExprType,
    },

    /// Binary operation with type-specific implementation
    /// Maps to various AST expression types (equality, inequality, arithmetic, etc.)
    BinaryOp {
        op: HirBinaryOperator,
        left: Box<HirNode>,
        right: Box<HirNode>,
        impl_id: BinaryImplId,
        result_ty: ExprType,
    },

    /// Unary operation (polarity, etc.)
    UnaryOp {
        op: HirUnaryOperator,
        expr: Box<HirNode>,
        result_ty: ExprType,
    },

    /// Type operation (is, as)
    TypeOp {
        op: HirTypeOperator,
        expr: Box<HirNode>,
        type_specifier: String,
        result_ty: ExprType,
    },

    /// Variable access (resolved)
    Variable {
        var_id: VariableId,
        ty: ExprType,
        /// Optional debug/name for external constants (e.g., %resource)
        name: Option<Arc<str>>,
    },

    /// Where clause with predicate as subplan
    Where {
        collection: Box<HirNode>,
        predicate_hir: Box<HirNode>, // Predicate expression to compile as subplan
        predicate_plan_id: PlanId,   // Set during codegen
        result_ty: ExprType,
    },

    /// Select clause with projection as subplan
    Select {
        collection: Box<HirNode>,
        projection_hir: Box<HirNode>, // Projection expression to compile as subplan
        projection_plan_id: PlanId,   // Set during codegen
        result_ty: ExprType,
    },

    /// Repeat clause with projection as subplan
    Repeat {
        collection: Box<HirNode>,
        projection_hir: Box<HirNode>, // Projection expression to compile as subplan
        projection_plan_id: PlanId,   // Set during codegen
        result_ty: ExprType,
    },

    /// Aggregate clause with aggregator subplan
    Aggregate {
        collection: Box<HirNode>,
        aggregator_hir: Box<HirNode>, // Aggregator expression to compile as subplan
        init_value_hir: Option<Box<HirNode>>, // Optional initial value expression
        aggregator_plan_id: PlanId,   // Set during codegen
        result_ty: ExprType,
    },

    /// exists() with optional predicate subplan
    Exists {
        collection: Box<HirNode>,
        predicate_hir: Option<Box<HirNode>>, // Optional predicate expression compiled as subplan
        predicate_plan_id: PlanId,           // Set during codegen
        result_ty: ExprType,
    },

    /// all() with required predicate subplan
    All {
        collection: Box<HirNode>,
        predicate_hir: Box<HirNode>,
        predicate_plan_id: PlanId, // Set during codegen
        result_ty: ExprType,
    },
}

impl HirNode {
    /// Get the result type of this HIR node
    pub fn result_type(&self) -> Option<ExprType> {
        match self {
            HirNode::Literal { ty, .. } => Some(ty.clone()),
            HirNode::Path { result_ty, .. } => Some(result_ty.clone()),
            HirNode::FunctionCall { result_ty, .. } => Some(result_ty.clone()),
            HirNode::MethodCall { result_ty, .. } => Some(result_ty.clone()),
            HirNode::BinaryOp { result_ty, .. } => Some(result_ty.clone()),
            HirNode::UnaryOp { result_ty, .. } => Some(result_ty.clone()),
            HirNode::TypeOp { result_ty, .. } => Some(result_ty.clone()),
            HirNode::Variable { ty, .. } => Some(ty.clone()),
            HirNode::Where { result_ty, .. } => Some(result_ty.clone()),
            HirNode::Select { result_ty, .. } => Some(result_ty.clone()),
            HirNode::Repeat { result_ty, .. } => Some(result_ty.clone()),
            HirNode::Aggregate { result_ty, .. } => Some(result_ty.clone()),
            HirNode::Exists { result_ty, .. } => Some(result_ty.clone()),
            HirNode::All { result_ty, .. } => Some(result_ty.clone()),
        }
    }
}

/// Path segment in HIR (may include type information)
#[derive(Debug, Clone)]
pub enum PathSegmentHir {
    Field(String),
    Index(usize),
    Choice(String), // Resolved choice type
}

impl PathSegmentHir {
    /// Get field name if this is a Field segment
    pub fn as_field(&self) -> Option<&str> {
        match self {
            PathSegmentHir::Field(name) => Some(name),
            _ => None,
        }
    }
}

/// HIR binary operators (normalized from AST)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinaryOperator {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    DivInt, // div
    Mod,
    // Comparison
    Eq,
    Ne,
    Equivalent,    // ~
    NotEquivalent, // !~
    Lt,
    Le,
    Gt,
    Ge,
    // Boolean
    And,
    Or,
    Xor,
    Implies,
    // Collection
    Union,
    In,
    Contains,
    // String
    Concat, // &
}

/// HIR unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOperator {
    Plus,  // +
    Minus, // -
}

/// HIR type operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirTypeOperator {
    Is, // is
    As, // as
}

/// Type aliases for IDs
pub type FunctionId = u16;
pub type VariableId = u16;
pub type BinaryImplId = u16;
pub type PlanId = usize;
