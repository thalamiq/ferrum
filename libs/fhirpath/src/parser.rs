//! FHIRPath parser - converts string expressions to AST
//!
//! Recursive descent parser that follows the FHIRPath grammar precedence rules.
//! Precedence (lowest to highest):
//! 1. implies
//! 2. or/xor
//! 3. and
//! 4. membership (in, contains)
//! 5. equality (=, ~, !=, !~)
//! 6. inequality (<=, <, >, >=)
//! 7. union (|)
//! 8. additive (+, -, &)
//! 9. multiplicative (*, /, div, mod)
//! 10. polarity (+, -)
//! 11. type (is, as) - applies after arithmetic/union/comparison
//! 12. indexer ([ ])
//! 13. invocation (.)
//! 14. term (invocation, literal, externalConstant, parenthesized)

use crate::ast::*;
use crate::error::{Error, Result};
use crate::lexer::Lexer;
use crate::token::{Token, TokenType};
use chrono::{NaiveDate, NaiveTime, TimeZone};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Parser for FHIRPath expressions
pub struct Parser {
    lexer: Lexer,
    current_token: Option<Token>,
    recursion_depth: usize,
}

const MAX_RECURSION_DEPTH: usize = 200;

impl Parser {
    /// Create a new parser for the given input string
    pub fn new(input: String) -> Self {
        let mut parser = Self {
            lexer: Lexer::new(input),
            current_token: None,
            recursion_depth: 0,
        };
        parser.advance();
        parser
    }

    /// Advance to the next token
    fn advance(&mut self) {
        self.current_token = Some(self.lexer.next_token());

        // Check for error tokens
        if let Some(ref token) = self.current_token {
            if token.token_type == TokenType::Error {
                // Error will be handled when we try to use the token
            }
        }
    }

    /// Get the current token (if any)
    fn current_token(&self) -> Option<&Token> {
        self.current_token.as_ref()
    }

    /// Check if current token matches the given type
    fn current_token_is(&self, token_type: TokenType) -> bool {
        self.current_token()
            .map(|t| t.token_type == token_type)
            .unwrap_or(false)
    }

    /// Check if current token is one of the given types
    fn current_token_is_one_of(&self, types: &[TokenType]) -> bool {
        self.current_token()
            .map(|t| types.contains(&t.token_type))
            .unwrap_or(false)
    }

    /// Expect a specific token type and advance
    fn expect(&mut self, token_type: TokenType) -> Result<Token> {
        match self.current_token.take() {
            Some(token) if token.token_type == token_type => {
                self.advance();
                Ok(token)
            }
            Some(token) => Err(Error::ParseError(format!(
                "Expected {:?}, got {:?} at line {}, column {}",
                token_type, token.token_type, token.line, token.column
            ))),
            None => Err(Error::ParseError(format!(
                "Expected {:?}, but reached end of input",
                token_type
            ))),
        }
    }

    /// Parse the entire expression (top-level entry point)
    pub fn parse(&mut self) -> Result<AstNode> {
        let expr = self.parse_expression()?;

        // Ensure we've consumed all input
        if !self.current_token_is(TokenType::Eof) {
            let token = self.current_token().unwrap();
            return Err(Error::ParseError(format!(
                "Unexpected token {:?} at line {}, column {}",
                token.token_type, token.line, token.column
            )));
        }

        Ok(expr)
    }

    /// Check recursion depth and increment
    fn check_recursion_depth(&mut self) -> Result<()> {
        self.recursion_depth += 1;
        if self.recursion_depth > MAX_RECURSION_DEPTH {
            return Err(Error::ParseError(format!(
                "Expression too deeply nested (max depth: {})",
                MAX_RECURSION_DEPTH
            )));
        }
        Ok(())
    }

    /// Decrement recursion depth
    fn decrement_recursion_depth(&mut self) {
        self.recursion_depth -= 1;
    }

    /// Parse an expression (lowest precedence)
    /// According to grammar: expression : term | expression '.' invocation | ...
    /// So we need to check if it's just a term first, otherwise parse as implies expression
    fn parse_expression(&mut self) -> Result<AstNode> {
        self.check_recursion_depth()?;
        // Try parsing as implies expression (which handles all operators)
        let expr = self.parse_implies_expression()?;
        self.decrement_recursion_depth();

        // If the result is a term (InvocationTerm, LiteralTerm, etc.), wrap it in TermExpression
        // Otherwise, return as-is (it's already an expression type)
        match &expr {
            AstNode::InvocationTerm { .. }
            | AstNode::LiteralTerm { .. }
            | AstNode::ExternalConstantTerm { .. }
            | AstNode::ParenthesizedTerm { .. } => Ok(AstNode::TermExpression {
                term: Box::new(expr),
            }),
            _ => Ok(expr),
        }
    }

    /// Parse implies expression: expression 'implies' expression
    fn parse_implies_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_or_expression()?;

