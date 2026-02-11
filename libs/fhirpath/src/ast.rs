//! Abstract Syntax Tree (AST) representation
//!
//! The AST mirrors the FHIRPath grammar structure directly, without semantic analysis.
//! Based on the official FHIRPath ANTLR grammar (fhirpath.g4).
//!
//! # Grammar Coverage
//!
//! This AST fully supports all grammar rules:
//!
//! ## Expression Rules (entireExpression → expression)
//! - ✅ TermExpression: `term`
//! - ✅ InvocationExpression: `expression '.' invocation`
//! - ✅ IndexerExpression: `expression '[' expression ']'`
//! - ✅ PolarityExpression: `('+' | '-') expression`
//! - ✅ MultiplicativeExpression: `expression ('*' | '/' | 'div' | 'mod') expression`
//! - ✅ AdditiveExpression: `expression ('+' | '-' | '&') expression`
//! - ✅ TypeExpression: `expression ('is' | 'as') typeSpecifier`
//! - ✅ UnionExpression: `expression '|' expression`
//! - ✅ InequalityExpression: `expression ('<=' | '<' | '>' | '>=') expression`
//! - ✅ EqualityExpression: `expression ('=' | '~' | '!=' | '!~') expression`
//! - ✅ MembershipExpression: `expression ('in' | 'contains') expression`
//! - ✅ AndExpression: `expression 'and' expression`
//! - ✅ OrExpression: `expression ('or' | 'xor') expression`
//! - ✅ ImpliesExpression: `expression 'implies' expression`
//!
//! ## Term Rules
//! - ✅ InvocationTerm: `invocation`
//! - ✅ LiteralTerm: `literal`
//! - ✅ ExternalConstantTerm: `'%' (identifier | STRING)`
//! - ✅ ParenthesizedTerm: `'(' expression ')'`
//!
//! ## Invocation Rules
//! - ✅ MemberInvocation: `identifier`
//! - ✅ FunctionInvocation: `function`
//! - ✅ ThisInvocation: `'$this'`
//! - ✅ IndexInvocation: `'$index'`
//! - ✅ TotalInvocation: `'$total'`
//!
//! ## Literal Rules
//! - ✅ NullLiteral: `'{}'`
//! - ✅ BooleanLiteral: `'true' | 'false'`
//! - ✅ StringLiteral: `STRING`
//! - ✅ NumberLiteral: `NUMBER`
//! - ✅ LongNumberLiteral: `LONGNUMBER`
//! - ✅ DateLiteral: `DATE`
//! - ✅ DateTimeLiteral: `DATETIME`
//! - ✅ TimeLiteral: `TIME`
//! - ✅ QuantityLiteral: `quantity`

use crate::value::{DatePrecision, DateTimePrecision, TimePrecision};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime};
use rust_decimal::Decimal;

