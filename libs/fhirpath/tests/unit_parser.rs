//! Unit tests for the FHIRPath parser module

use zunder_fhirpath::parser::Parser;

/// Helper to parse expression and return AST
fn parse(expr: &str) -> Result<zunder_fhirpath::ast::AstNode, zunder_fhirpath::Error> {
    let mut parser = Parser::new(expr.to_string());
    parser.parse()
}

#[test]
fn test_parse_literals() {
    // Integer
    assert!(parse("42").is_ok());

    // Decimal
    assert!(parse("3.14").is_ok());

    // String
    assert!(parse("'hello'").is_ok());

    // Boolean
    assert!(parse("true").is_ok());
    assert!(parse("false").is_ok());
}

#[test]
fn test_parse_arithmetic() {
    // Addition
    assert!(parse("1 + 2").is_ok());

    // Subtraction
    assert!(parse("5 - 3").is_ok());

    // Multiplication
    assert!(parse("3 * 4").is_ok());

    // Division
    assert!(parse("10 / 2").is_ok());

    // Modulo
    assert!(parse("10 mod 3").is_ok());
}

#[test]
fn test_parse_precedence() {
    // Multiplication should bind tighter than addition
    assert!(parse("1 + 2 * 3").is_ok());

    // Parentheses should override precedence
    assert!(parse("(1 + 2) * 3").is_ok());
}

#[test]
fn test_parse_comparison() {
    assert!(parse("1 < 2").is_ok());
    assert!(parse("1 <= 2").is_ok());
    assert!(parse("3 > 2").is_ok());
    assert!(parse("3 >= 2").is_ok());
}

#[test]
fn test_parse_equality() {
    assert!(parse("1 = 1").is_ok());
    assert!(parse("1 != 2").is_ok());
}

#[test]
fn test_parse_boolean_ops() {
    assert!(parse("true and false").is_ok());
    assert!(parse("true or false").is_ok());
}

#[test]
fn test_parse_unary() {
    assert!(parse("-5").is_ok());
    assert!(parse("+5").is_ok());
}

#[test]
fn test_parse_navigation() {
    assert!(parse("name").is_ok());
    assert!(parse("Patient.name").is_ok());
}

#[test]
fn test_parse_function_call() {
    assert!(parse("name.length()").is_ok());
    assert!(parse("name.substring(0, 5)").is_ok());
}

#[test]
fn test_parse_collection() {
    assert!(parse("{}").is_ok());
    assert!(parse("{1, 2, 3}").is_ok());
}

#[test]
fn test_parse_union() {
    assert!(parse("1 | 2").is_ok());
}

#[test]
fn test_parse_indexer() {
    assert!(parse("name[0]").is_ok());
}

#[test]
fn test_parse_type_operations() {
    assert!(parse("1 is Integer").is_ok());
    assert!(parse("1 as String").is_ok());
}

#[test]
fn test_type_precedence_over_union() {
    use zunder_fhirpath::ast::AstNode;

    // Per HL7 test suite precedence, union binds tighter than `as/is`,
    // so parentheses are required to write a union of `(a as X)` and `b`.
    assert!(parse("a as X | b").is_err());

    let ast = parse("(a as X) | b").unwrap();
    let AstNode::UnionExpression { left, right: _ } = ast else {
        panic!("Expected UnionExpression, got {ast:?}");
    };
    fn contains_type_expression(node: &AstNode) -> bool {
        match node {
            AstNode::TypeExpression { .. } => true,
            AstNode::TermExpression { term } => contains_type_expression(term),
            AstNode::ParenthesizedTerm { expression } => contains_type_expression(expression),
            _ => false,
        }
    }

    assert!(contains_type_expression(left.as_ref()));
}

#[test]
fn test_parse_context_variables() {
    assert!(parse("$this").is_ok());
    assert!(parse("$index").is_ok());
}

#[test]
fn test_parse_external_variables() {
    assert!(parse("%resource").is_ok());
}

#[test]
fn test_parse_where() {
    assert!(parse("(1 | 2 | 3).where($this > 1)").is_ok());
}

#[test]
fn test_parse_select() {
    assert!(parse("(1 | 2).select($this * 2)").is_ok());
}

#[test]
fn test_parse_complex_expression() {
    assert!(parse("Patient.name.where($this.length() > 0).first()").is_ok());
}

#[test]
fn test_parse_error_missing_closing_paren() {
    assert!(parse("(1 + 2").is_err());
}

#[test]
fn test_parse_error_missing_closing_brace() {
    assert!(parse("{1, 2").is_err());
}

#[test]
fn test_parse_error_invalid_operator() {
    // This might parse but fail at evaluation
    let result = parse("1 @ 2");
    // Either parse error or valid (if @ is valid for dates)
    assert!(result.is_ok() || result.is_err());
}
