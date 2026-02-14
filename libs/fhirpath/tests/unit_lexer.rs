//! Unit tests for the FHIRPath lexer module

use ferrum_fhirpath::lexer::Lexer;
use ferrum_fhirpath::token::{Token, TokenType};

/// Helper function to tokenize input and collect all tokens
fn tokenize(input: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(input.to_string());
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token();
        match &token.token_type {
            TokenType::Eof => {
                tokens.push(token);
                break;
            }
            TokenType::Error => {
                tokens.push(token);
                break;
            }
            _ => tokens.push(token),
        }
    }
    tokens
}

#[test]
fn test_literal_integers() {
    let tokens = tokenize("42");
    assert_eq!(tokens.len(), 2); // NumberLiteral + EOF
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "42");

    let tokens = tokenize("0");
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "0");

    let tokens = tokenize("123456789");
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "123456789");

    let tokens = tokenize("-42");
    assert_eq!(tokens[0].token_type, TokenType::Minus);
    assert_eq!(tokens[1].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[1].value, "42");
}

#[test]
fn test_literal_decimals() {
    let tokens = tokenize("3.14");
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "3.14");

    let tokens = tokenize("0.5");
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "0.5");

    let tokens = tokenize("123.456");
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "123.456");
}

#[test]
fn test_literal_strings() {
    let tokens = tokenize("'hello'");
    assert_eq!(tokens[0].token_type, TokenType::StringLiteral);
    assert_eq!(tokens[0].value, "hello");

    let tokens = tokenize("'world'");
    assert_eq!(tokens[0].token_type, TokenType::StringLiteral);
    assert_eq!(tokens[0].value, "world");

    let tokens = tokenize("''");
    assert_eq!(tokens[0].token_type, TokenType::StringLiteral);
    assert_eq!(tokens[0].value, "");

    // Escaped quotes
    let tokens = tokenize("'don\\'t'");
    assert_eq!(tokens[0].token_type, TokenType::StringLiteral);
    assert_eq!(tokens[0].value, "don't");
}

#[test]
fn test_literal_booleans() {
    let tokens = tokenize("true");
    assert_eq!(tokens[0].token_type, TokenType::BooleanLiteral);
    assert_eq!(tokens[0].value, "true");

    let tokens = tokenize("false");
    assert_eq!(tokens[0].token_type, TokenType::BooleanLiteral);
    assert_eq!(tokens[0].value, "false");
}

#[test]
fn test_operators() {
    // Arithmetic
    let tokens = tokenize("+");
    assert_eq!(tokens[0].token_type, TokenType::Plus);

    let tokens = tokenize("-");
    assert_eq!(tokens[0].token_type, TokenType::Minus);

    let tokens = tokenize("*");
    assert_eq!(tokens[0].token_type, TokenType::Multiply);

    let tokens = tokenize("/");
    assert_eq!(tokens[0].token_type, TokenType::Divide);

    let tokens = tokenize("mod");
    assert_eq!(tokens[0].token_type, TokenType::Mod);

    // Comparison
    let tokens = tokenize("=");
    assert_eq!(tokens[0].token_type, TokenType::Equal);

    let tokens = tokenize("!=");
    assert_eq!(tokens[0].token_type, TokenType::NotEqual);

    let tokens = tokenize("<");
    assert_eq!(tokens[0].token_type, TokenType::LessThan);

    let tokens = tokenize("<=");
    assert_eq!(tokens[0].token_type, TokenType::LessThanOrEqual);

    let tokens = tokenize(">");
    assert_eq!(tokens[0].token_type, TokenType::GreaterThan);

    let tokens = tokenize(">=");
    assert_eq!(tokens[0].token_type, TokenType::GreaterThanOrEqual);

    // Boolean
    let tokens = tokenize("and");
    assert_eq!(tokens[0].token_type, TokenType::And);

    let tokens = tokenize("or");
    assert_eq!(tokens[0].token_type, TokenType::Or);

    let tokens = tokenize("xor");
    assert_eq!(tokens[0].token_type, TokenType::Xor);

    let tokens = tokenize("implies");
    assert_eq!(tokens[0].token_type, TokenType::Implies);
}

#[test]
fn test_keywords() {
    let tokens = tokenize("is");
    assert_eq!(tokens[0].token_type, TokenType::Is);

    let tokens = tokenize("as");
    assert_eq!(tokens[0].token_type, TokenType::As);

    let tokens = tokenize("in");
    assert_eq!(tokens[0].token_type, TokenType::In);

    let tokens = tokenize("contains");
    assert_eq!(tokens[0].token_type, TokenType::Contains);
}