/// AST node representing a FHIRPath expression
#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    // ============================================
    // Expression types (from expression rule)
    // ============================================
    /// Term expression: term
    TermExpression { term: Box<AstNode> },

    /// Invocation expression: expression '.' invocation
    InvocationExpression {
        expression: Box<AstNode>,
        invocation: Box<AstNode>,
    },

    /// Indexer expression: expression '[' expression ']'
    IndexerExpression {
        collection: Box<AstNode>,
        index: Box<AstNode>,
    },

    /// Polarity expression: ('+' | '-') expression
    PolarityExpression {
        operator: PolarityOperator,
        expression: Box<AstNode>,
    },

    /// Multiplicative expression: expression ('*' | '/' | 'div' | 'mod') expression
    MultiplicativeExpression {
        left: Box<AstNode>,
        operator: MultiplicativeOperator,
        right: Box<AstNode>,
    },

    /// Additive expression: expression ('+' | '-' | '&') expression
    AdditiveExpression {
        left: Box<AstNode>,
        operator: AdditiveOperator,
        right: Box<AstNode>,
    },

    /// Type expression: expression ('is' | 'as') typeSpecifier
    TypeExpression {
        expression: Box<AstNode>,
        operator: TypeOperator,
        type_specifier: QualifiedIdentifier,
    },

    /// Union expression: expression '|' expression
    UnionExpression {
        left: Box<AstNode>,
        right: Box<AstNode>,
    },

    /// Inequality expression: expression ('<=' | '<' | '>' | '>=') expression
    InequalityExpression {
        left: Box<AstNode>,
        operator: InequalityOperator,
        right: Box<AstNode>,
    },

    /// Equality expression: expression ('=' | '~' | '!=' | '!~') expression
    EqualityExpression {
        left: Box<AstNode>,
        operator: EqualityOperator,
        right: Box<AstNode>,
    },

    /// Membership expression: expression ('in' | 'contains') expression
    MembershipExpression {
        left: Box<AstNode>,
        operator: MembershipOperator,
        right: Box<AstNode>,
    },

    /// And expression: expression 'and' expression
    AndExpression {
        left: Box<AstNode>,
        right: Box<AstNode>,
    },

    /// Or expression: expression ('or' | 'xor') expression
    OrExpression {
        left: Box<AstNode>,
        operator: OrOperator,
        right: Box<AstNode>,
    },

    /// Implies expression: expression 'implies' expression
    ImpliesExpression {
        left: Box<AstNode>,
        right: Box<AstNode>,
    },

    // ============================================
    // Term types (from term rule)
    // ============================================
    /// Invocation term: invocation
    InvocationTerm { invocation: Box<AstNode> },

    /// Literal term: literal
    LiteralTerm { literal: Box<AstNode> },

    /// External constant term: '%' (identifier | STRING)
    ExternalConstantTerm { constant: String },

    /// Parenthesized term: '(' expression ')'
    ParenthesizedTerm { expression: Box<AstNode> },

    // ============================================
    // Invocation types (from invocation rule)
    // ============================================
    /// Member invocation: identifier
    MemberInvocation { identifier: String },

    /// Function invocation: function
    FunctionInvocation {
        function_name: String,
        parameters: Vec<AstNode>,
    },

    /// This invocation: '$this'
    ThisInvocation,

    /// Index invocation: '$index'
    IndexInvocation,

    /// Total invocation: '$total'
    TotalInvocation,

    // ============================================
    // Literal types (from literal rule)
    // ============================================
    /// Null literal: '{}'
    NullLiteral,

    /// Boolean literal: 'true' | 'false'
    BooleanLiteral(bool),

    /// String literal: STRING
    StringLiteral(String),

    /// Integer literal: NUMBER without decimal point
    IntegerLiteral(i64),

    /// Number literal: NUMBER (with decimal point)
    NumberLiteral(Decimal),

    /// Long number literal: LONGNUMBER
    LongNumberLiteral(i64),

    /// Date literal: DATE
    DateLiteral(NaiveDate, DatePrecision),

    /// DateTime literal: DATETIME with precision
    /// DateTime literal with optional timezone offset seconds east of UTC.
    /// `None` means no timezone was specified in the literal.
    DateTimeLiteral(DateTime<FixedOffset>, DateTimePrecision, Option<i32>),

    /// Time literal: TIME with precision
    TimeLiteral(NaiveTime, TimePrecision),

    /// Quantity literal: quantity
    QuantityLiteral {
        value: Decimal,
        unit: Option<String>,
    },

    /// Collection literal: '{' expression (',' expression)* '}'
    CollectionLiteral { elements: Vec<AstNode> },
}

/// Qualified identifier: identifier ('.' identifier)*
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualifiedIdentifier {
    pub parts: Vec<String>,
}

impl QualifiedIdentifier {
    pub fn new(parts: Vec<String>) -> Self {
        Self { parts }
    }

    pub fn single(name: String) -> Self {
        Self { parts: vec![name] }
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.parts.join(".")
    }
}

// ============================================
// Operator types
// ============================================

/// Polarity operator: '+' | '-'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolarityOperator {
    Plus,  // +
    Minus, // -
}

/// Multiplicative operator: '*' | '/' | 'div' | 'mod'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiplicativeOperator {
    Multiply, // *
    Divide,   // /
    Div,      // div
    Mod,      // mod
}

/// Additive operator: '+' | '-' | '&'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdditiveOperator {
    Plus,   // +
    Minus,  // -
    Concat, // &
}

/// Type operator: 'is' | 'as'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeOperator {
    Is, // is
    As, // as
}

/// Inequality operator: '<=' | '<' | '>' | '>='
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InequalityOperator {
    LessThanOrEqual,    // <=
    LessThan,           // <
    GreaterThan,        // >
    GreaterThanOrEqual, // >=
}

/// Equality operator: '=' | '~' | '!=' | '!~'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualityOperator {
    Equal,         // =
    Equivalent,    // ~
    NotEqual,      // !=
    NotEquivalent, // !~
}

/// Membership operator: 'in' | 'contains'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipOperator {
    In,       // in
    Contains, // contains
}

/// Or operator: 'or' | 'xor'
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrOperator {
    Or,  // or
    Xor, // xor
}

// ============================================
// Legacy/compatibility types (for easier migration)
// ============================================

/// Binary operators (legacy - maps to specific expression types)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[deprecated(note = "Use specific expression types instead")]
pub enum BinaryOperator {
    // Arithmetic
    Add, // +
    Sub, // -
    Mul, // *
    Div, // /
    Mod, // mod

    // Comparison
    Eq, // =
    Ne, // !=
    Lt, // <
    Le, // <=
    Gt, // >
    Ge, // >=

    // Boolean
    And,     // and
    Or,      // or
    Implies, // implies

    // Collection
    Union,    // |
    In,       // in
    Contains, // contains
}

/// Unary operators (legacy - maps to PolarityExpression)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[deprecated(note = "Use PolarityExpression instead")]
pub enum UnaryOperator {
    Not, // not
    Neg, // -
}