        while self.current_token_is(TokenType::Implies) {
            self.advance(); // Skip 'implies'
            let right = self.parse_or_expression()?;
            left = AstNode::ImpliesExpression {
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse or/xor expression: expression ('or' | 'xor') expression
    fn parse_or_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_and_expression()?;

        while self.current_token_is_one_of(&[TokenType::Or, TokenType::Xor]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::Or => OrOperator::Or,
                TokenType::Xor => OrOperator::Xor,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_and_expression()?;
            left = AstNode::OrExpression {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse and expression: expression 'and' expression
    fn parse_and_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_membership_expression()?;

        while self.current_token_is(TokenType::And) {
            self.advance(); // Skip 'and'
            let right = self.parse_membership_expression()?;
            left = AstNode::AndExpression {
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse membership expression: expression ('in' | 'contains') expression
    fn parse_membership_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_type_expression()?;

        while self.current_token_is_one_of(&[TokenType::In, TokenType::Contains]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::In => MembershipOperator::In,
                TokenType::Contains => MembershipOperator::Contains,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_type_expression()?;
            left = AstNode::MembershipExpression {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse equality expression: expression ('=' | '~' | '!=' | '!~') expression
    fn parse_equality_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_inequality_expression()?;

        while self.current_token_is_one_of(&[
            TokenType::Equal,
            TokenType::Equivalent,
            TokenType::NotEqual,
            TokenType::NotEquivalent,
        ]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::Equal => EqualityOperator::Equal,
                TokenType::Equivalent => EqualityOperator::Equivalent,
                TokenType::NotEqual => EqualityOperator::NotEqual,
                TokenType::NotEquivalent => EqualityOperator::NotEquivalent,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_inequality_expression()?;
            left = AstNode::EqualityExpression {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse inequality expression: expression ('<=' | '<' | '>' | '>=') expression
    fn parse_inequality_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_union_expression()?;

        while self.current_token_is_one_of(&[
            TokenType::LessThan,
            TokenType::LessThanOrEqual,
            TokenType::GreaterThan,
            TokenType::GreaterThanOrEqual,
        ]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::LessThan => InequalityOperator::LessThan,
                TokenType::LessThanOrEqual => InequalityOperator::LessThanOrEqual,
                TokenType::GreaterThan => InequalityOperator::GreaterThan,
                TokenType::GreaterThanOrEqual => InequalityOperator::GreaterThanOrEqual,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_union_expression()?;
            left = AstNode::InequalityExpression {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse union expression: expression '|' expression
    fn parse_union_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_additive_expression()?;

        while self.current_token_is(TokenType::Pipe) {
            self.advance(); // Skip '|'
            let right = self.parse_additive_expression()?;
            left = AstNode::UnionExpression {
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse type expression: expression ('is' | 'as') typeSpecifier
    fn parse_type_expression(&mut self) -> Result<AstNode> {
        // Type operations are parsed after comparison/union/equality expressions.
        // This matches the HL7 FHIRPath test suite precedence cases (e.g. `(1 | 1) is Integer`).
        let mut left = self.parse_equality_expression()?;

        while self.current_token_is_one_of(&[TokenType::Is, TokenType::As]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::Is => TypeOperator::Is,
                TokenType::As => TypeOperator::As,
                _ => unreachable!(),
            };
            self.advance();
            let type_spec = self.parse_qualified_identifier()?;
            left = AstNode::TypeExpression {
                expression: Box::new(left),
                operator: op,
                type_specifier: type_spec,
            };
        }

        Ok(left)
    }

    /// Parse additive expression: expression ('+' | '-' | '&') expression
    fn parse_additive_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_multiplicative_expression()?;

        while self.current_token_is_one_of(&[
            TokenType::Plus,
            TokenType::Minus,
            TokenType::Ampersand,
        ]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::Plus => AdditiveOperator::Plus,
                TokenType::Minus => AdditiveOperator::Minus,
                TokenType::Ampersand => AdditiveOperator::Concat,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_multiplicative_expression()?;
            left = AstNode::AdditiveExpression {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse multiplicative expression: expression ('*' | '/' | 'div' | 'mod') expression
    fn parse_multiplicative_expression(&mut self) -> Result<AstNode> {
        let mut left = self.parse_polarity_expression()?;

        while self.current_token_is_one_of(&[
            TokenType::Multiply,
            TokenType::Divide,
            TokenType::Div,
            TokenType::Mod,
        ]) {
            let token = self.current_token().unwrap();
            let op = match token.token_type {
                TokenType::Multiply => MultiplicativeOperator::Multiply,
                TokenType::Divide => MultiplicativeOperator::Divide,
                TokenType::Div => MultiplicativeOperator::Div,
                TokenType::Mod => MultiplicativeOperator::Mod,
                _ => unreachable!(),
            };
            self.advance();
            let right = self.parse_polarity_expression()?;
            left = AstNode::MultiplicativeExpression {
                left: Box::new(left),
                operator: op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse polarity expression: ('+' | '-') expression
    /// Handles negative literals specially: -5 should be parsed as IntegerLiteral(-5) when followed by a number
    fn parse_polarity_expression(&mut self) -> Result<AstNode> {
        if self.current_token_is_one_of(&[TokenType::Plus, TokenType::Minus]) {
            let token = self.current_token().unwrap();
            let is_minus = token.token_type == TokenType::Minus;
            self.advance();

            // Check if next token is a number literal - if so, parse as negative literal (including quantity)
            if is_minus
                && self.current_token_is_one_of(&[
                    TokenType::NumberLiteral,
                    TokenType::LongNumberLiteral,
                ])
            {
                let num_token_type = self.current_token().unwrap().token_type.clone();
                let num_value = self.current_token().unwrap().value.clone();
                self.advance();

                // Optional unit following the number (quantity literal)
                let unit = if self.current_token_is(TokenType::StringLiteral) {
                    let unit_token = self.expect(TokenType::StringLiteral)?;
                    Some(unit_token.value)
                } else if self.current_token_is(TokenType::Identifier) {
                    if let Some(ref token) = self.current_token {
                        let ident = token.value.clone();
                        if matches!(
                            ident.as_str(),
                            "years"
                                | "year"
                                | "months"
                                | "month"
                                | "weeks"
                                | "week"
                                | "days"
                                | "day"
                                | "hours"
                                | "hour"
                                | "minutes"
                                | "minute"
                                | "seconds"
                                | "second"
                                | "milliseconds"
                                | "millisecond"
                        ) {
                            self.advance();
                            Some(ident)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let negative_literal = match num_token_type {
                    TokenType::NumberLiteral => {
                        let value_str = format!("-{}", num_value);
                        let dec = Decimal::from_str(&value_str)
                            .map_err(|e| Error::ParseError(format!("Invalid number: {}", e)))?;
                        if let Some(u) = unit {
                            AstNode::QuantityLiteral {
                                value: dec,
                                unit: Some(u),
                            }
                        } else if num_value.contains('.') {
                            AstNode::NumberLiteral(dec)
                        } else {
                            let value = i64::from_str(&value_str).map_err(|e| {
                                Error::ParseError(format!("Invalid integer: {}", e))
                            })?;
                            AstNode::IntegerLiteral(value)
                        }
                    }
                    TokenType::LongNumberLiteral => {
                        let num_str = num_value.trim_end_matches('L');
                        let value_str = format!("-{}", num_str);
                        let int_value = i64::from_str(&value_str).map_err(|e| {
                            Error::ParseError(format!("Invalid long number: {}", e))
                        })?;
                        if let Some(u) = unit {
                            let dec = Decimal::from_str(&value_str)
                                .map_err(|e| Error::ParseError(format!("Invalid number: {}", e)))?;
                            AstNode::QuantityLiteral {
                                value: dec,
                                unit: Some(u),
                            }
                        } else {
                            AstNode::LongNumberLiteral(int_value)
                        }
                    }
                    _ => unreachable!(),
                };

                // Wrap in literal term and continue parsing indexer/invocation expressions
                // This allows method calls like -120.highBoundary(2)
                let literal_term = AstNode::LiteralTerm {
                    literal: Box::new(negative_literal),
                };
                // Continue parsing indexer/invocation expressions to handle method calls
                self.parse_indexer_expression_from_term(literal_term)
            } else {
                // Regular polarity expression
                let op = if is_minus {
                    PolarityOperator::Minus
                } else {
                    PolarityOperator::Plus
                };
                let expr = self.parse_polarity_expression()?; // Recursive for multiple unary ops
                Ok(AstNode::PolarityExpression {
                    operator: op,
                    expression: Box::new(expr),
                })
            }
        } else {
            self.parse_indexer_expression()
        }
    }

    /// Parse indexer expression: expression '[' expression ']'
    fn parse_indexer_expression(&mut self) -> Result<AstNode> {
        let expr = self.parse_invocation_expression()?;
        self.parse_indexer_expression_from_expr(expr)
    }

    /// Parse indexer expression starting from an existing expression
    fn parse_indexer_expression_from_expr(&mut self, mut expr: AstNode) -> Result<AstNode> {
        loop {
            if self.current_token_is(TokenType::OpenBracket) {
                self.advance(); // Skip '['
                let index = self.parse_expression()?;
                self.expect(TokenType::CloseBracket)?;
                expr = AstNode::IndexerExpression {
                    collection: Box::new(expr),
                    index: Box::new(index),
                };
                continue;
            }

            if self.current_token_is(TokenType::Dot) {
                // Allow continued navigation after an indexer (e.g., name[0].given)
                self.advance(); // Skip '.'
                let invocation = self.parse_invocation()?;
                expr = AstNode::InvocationExpression {
                    expression: Box::new(expr),
                    invocation: Box::new(invocation),
                };
                continue;
            }

            break;
        }
        Ok(expr)
    }

    /// Parse indexer/invocation expressions starting from a term
    fn parse_indexer_expression_from_term(&mut self, term: AstNode) -> Result<AstNode> {
        // Parse invocation expressions first (method calls like .highBoundary())
        let expr = self.parse_invocation_expression_from_term(term)?;
        // Then parse indexer expressions
        self.parse_indexer_expression_from_expr(expr)
    }

    /// Parse invocation expression starting from a term
    fn parse_invocation_expression_from_term(&mut self, mut expr: AstNode) -> Result<AstNode> {
        while self.current_token_is(TokenType::Dot) {
            self.advance(); // Skip '.'
            let invocation = self.parse_invocation()?;
            expr = AstNode::InvocationExpression {
                expression: Box::new(expr),
                invocation: Box::new(invocation),
            };
        }
        Ok(expr)
    }

    /// Parse invocation expression: expression '.' invocation
    fn parse_invocation_expression(&mut self) -> Result<AstNode> {
        let mut expr = self.parse_term()?;

        while self.current_token_is(TokenType::Dot) {
            self.advance(); // Skip '.'
            let invocation = self.parse_invocation()?;
            expr = AstNode::InvocationExpression {
                expression: Box::new(expr),
                invocation: Box::new(invocation),
            };
        }

        Ok(expr)
    }

    /// Parse a term
    fn parse_term(&mut self) -> Result<AstNode> {
        if self.current_token_is(TokenType::OpenParen) {
            // Parenthesized term: '(' expression ')'
            self.advance(); // Skip '('
            let expr = self.parse_expression()?;
            self.expect(TokenType::CloseParen)?;
            Ok(AstNode::ParenthesizedTerm {
                expression: Box::new(expr),
            })
        } else if self.current_token_is(TokenType::ExternalConstant) {
            // External constant: %identifier or %STRING
            let token = self.current_token().unwrap();
            let value = token.value.clone();
            self.advance();
            Ok(AstNode::ExternalConstantTerm { constant: value })
        } else if self.is_literal_start() {
            // Literal
            let literal = self.parse_literal()?;
            Ok(AstNode::LiteralTerm {
                literal: Box::new(literal),
            })
        } else {
            // Invocation term
            let invocation = self.parse_invocation()?;
            Ok(AstNode::InvocationTerm {
                invocation: Box::new(invocation),
            })
        }
    }

    /// Check if current token starts a literal
    fn is_literal_start(&self) -> bool {
        self.current_token_is_one_of(&[
            TokenType::NullLiteral,
            TokenType::BooleanLiteral,
            TokenType::StringLiteral,
            TokenType::NumberLiteral,
            TokenType::LongNumberLiteral,
            TokenType::DateLiteral,
            TokenType::DateTimeLiteral,
            TokenType::TimeLiteral,
            TokenType::OpenBrace, // {} for null
        ])
    }

    /// Parse a literal
    fn parse_literal(&mut self) -> Result<AstNode> {
        let token = self
            .current_token()
            .ok_or_else(|| Error::ParseError("Expected literal".into()))?;

        match token.token_type {
            TokenType::OpenBrace => {
                self.advance(); // Skip '{'
                                // Check if it's a null literal {} or a collection literal {expr, expr, ...}
                if self.current_token_is(TokenType::CloseBrace) {
                    // Null literal: {}
                    self.advance(); // Skip '}'
                    Ok(AstNode::NullLiteral)
                } else {
                    // Collection literal: {expr, expr, ...}
                    let mut elements = Vec::new();
                    loop {
                        // Parse an expression
                        let expr = self.parse_expression()?;
                        elements.push(expr);

                        // Check if there's a comma (more elements) or closing brace (end)
                        if self.current_token_is(TokenType::Comma) {
                            self.advance(); // Skip ','
                        } else if self.current_token_is(TokenType::CloseBrace) {
                            self.advance(); // Skip '}'
                            break;
                        } else {
                            return Err(Error::ParseError(
                                "Expected ',' or '}' in collection literal".into(),
                            ));
                        }
                    }
                    Ok(AstNode::CollectionLiteral { elements })
                }
            }
            TokenType::BooleanLiteral => {
                let value = token.value == "true";
                self.advance();
                Ok(AstNode::BooleanLiteral(value))
            }
            TokenType::StringLiteral => {
                let value = token.value.clone();
                self.advance();
                Ok(AstNode::StringLiteral(value))
            }
            TokenType::NumberLiteral => {
                let num_value = token.value.clone();
                self.advance();

                // Check for quantity literal: number followed by unit
                // Units can be: 'string' or calendar duration identifiers (years, months, etc.)
                let unit = if self.current_token_is(TokenType::StringLiteral) {
                    // Unit as string literal: 10 'mg'
                    let unit_token = self.expect(TokenType::StringLiteral)?;
                    Some(unit_token.value)
                } else if self.current_token_is(TokenType::Identifier) {
                    // Check if it's a calendar duration unit
                    if let Some(ref token) = self.current_token {
                        let ident = token.value.clone();
                        if matches!(
                            ident.as_str(),
                            "years"
                                | "year"
                                | "months"
                                | "month"
                                | "weeks"
                                | "week"
                                | "days"
                                | "day"
                                | "hours"
                                | "hour"
                                | "minutes"
                                | "minute"
                                | "seconds"
                                | "second"
                                | "milliseconds"
                                | "millisecond"
                        ) {
                            self.advance();
                            Some(ident)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(unit_str) = unit {
                    // It's a quantity literal
                    let value = Decimal::from_str(&num_value)
                        .map_err(|e| Error::ParseError(format!("Invalid number: {}", e)))?;
                    Ok(AstNode::QuantityLiteral {
                        value,
                        unit: Some(unit_str),
                    })
                } else {
                    // Regular number literal
                    if num_value.contains('.') {
                        // Has decimal point - parse as Decimal
                        let value = Decimal::from_str(&num_value)
                            .map_err(|e| Error::ParseError(format!("Invalid number: {}", e)))?;
                        Ok(AstNode::NumberLiteral(value))
                    } else {
                        // No decimal point - parse as Integer
                        let value = i64::from_str(&num_value)
                            .map_err(|e| Error::ParseError(format!("Invalid integer: {}", e)))?;
                        Ok(AstNode::IntegerLiteral(value))
                    }
                }
            }
            TokenType::LongNumberLiteral => {
                // Remove 'L' suffix if present
                let num_str = token.value.trim_end_matches('L');
                let value = i64::from_str(num_str)
                    .map_err(|e| Error::ParseError(format!("Invalid long number: {}", e)))?;
                self.advance();
                Ok(AstNode::LongNumberLiteral(value))
            }
            TokenType::DateLiteral => {
                // Parse date - lexer produces formats like "1974", "1974-12", or "1974-12-25"
                // For partial dates, we normalize to full dates for internal storage, but preserve precision.
                use crate::value::DatePrecision;
                let (value, precision) = if token.value.len() == 4 {
                    // Just year: YYYY -> YYYY-01-01
                    (
                        NaiveDate::parse_from_str(&format!("{}-01-01", token.value), "%Y-%m-%d")
                            .map_err(|e| Error::ParseError(format!("Invalid date: {}", e)))?,
                        DatePrecision::Year,
                    )
                } else if token.value.len() == 7 {
                    // Year-month: YYYY-MM -> YYYY-MM-01
                    (
                        NaiveDate::parse_from_str(&format!("{}-01", token.value), "%Y-%m-%d")
                            .map_err(|e| Error::ParseError(format!("Invalid date: {}", e)))?,
                        DatePrecision::Month,
                    )
                } else if token.value.len() == 10 {
                    // Full date: YYYY-MM-DD
                    (
                        NaiveDate::parse_from_str(&token.value, "%Y-%m-%d")
                            .map_err(|e| Error::ParseError(format!("Invalid date: {}", e)))?,
                        DatePrecision::Day,
                    )
                } else {
                    return Err(Error::ParseError(format!(
                        "Invalid date format length: {}",
                        token.value.len()
                    )));
                };
                self.advance();
                Ok(AstNode::DateLiteral(value, precision))
            }
            TokenType::DateTimeLiteral => {
                // Parse datetime - lexer produces format like:
                // - Full: "1974-12-25T14:30:00" or "1974-12-25T14:30:00Z"
                // - Partial: "2015T" (year only) or "2015-02T" (year-month only)

                use crate::value::DateTimePrecision;
                let raw = token.value.as_str();

                // Detect and parse timezone offset, if present.
                let (core, timezone_offset): (String, Option<i32>) =
                    if let Some(stripped) = raw.strip_suffix('Z') {
                        (stripped.to_string(), Some(0))
                    } else if let Some(t_pos) = raw.find('T') {
                        let after_t = &raw[t_pos + 1..];
                        if let Some(tz_rel) = after_t.find(['+', '-']) {
                            let tz_abs = t_pos + 1 + tz_rel;
                            let core = raw[..tz_abs].to_string();
                            let tz_part = &raw[tz_abs..];

                            let sign = if tz_part.starts_with('-') { -1 } else { 1 };
                            let tz_digits = tz_part.trim_start_matches(['+', '-']);

                            let (hh, mm) = if let Some((h, m)) = tz_digits.split_once(':') {
                                (h, m)
                            } else if tz_digits.len() == 4 {
                                (&tz_digits[0..2], &tz_digits[2..4])
                            } else {
                                return Err(Error::ParseError(format!(
                                    "Invalid timezone offset in datetime literal '{}'",
                                    raw
                                )));
                            };

                            let hours: i32 = hh.parse().map_err(|_| {
                                Error::ParseError(format!("Invalid timezone hour in '{}'", raw))
                            })?;
                            let mins: i32 = mm.parse().map_err(|_| {
                                Error::ParseError(format!("Invalid timezone minute in '{}'", raw))
                            })?;

                            let offset = sign * (hours * 3600 + mins * 60);
                            (core, Some(offset))
                        } else {
                            (raw.to_string(), None)
                        }
                    } else {
                        (raw.to_string(), None)
                    };

                let t_pos = core.find('T').ok_or_else(|| {
                    Error::ParseError(format!("Invalid datetime literal '{}'", raw))
                })?;
                let date_part = &core[..t_pos];
                let time_part = &core[t_pos + 1..];

                let (date, mut precision) = if date_part.len() == 4 {
                    (
                        NaiveDate::parse_from_str(&format!("{}-01-01", date_part), "%Y-%m-%d")
                            .map_err(|e| {
                                Error::ParseError(format!("Invalid datetime '{}': {}", raw, e))
                            })?,
                        DateTimePrecision::Year,
                    )
                } else if date_part.len() == 7 {
                    (
                        NaiveDate::parse_from_str(&format!("{}-01", date_part), "%Y-%m-%d")
                            .map_err(|e| {
                                Error::ParseError(format!("Invalid datetime '{}': {}", raw, e))
                            })?,
                        DateTimePrecision::Month,
                    )
                } else if date_part.len() == 10 {
                    (
                        NaiveDate::parse_from_str(date_part, "%Y-%m-%d").map_err(|e| {
                            Error::ParseError(format!("Invalid datetime '{}': {}", raw, e))
                        })?,
                        DateTimePrecision::Day,
                    )
                } else {
                    return Err(Error::ParseError(format!(
                        "Invalid datetime date component in '{}'",
                        raw
                    )));
                };

                let time = if time_part.is_empty() {
                    NaiveTime::from_hms_opt(0, 0, 0).unwrap()
                } else {
                    let (main, frac) = time_part
                        .split_once('.')
                        .map(|(a, b)| (a, Some(b)))
                        .unwrap_or((time_part, None));

                    let parts: Vec<&str> = main.split(':').collect();
                    let (h, m, s): (&str, &str, &str) = match parts.as_slice() {
                        [hh] => {
                            // FHIR dateTime requires at least minutes; normalize hour-only to minute precision with :00.
                            precision = DateTimePrecision::Minute;
                            (*hh, "0", "0")
                        }
                        [hh, mm] => {
                            precision = DateTimePrecision::Minute;
                            (*hh, *mm, "0")
                        }
                        [hh, mm, ss] => {
                            precision = if frac.is_some() {
                                DateTimePrecision::Millisecond
                            } else {
                                DateTimePrecision::Second
                            };
                            (*hh, *mm, *ss)
                        }
                        _ => {
                            return Err(Error::ParseError(format!(
                                "Invalid datetime time component in '{}'",
                                raw
                            )));
                        }
                    };

                    let hour: u32 = h.parse::<u32>().map_err(|_| {
                        Error::ParseError(format!("Invalid datetime hour in '{}'", raw))
                    })?;
                    let minute: u32 = m.parse::<u32>().map_err(|_| {
                        Error::ParseError(format!("Invalid datetime minute in '{}'", raw))
                    })?;
                    let second: u32 = s.parse::<u32>().map_err(|_| {
                        Error::ParseError(format!("Invalid datetime second in '{}'", raw))
                    })?;

                    let nanos: u32 = if let Some(frac) = frac {
                        let frac_digits: String = frac.chars().take(3).collect();
                        let padded = format!("{:0<3}", frac_digits);
                        let ms: u32 = padded.parse().map_err(|_| {
                            Error::ParseError(format!(
                                "Invalid datetime fractional seconds in '{}'",
                                raw
                            ))
                        })?;
                        ms * 1_000_000
                    } else {
                        0
                    };

                    NaiveTime::from_hms_nano_opt(hour, minute, second, nanos).ok_or_else(|| {
                        Error::ParseError(format!("Invalid datetime time in '{}'", raw))
                    })?
                };

                let dt_naive = date.and_time(time);
                let fixed = chrono::FixedOffset::east_opt(timezone_offset.unwrap_or(0))
                    .ok_or_else(|| {
                        Error::ParseError(format!("Invalid datetime timezone offset in '{}'", raw))
                    })?;
                let value = fixed
                    .from_local_datetime(&dt_naive)
                    .single()
                    .ok_or_else(|| Error::ParseError(format!("Invalid datetime '{}'", raw)))?;

                self.advance();
                Ok(AstNode::DateTimeLiteral(value, precision, timezone_offset))
            }
            TokenType::TimeLiteral => {
                // Parse time - lexer produces format like "14:30:00" or "14:30" or "14"
                // Try various formats, starting with most specific, and track precision
                use crate::value::TimePrecision;

                // Determine precision from format
                let precision = if token.value.contains('.') {
                    TimePrecision::Millisecond // HH:MM:SS.fff (including .000)
                } else if token.value.matches(':').count() >= 2 {
                    TimePrecision::Second // HH:MM:SS
                } else if token.value.contains(':') {
                    TimePrecision::Minute // HH:MM
                } else {
                    TimePrecision::Hour // HH
                };

                let value = NaiveTime::parse_from_str(&token.value, "%H:%M:%S%.f")
                    .or_else(|_| NaiveTime::parse_from_str(&token.value, "%H:%M:%S"))
                    .or_else(|_| NaiveTime::parse_from_str(&token.value, "%H:%M"))
                    .or_else(|_| {
                        // For hour-only format (e.g., "14"), create manually
                        if token.value.len() == 2 && token.value.chars().all(|c| c.is_ascii_digit())
                        {
                            // Just hour: HH
                            if let Ok(hour) = token.value.parse::<u32>() {
                                if hour < 24 {
                                    if let Some(time) = NaiveTime::from_hms_opt(hour, 0, 0) {
                                        return Ok(time);
                                    }
                                }
                            }
                        }
                        // Try standard format or return previous error
                        NaiveTime::parse_from_str(&token.value, "%H")
                    })
                    .map_err(|e| {
                        Error::ParseError(format!("Invalid time format '{}': {}", token.value, e))
                    })?;
                self.advance();
                Ok(AstNode::TimeLiteral(value, precision))
            }
            _ => Err(Error::ParseError(format!(
                "Unexpected token type for literal: {:?}",
                token.token_type
            ))),
        }
    }

    /// Parse an invocation
    fn parse_invocation(&mut self) -> Result<AstNode> {
        if self.current_token_is_one_of(&[TokenType::This, TokenType::Index, TokenType::Total]) {
            let token = self.current_token().unwrap();
            let node = match token.token_type {
                TokenType::This => AstNode::ThisInvocation,
                TokenType::Index => AstNode::IndexInvocation,
                TokenType::Total => AstNode::TotalInvocation,
                _ => unreachable!(),
            };
            self.advance();
            Ok(node)
        } else if self.current_token_is_one_of(&[
            TokenType::Identifier,
            TokenType::DelimitedIdentifier,
            // Operator keywords can also be used as function names (e.g., contains(), in(), as(), is())
            TokenType::Contains,
            TokenType::In,
            TokenType::As,
            TokenType::Is,
        ]) {
            let token = self.current_token().unwrap();
            let ident = token.value.clone();
            self.advance();

            // Check if it's a function call
            if self.current_token_is(TokenType::OpenParen) {
                self.advance(); // Skip '('
                let mut params = Vec::new();

                if !self.current_token_is(TokenType::CloseParen) {
                    loop {
                        params.push(self.parse_expression()?);
                        // fhirpath.g4: sortArgument allows an optional 'asc'|'desc' after each expression.
                        // We accept and consume it here so parsing matches the grammar, but we currently
                        // don't preserve the direction in the AST.
                        if ident == "sort" && self.current_token_is(TokenType::Identifier) {
                            if let Some(token) = self.current_token() {
                                if token.value == "asc" || token.value == "desc" {
                                    self.advance();
                                }
                            }
                        }
                        if self.current_token_is(TokenType::Comma) {
                            self.advance(); // Skip ','
                        } else {
                            break;
                        }
                    }
                }

                self.expect(TokenType::CloseParen)?;
                Ok(AstNode::FunctionInvocation {
                    function_name: ident,
                    parameters: params,
                })
            } else {
                Ok(AstNode::MemberInvocation { identifier: ident })
            }
        } else {
            Err(Error::ParseError(format!(
                "Expected invocation, got {:?}",
                self.current_token().map(|t| &t.token_type)
            )))
        }
    }

    /// Parse a qualified identifier: identifier ('.' identifier)*
    fn parse_qualified_identifier(&mut self) -> Result<QualifiedIdentifier> {
        let mut parts = Vec::new();

        if self.current_token_is_one_of(&[TokenType::Identifier, TokenType::DelimitedIdentifier]) {
            let token = self.current_token().unwrap();
            parts.push(token.value.clone());
            self.advance();

            while self.current_token_is(TokenType::Dot) {
                self.advance(); // Skip '.'
                if self.current_token_is_one_of(&[
                    TokenType::Identifier,
                    TokenType::DelimitedIdentifier,
                ]) {
                    let token = self.current_token().unwrap();
                    parts.push(token.value.clone());
                    self.advance();
                } else {
                    return Err(Error::ParseError("Expected identifier after '.'".into()));
                }
            }
        } else {
            return Err(Error::ParseError("Expected identifier".into()));
        }

        Ok(QualifiedIdentifier::new(parts))
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Result<AstNode> {
        let mut parser = Parser::new(input.to_string());
        parser.parse()
    }

    #[test]
    fn test_simple_identifier() {
        let ast = parse("Patient").unwrap();
        // Top-level expression should be TermExpression wrapping InvocationTerm
        match &ast {
            AstNode::TermExpression { term } => match term.as_ref() {
                AstNode::InvocationTerm { invocation } => {
                    assert!(matches!(
                        invocation.as_ref(),
                        AstNode::MemberInvocation { .. }
                    ));
                }
                _ => panic!("Expected InvocationTerm, got {:?}", term),
            },
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }

    #[test]
    fn test_path_navigation() {
        let ast = parse("Patient.name").unwrap();
        assert!(matches!(ast, AstNode::InvocationExpression { .. }));
    }

    #[test]
    fn test_path_navigation_multiple() {
        let ast = parse("Patient.name.given").unwrap();
        assert!(matches!(ast, AstNode::InvocationExpression { .. }));
    }

    #[test]
    fn test_string_literal() {
        let ast = parse("'hello'").unwrap();
        // Should be wrapped in TermExpression -> LiteralTerm -> StringLiteral
        match &ast {
            AstNode::TermExpression { term } => {
                assert!(matches!(term.as_ref(), AstNode::LiteralTerm { .. }));
            }
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }

    #[test]
    fn test_number_literal() {
        let ast = parse("123").unwrap();
        // Top-level expression is always TermExpression
        match &ast {
            AstNode::TermExpression { term } => {
                // Term should be LiteralTerm containing IntegerLiteral (integers are now distinct from decimals)
                match term.as_ref() {
                    AstNode::LiteralTerm { literal } => {
                        assert!(matches!(literal.as_ref(), AstNode::IntegerLiteral(_)));
                    }
                    _ => panic!("Expected LiteralTerm, got {:?}", term),
                }
            }
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }

        // Test decimal parsing
        let ast = parse("3.14").unwrap();
        match &ast {
            AstNode::TermExpression { term } => match term.as_ref() {
                AstNode::LiteralTerm { literal } => {
                    assert!(matches!(literal.as_ref(), AstNode::NumberLiteral(_)));
                }
                _ => panic!("Expected LiteralTerm, got {:?}", term),
            },
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }

    #[test]
    fn test_boolean_literal() {
        let ast = parse("true").unwrap();
        // Should be wrapped in TermExpression -> LiteralTerm -> BooleanLiteral
        match &ast {
            AstNode::TermExpression { term } => {
                assert!(matches!(term.as_ref(), AstNode::LiteralTerm { .. }));
            }
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }

    #[test]
    fn test_equality() {
        let ast = parse("age = 18").unwrap();
        assert!(matches!(ast, AstNode::EqualityExpression { .. }));
    }

    #[test]
    fn test_inequality() {
        let ast = parse("age > 18").unwrap();
        assert!(matches!(ast, AstNode::InequalityExpression { .. }));
    }

    #[test]
    fn test_and_expression() {
        let ast = parse("age > 18 and age < 65").unwrap();
        assert!(matches!(ast, AstNode::AndExpression { .. }));
    }

    #[test]
    fn test_or_expression() {
        let ast = parse("age < 18 or age > 65").unwrap();
        assert!(matches!(ast, AstNode::OrExpression { .. }));
    }

    #[test]
    fn test_function_call() {
        let ast = parse("name.exists()").unwrap();
        assert!(matches!(ast, AstNode::InvocationExpression { .. }));
    }

    #[test]
    fn test_function_call_with_args() {
        let ast = parse("name.where(given = 'John')").unwrap();
        assert!(matches!(ast, AstNode::InvocationExpression { .. }));
    }

    #[test]
    fn test_literal_method_call() {
        let ast = parse("1.empty()").unwrap();
        assert!(matches!(ast, AstNode::InvocationExpression { .. }));
    }

    #[test]
    fn test_parentheses() {
        let ast = parse("(age + 5) * 2").unwrap();
        assert!(matches!(ast, AstNode::MultiplicativeExpression { .. }));
    }

    #[test]
    fn test_indexer() {
        let ast = parse("name[0]").unwrap();
        assert!(matches!(ast, AstNode::IndexerExpression { .. }));
    }

    #[test]
    fn test_external_constant() {
        let ast = parse("%resource").unwrap();
        // Should be wrapped in TermExpression -> ExternalConstantTerm
        match &ast {
            AstNode::TermExpression { term } => {
                assert!(matches!(
                    term.as_ref(),
                    AstNode::ExternalConstantTerm { .. }
                ));
            }
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }

    #[test]
    fn test_this_variable() {
        let ast = parse("$this").unwrap();
        // Top-level expression is always TermExpression
        match &ast {
            AstNode::TermExpression { term } => {
                // Term should be InvocationTerm containing ThisInvocation
                match term.as_ref() {
                    AstNode::InvocationTerm { invocation } => {
                        assert!(matches!(invocation.as_ref(), AstNode::ThisInvocation));
                    }
                    _ => panic!("Expected InvocationTerm, got {:?}", term),
                }
            }
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }

    #[test]
    fn test_arithmetic() {
        let ast = parse("age + 5").unwrap();
        assert!(matches!(ast, AstNode::AdditiveExpression { .. }));
    }

    #[test]
    fn test_precedence() {
        // Should parse as (age + 5) * 2, not age + (5 * 2)
        let ast = parse("age + 5 * 2").unwrap();
        assert!(matches!(ast, AstNode::AdditiveExpression { .. }));
    }

    #[test]
    fn test_null_literal() {
        let ast = parse("{}").unwrap();
        // Should be wrapped in TermExpression -> LiteralTerm -> NullLiteral
        match &ast {
            AstNode::TermExpression { term } => {
                assert!(matches!(term.as_ref(), AstNode::LiteralTerm { .. }));
            }
            _ => panic!("Expected TermExpression, got {:?}", ast),
        }
    }
}