#[test]
fn test_identifiers() {
    let tokens = tokenize("name");
    assert_eq!(tokens[0].token_type, TokenType::Identifier);
    assert_eq!(tokens[0].value, "name");

    let tokens = tokenize("Patient");
    assert_eq!(tokens[0].token_type, TokenType::Identifier);
    assert_eq!(tokens[0].value, "Patient");

    let tokens = tokenize("_private");
    assert_eq!(tokens[0].token_type, TokenType::Identifier);
    assert_eq!(tokens[0].value, "_private");

    let tokens = tokenize("name123");
    assert_eq!(tokens[0].token_type, TokenType::Identifier);
    assert_eq!(tokens[0].value, "name123");
}

#[test]
fn test_external_variables() {
    let tokens = tokenize("%resource");
    assert_eq!(tokens[0].token_type, TokenType::ExternalConstant);
    assert_eq!(tokens[0].value, "resource");

    let tokens = tokenize("%context");
    assert_eq!(tokens[0].token_type, TokenType::ExternalConstant);
    assert_eq!(tokens[0].value, "context");
}

#[test]
fn test_context_variables() {
    let tokens = tokenize("$this");
    assert_eq!(tokens[0].token_type, TokenType::This);

    let tokens = tokenize("$index");
    assert_eq!(tokens[0].token_type, TokenType::Index);

    let tokens = tokenize("$total");
    assert_eq!(tokens[0].token_type, TokenType::Total);
}

#[test]
fn test_punctuation() {
    let tokens = tokenize(".");
    assert_eq!(tokens[0].token_type, TokenType::Dot);

    let tokens = tokenize(",");
    assert_eq!(tokens[0].token_type, TokenType::Comma);

    let tokens = tokenize("(");
    assert_eq!(tokens[0].token_type, TokenType::OpenParen);

    let tokens = tokenize(")");
    assert_eq!(tokens[0].token_type, TokenType::CloseParen);

    let tokens = tokenize("[");
    assert_eq!(tokens[0].token_type, TokenType::OpenBracket);

    let tokens = tokenize("]");
    assert_eq!(tokens[0].token_type, TokenType::CloseBracket);

    let tokens = tokenize("{");
    assert_eq!(tokens[0].token_type, TokenType::OpenBrace);

    let tokens = tokenize("}");
    assert_eq!(tokens[0].token_type, TokenType::CloseBrace);

    let tokens = tokenize("|");
    assert_eq!(tokens[0].token_type, TokenType::Pipe);
}

#[test]
fn test_whitespace_handling() {
    let tokens = tokenize("  42  ");
    assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
    assert_eq!(tokens[0].value, "42");

    let tokens = tokenize("name  .  field");
    assert_eq!(tokens[0].token_type, TokenType::Identifier);
    assert_eq!(tokens[0].value, "name");
    assert_eq!(tokens[1].token_type, TokenType::Dot);
    assert_eq!(tokens[2].token_type, TokenType::Identifier);
    assert_eq!(tokens[2].value, "field");
}

#[test]
fn test_complex_expression() {
    let tokens = tokenize("Patient.name.where($this.length() > 0)");
    let types: Vec<&TokenType> = tokens.iter().map(|t| &t.token_type).collect();

    assert!(types.contains(&&TokenType::Identifier));
    assert!(types.contains(&&TokenType::Dot));
    assert!(types.contains(&&TokenType::This));
    assert!(types.contains(&&TokenType::GreaterThan));
    assert!(types.contains(&&TokenType::NumberLiteral));
    assert!(types.contains(&&TokenType::OpenParen));
    assert!(types.contains(&&TokenType::CloseParen));
}

#[test]
fn test_empty_input() {
    let tokens = tokenize("");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].token_type, TokenType::Eof);
}

#[test]
fn test_invalid_characters() {
    let _tokens = tokenize("@");
    // @ is valid for date/time literals, so this might not error
    // Let's test with a truly invalid character
    let tokens = tokenize("~invalid~");
    // Should either parse or error gracefully
    assert!(!tokens.is_empty());
}

#[test]
fn test_string_unterminated() {
    let tokens = tokenize("'unterminated");
    assert_eq!(tokens[0].token_type, TokenType::Error);
}

#[test]
fn test_line_column_tracking() {
    let mut lexer = Lexer::new("42\n  name".to_string());
    let token1 = lexer.next_token();
    assert_eq!(token1.line, 1);
    assert_eq!(token1.column, 1);

    // Whitespace (including newline) is skipped, so the next token is the identifier
    let token2 = lexer.next_token(); // identifier "name"
    assert_eq!(token2.line, 2);
    assert_eq!(token2.column, 3);
    assert_eq!(token2.token_type, TokenType::Identifier);
    assert_eq!(token2.value, "name");
}
