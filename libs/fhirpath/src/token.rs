//! Token types for the FHIRPath lexer
//!
//! Tokens represent the lexical elements of FHIRPath expressions.

/// Token types for the FHIRPath lexer
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum TokenType {
    // Literals
    StringLiteral,
    NumberLiteral,
    LongNumberLiteral,
    DateLiteral,
    DateTimeLiteral,
    TimeLiteral,
    BooleanLiteral,
    NullLiteral,

    // Identifiers
    Identifier,
    DelimitedIdentifier,

    // Keywords
    True,
    False,
    As,
    Is,
    Div,
    Mod,
    In,
    Contains,
    And,
    Or,
    Xor,
    Implies,
    This,  // $this
    Index, // $index
    Total, // $total

    // External constant
    ExternalConstant, // %identifier or %STRING

    // Operators
    Dot,                // .
    OpenBracket,        // [
    CloseBracket,       // ]
    Plus,               // +
    Minus,              // -
    Multiply,           // *
    Divide,             // /
    Ampersand,          // &
    Pipe,               // |
    LessThanOrEqual,    // <=
    LessThan,           // <
    GreaterThanOrEqual, // >=
    GreaterThan,        // >
    Equal,              // =
    Equivalent,         // ~
    NotEqual,           // !=
    NotEquivalent,      // !~

    // Delimiters
    OpenParen,  // (
    CloseParen, // )
    OpenBrace,  // {
    CloseBrace, // }
    Comma,      // ,

    // Special
    Percent, // %
    At,      // @

    // End of input
    Eof,

    // Error
    Error, // For syntax errors
}

/// A token in the FHIRPath expression
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
    pub position: usize,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(
        token_type: TokenType,
        value: String,
        position: usize,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            token_type,
            value,
            position,
            line,
            column,
        }
    }

    pub fn eof(position: usize, line: usize, column: usize) -> Self {
        Self {
            token_type: TokenType::Eof,
            value: String::new(),
            position,
            line,
            column,
        }
    }

    pub fn error(message: String, position: usize, line: usize, column: usize) -> Self {
        Self {
            token_type: TokenType::Error,
            value: message,
            position,
            line,
            column,
        }
    }
}
