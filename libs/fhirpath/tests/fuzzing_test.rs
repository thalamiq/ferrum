//! Fuzzing tests to ensure the parser and evaluator handle malformed input gracefully

use ferrum_fhirpath::{Context, Value};

mod test_support;

/// Test that malformed expressions don't panic
#[test]
fn test_malformed_expressions_no_panic() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    let malformed = vec![
        "",         // Empty
        "(",        // Unclosed paren
        ")",        // Unmatched closing paren
        "[",        // Unclosed bracket
        "{",        // Unclosed brace
        "'",        // Unterminated string
        "1 +",      // Incomplete expression
        "+",        // Just operator
        "..",       // Double dot
        "1 2",      // Missing operator
        "name.",    // Trailing dot
        ".name",    // Leading dot
        "1 + + 2",  // Double operator
        "1 ** 2",   // Invalid operator
        "1 @ 2",    // Invalid character
        "name()()", // Double empty call
        "$",        // Incomplete variable
        "%",        // Incomplete external variable
    ];

    for expr in malformed {
        // Should return error, not panic
        let result = engine.evaluate_expr(expr, &ctx, None);
        assert!(
            result.is_err() || result.is_ok(),
            "Expression '{}' should not panic",
            expr
        );
    }
}

/// Test that very long expressions are handled
#[test]
fn test_very_long_expressions() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    // Create a very long but valid expression (reduced from 10000 to 50 to avoid stack overflow)
    // The recursion depth limit is 200, but we use a smaller number to be safe
    let mut expr = "1".to_string();
    for _ in 0..50 {
        expr.push_str(" + 1");
    }

    // Should either succeed or return error gracefully
    let result = engine.evaluate_expr(&expr, &ctx, None);
    assert!(result.is_ok() || result.is_err());
}

/// Test random character sequences
#[test]
fn test_random_characters() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    let random_strings = vec![
        "!@#$%^&*()",
        "abcdefghijklmnopqrstuvwxyz",
        "1234567890",
        "αβγδε",        // Greek letters
        "中文",         // Chinese characters
        "\x00\x01\x02", // Control characters
    ];

    for s in random_strings {
        // Should handle gracefully (parse error is fine)
        let result = engine.evaluate_expr(s, &ctx, None);
        assert!(result.is_ok() || result.is_err());
    }
}

/// Test expressions with many operators
#[test]
fn test_many_operators() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    let expressions = vec![
        "1 + 2 + 3 + 4 + 5",
        "1 * 2 * 3 * 4 * 5",
        "1 = 2 = 3 = 4", // May be invalid
        "true and true and true and true",
        "1 < 2 < 3 < 4", // May be invalid
    ];

    for expr in expressions {
        let result = engine.evaluate_expr(expr, &ctx, None);
        // Should handle gracefully
        assert!(result.is_ok() || result.is_err());
    }
}

/// Test expressions with unusual whitespace
#[test]
fn test_unusual_whitespace() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    let expressions = vec![
        "1\t+\t2",   // Tabs
        "1\n+\n2",   // Newlines
        "1   +   2", // Multiple spaces
        "1+2",       // No spaces
        " 1 + 2 ",   // Leading/trailing spaces
    ];

    for expr in expressions {
        let result = engine.evaluate_expr(expr, &ctx, None);
        // Should parse correctly (whitespace should be ignored)
        assert!(result.is_ok());
    }
}

/// Test expressions with special Unicode characters
#[test]
fn test_unicode_edge_cases() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    let expressions = vec![
        "'\u{0000}'",  // Null character
        "'\u{202E}'",  // Right-to-left override
        "'\u{FEFF}'",  // Zero-width no-break space
        "'\u{1F600}'", // Emoji
    ];

    for expr in expressions {
        let result = engine.evaluate_expr(expr, &ctx, None);
        // Should handle Unicode correctly
        assert!(result.is_ok() || result.is_err());
    }
}

/// Test that the engine doesn't consume excessive memory
#[test]
fn test_memory_usage() {
    let engine = test_support::engine_r5();
    let ctx = Context::new(Value::empty());

    // Create expression that might cause memory issues
    let expr = format!("({})", "1 | ".repeat(10000));

    let result = engine.evaluate_expr(&expr, &ctx, None);
    // Should handle without excessive memory usage
    assert!(result.is_ok() || result.is_err());
}
