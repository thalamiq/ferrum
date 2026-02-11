//! FHIRPath lexer - tokenizes input strings
//!
//! Converts FHIRPath expression strings into a stream of tokens.
//! Handles all lexical rules from the FHIRPath grammar.

use crate::error::{Error, Result};
use crate::token::{Token, TokenType};

/// The FHIRPath lexer
pub struct Lexer {
    #[allow(dead_code)]
    input: String,
    position: usize,
    line: usize,
    column: usize,
    chars: Vec<char>,
    current_char: Option<char>,
}

impl Lexer {
    /// Create a new lexer for the given input
    pub fn new(input: String) -> Self {
        let chars: Vec<char> = input.chars().collect();
        let current_char = chars.first().copied();

        Self {
            input,
            position: 0,
            line: 1,
            column: 1,
            chars,
            current_char,
        }
    }

    /// Advance to the next character
    fn advance(&mut self) {
        if let Some(c) = self.current_char {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.position += 1;
        if self.position < self.chars.len() {
            self.current_char = Some(self.chars[self.position]);
        } else {
            self.current_char = None;
        }
    }

    /// Peek at the next character without advancing
    fn peek(&self) -> Option<char> {
        if self.position + 1 < self.chars.len() {
            Some(self.chars[self.position + 1])
        } else {
            None
        }
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current_char {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip comments (both // and /* */)
    fn skip_comment(&mut self) -> Result<()> {
        if self.current_char == Some('/') && self.peek() == Some('/') {
            // Line comment: //
            self.advance(); // Skip first '/'
            self.advance(); // Skip second '/'

            // Skip until end of line
            while let Some(c) = self.current_char {
                if c == '\n' {
                    self.advance();
                    break;
                }
                self.advance();
            }
            Ok(())
        } else if self.current_char == Some('/') && self.peek() == Some('*') {
            // Block comment: /* */
            self.advance(); // Skip '/'
            self.advance(); // Skip '*'

            // Skip until */
            let mut found_end = false;
            while let Some(c) = self.current_char {
                if c == '*' && self.peek() == Some('/') {
                    self.advance(); // Skip '*'
                    self.advance(); // Skip '/'
                    found_end = true;
                    break;
                }
                self.advance();
            }

            if !found_end {
                return Err(Error::ParseError("Unterminated block comment".into()));
            }
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Read an identifier
    fn read_identifier(&mut self) -> String {
        let start_pos = self.position;

        while let Some(c) = self.current_char {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        self.chars[start_pos..self.position].iter().collect()
    }

    /// Read a delimited identifier: `identifier`
    fn read_delimited_identifier(&mut self) -> Result<String> {
        self.advance(); // Skip opening backtick

        let mut value = String::new();

        while let Some(c) = self.current_char {
            if c == '`' {
                // Check if escaped
                if self.peek() == Some('`') {
                    value.push('`');
                    self.advance(); // Skip first backtick
                    self.advance(); // Skip second backtick
                } else {
                    // End of identifier
                    self.advance(); // Skip closing backtick
                    return Ok(value);
                }
            } else if c == '\\' {
                // Handle escape sequences (ESC in fhirpath.g4)
                self.advance(); // Skip backslash
                let Some(escaped) = self.current_char else {
                    return Err(Error::ParseError(
                        "Incomplete escape sequence in delimited identifier".into(),
                    ));
                };

                match escaped {
                    '`' => value.push('`'),
                    '\'' => value.push('\''),
                    '\\' => value.push('\\'),
                    '/' => value.push('/'),
                    'f' => value.push('\x0C'),
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    'u' => {
                        // Unicode escape: \uXXXX
                        self.advance(); // Skip 'u'
                        let mut hex = String::new();
                        for _ in 0..4 {
                            if let Some(h) = self.current_char {
                                if h.is_ascii_hexdigit() {
                                    hex.push(h);
                                    self.advance();
                                } else {
                                    return Err(Error::ParseError(
                                        "Invalid unicode escape sequence".into(),
                                    ));
                                }
                            } else {
                                return Err(Error::ParseError(
                                    "Incomplete unicode escape sequence".into(),
                                ));
                            }
                        }
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| Error::ParseError("Invalid unicode code point".into()))?;
                        value.push(char::from_u32(code).ok_or_else(|| {
                            Error::ParseError("Invalid unicode character".into())
                        })?);
                        continue; // Don't advance again after unicode sequence
                    }
                    other => value.push(other),
                }

                self.advance();
            } else {
                value.push(c);
                self.advance();
            }
        }

        Err(Error::ParseError(
            "Unterminated delimited identifier".into(),
        ))
    }

    /// Read a string literal: 'string'
    fn read_string(&mut self) -> Result<String> {
        self.advance(); // Skip opening quote

        let mut value = String::new();

        while let Some(c) = self.current_char {
            if c == '\'' {
                // Check if escaped
                if self.peek() == Some('\'') {
                    value.push('\'');
                    self.advance(); // Skip first quote
                    self.advance(); // Skip second quote
                } else {
                    // End of string
                    self.advance(); // Skip closing quote
                    return Ok(value);
                }
            } else if c == '\\' {
                // Handle escape sequences
                self.advance(); // Skip backslash
                if let Some(escaped) = self.current_char {
                    match escaped {
                        '\'' => value.push('\''),
                        '\\' => value.push('\\'),
                        '/' => value.push('/'),
                        '"' => value.push('"'),
                        '`' => value.push('`'),
                        'f' => value.push('\x0C'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        'u' => {
                            // Unicode escape: \uXXXX
                            self.advance(); // Skip 'u'
                            let mut hex = String::new();
                            for _ in 0..4 {
                                if let Some(h) = self.current_char {
                                    if h.is_ascii_hexdigit() {
                                        hex.push(h);
                                        self.advance();
                                    } else {
                                        return Err(Error::ParseError(
                                            "Invalid unicode escape sequence".into(),
                                        ));
                                    }
                                } else {
                                    return Err(Error::ParseError(
                                        "Incomplete unicode escape sequence".into(),
                                    ));
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16).map_err(|_| {
                                Error::ParseError("Invalid unicode code point".into())
                            })?;
                            value.push(char::from_u32(code).ok_or_else(|| {
                                Error::ParseError("Invalid unicode character".into())
                            })?);
                            continue; // Don't advance again after unicode sequence
                        }
                        _ => value.push(escaped),
                    }
                    self.advance();
                }
            } else {
                value.push(c);
                self.advance();
            }
        }

        Err(Error::ParseError("Unterminated string literal".into()))
    }

    /// Read a number (NUMBER or LONGNUMBER)
    fn read_number(&mut self) -> (String, bool) {
        let start_pos = self.position;
        let mut is_long = false;
        let mut has_decimal = false;

        // Read integer part
        while let Some(c) = self.current_char {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        // Read decimal part if present (only if followed by digits)
        if self.current_char == Some('.') {
            // Peek ahead to see if there are digits after the dot
            if let Some(next_char) = self.peek() {
                if next_char.is_ascii_digit() {
                    has_decimal = true;
                    // There are digits after the dot, so include it in the number
                    self.advance(); // Skip '.'
                    while let Some(c) = self.current_char {
                        if c.is_ascii_digit() {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
            }
            // If no digits after dot, don't consume it - leave it for next token
        }

        // Check for 'L' suffix (long number)
        if !has_decimal && self.current_char == Some('L') {
            is_long = true;
            self.advance();
        }

        let value: String = self.chars[start_pos..self.position].iter().collect();
        (value, is_long)
    }

    /// Read a date/time literal: @DATE, @DATETIME, @TIME
    fn read_date_time(&mut self) -> Result<(String, TokenType)> {
        self.advance(); // Skip '@'

        if self.current_char == Some('T') {
            // Time literal: @T...
            self.advance(); // Skip 'T'
            return self.read_time_format().map(|s| (s, TokenType::TimeLiteral));
        }

        // Date or DateTime
        let date_str = self.read_date_format()?;

        if self.current_char == Some('T') {
            // DateTime literal: @DATE T TIME TIMEZONE?
            // Or partial DateTime: @2015T (year only) or @2015-02T (year-month only)
            self.advance(); // Skip 'T'

            // Check if there's a time component after T
            // If current char is a digit, there's a time component
            if self
                .current_char
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            {
                let time_str = self.read_time_format()?;
                let tz_str = if self.current_char_is_one_of(&['Z', '+', '-']) {
                    self.read_timezone_offset()?
                } else {
                    String::new()
                };
                Ok((
                    format!("{}T{}{}", date_str, time_str, tz_str),
                    TokenType::DateTimeLiteral,
                ))
            } else {
                // Partial DateTime: just date with trailing T
                Ok((format!("{}T", date_str), TokenType::DateTimeLiteral))
            }
        } else {
            Ok((date_str, TokenType::DateLiteral))
        }
    }

    /// Read date format: YYYY-MM-DD?
    fn read_date_format(&mut self) -> Result<String> {
        let mut value = String::new();

        // Read year (4 digits)
        for _ in 0..4 {
            if let Some(c) = self.current_char {
                if c.is_ascii_digit() {
                    value.push(c);
                    self.advance();
                } else {
                    return Err(Error::ParseError(
                        "Invalid date format: expected 4-digit year".into(),
                    ));
                }
            } else {
                return Err(Error::ParseError("Incomplete date format".into()));
            }
        }

        // Optional month
        if self.current_char == Some('-') {
            value.push('-'); // Include dash in output
            self.advance(); // Skip '-'
            for _ in 0..2 {
                if let Some(c) = self.current_char {
                    if c.is_ascii_digit() {
                        value.push(c);
                        self.advance();
                    } else {
                        return Err(Error::ParseError(
                            "Invalid date format: expected 2-digit month".into(),
                        ));
                    }
                } else {
                    return Err(Error::ParseError("Incomplete date format".into()));
                }
            }

            // Optional day
            if self.current_char == Some('-') {
                value.push('-'); // Include dash in output
                self.advance(); // Skip '-'
                for _ in 0..2 {
                    if let Some(c) = self.current_char {
                        if c.is_ascii_digit() {
                            value.push(c);
                            self.advance();
                        } else {
                            return Err(Error::ParseError(
                                "Invalid date format: expected 2-digit day".into(),
                            ));
                        }
                    } else {
                        return Err(Error::ParseError("Incomplete date format".into()));
                    }
                }
            }
        }

        Ok(value)
    }

    /// Read time format: HH:MM:SS.mmm?
    fn read_time_format(&mut self) -> Result<String> {
        let mut value = String::new();

        // Read hour (2 digits)
        for _ in 0..2 {
            if let Some(c) = self.current_char {
                if c.is_ascii_digit() {
                    value.push(c);
                    self.advance();
                } else {
                    return Err(Error::ParseError(
                        "Invalid time format: expected 2-digit hour".into(),
                    ));
                }
            } else {
                return Err(Error::ParseError("Incomplete time format".into()));
            }
        }

        // Optional minutes
        if self.current_char == Some(':') {
            value.push(':'); // Include colon in output
            self.advance(); // Skip ':'
            for _ in 0..2 {
                if let Some(c) = self.current_char {
                    if c.is_ascii_digit() {
                        value.push(c);
                        self.advance();
                    } else {
                        return Err(Error::ParseError(
                            "Invalid time format: expected 2-digit minute".into(),
                        ));
                    }
                } else {
                    return Err(Error::ParseError("Incomplete time format".into()));
                }
            }

            // Optional seconds
            if self.current_char == Some(':') {
                value.push(':'); // Include colon in output
                self.advance(); // Skip ':'
                for _ in 0..2 {
                    if let Some(c) = self.current_char {
                        if c.is_ascii_digit() {
                            value.push(c);
                            self.advance();
                        } else {
                            return Err(Error::ParseError(
                                "Invalid time format: expected 2-digit second".into(),
                            ));
                        }
                    } else {
                        return Err(Error::ParseError("Incomplete time format".into()));
                    }
                }

                // Optional milliseconds
                if self.current_char == Some('.') {
                    // Peek ahead to check if there are digits after the dot
                    // If no digits, the dot is not part of the time literal (e.g., @T14:34:28.is())
                    if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                        value.push('.'); // Include dot in output
                        self.advance(); // Skip '.'
                        while let Some(c) = self.current_char {
                            if c.is_ascii_digit() {
                                value.push(c);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(value)
    }

    /// Read timezone offset: Z or +/-HH:MM
    fn read_timezone_offset(&mut self) -> Result<String> {
        let mut value = String::new();

        if self.current_char == Some('Z') {
            value.push('Z');
            self.advance();
            return Ok(value);
        }

        // +/-HH:MM
        if let Some(sign) = self.current_char {
            if sign == '+' || sign == '-' {
                value.push(sign);
                self.advance();

                // Read hours
                for _ in 0..2 {
                    if let Some(c) = self.current_char {
                        if c.is_ascii_digit() {
                            value.push(c);
                            self.advance();
                        } else {
                            return Err(Error::ParseError(
                                "Invalid timezone format: expected 2-digit hour".into(),
                            ));
                        }
                    } else {
                        return Err(Error::ParseError("Incomplete timezone format".into()));
                    }
                }

                // fhirpath.g4 requires '+/-HH:MM'
                if self.current_char != Some(':') {
                    return Err(Error::ParseError(
                        "Invalid timezone format: expected ':' and 2-digit minute".into(),
                    ));
                }

                // Preserve the ':' in the offset so it remains RFC3339-compatible
                value.push(':');
                self.advance(); // Skip ':'

                // Read minutes
                for _ in 0..2 {
                    if let Some(c) = self.current_char {
                        if c.is_ascii_digit() {
                            value.push(c);
                            self.advance();
                        } else {
                            return Err(Error::ParseError(
                                "Invalid timezone format: expected 2-digit minute".into(),
                            ));
                        }
                    } else {
                        return Err(Error::ParseError("Incomplete timezone format".into()));
                    }
                }
            }
        }

        Ok(value)
    }

    /// Check if current char is one of the given characters
    fn current_char_is_one_of(&self, chars: &[char]) -> bool {
        self.current_char
            .map(|c| chars.contains(&c))
            .unwrap_or(false)
    }

    /// Get the next token from the input
    pub fn next_token(&mut self) -> Token {
        // Skip whitespace and comments
        loop {
            self.skip_whitespace();
            if self.current_char == Some('/') {
                // Check if it's actually a comment
                let is_line_comment = self.peek() == Some('/');
                let is_block_comment = self.peek() == Some('*');

                if is_line_comment || is_block_comment {
                    // It's a comment, skip it
                    if let Err(e) = self.skip_comment() {
                        return Token::error(
                            format!("Comment error: {}", e),
                            self.position,
                            self.line,
                            self.column,
                        );
                    }
                    // After skipping comment, continue loop to skip more whitespace/comments
                } else {
                    // It's a division operator, not a comment - break and handle it below
                    break;
                }
            } else {
                break;
            }
        }

        let position = self.position;
        let line = self.line;
        let column = self.column;

        if self.current_char.is_none() {
            return Token::eof(position, line, column);
        }

        let c = self.current_char.unwrap();

        // Single character tokens
        match c {
            '.' => {
                self.advance();
                Token::new(TokenType::Dot, ".".into(), position, line, column)
            }
            '[' => {
                self.advance();
                Token::new(TokenType::OpenBracket, "[".into(), position, line, column)
            }
            ']' => {
                self.advance();
                Token::new(TokenType::CloseBracket, "]".into(), position, line, column)
            }
            '(' => {
                self.advance();
                Token::new(TokenType::OpenParen, "(".into(), position, line, column)
            }
            ')' => {
                self.advance();
                Token::new(TokenType::CloseParen, ")".into(), position, line, column)
            }
            '{' => {
                self.advance();
                Token::new(TokenType::OpenBrace, "{".into(), position, line, column)
            }
            '}' => {
                self.advance();
                Token::new(TokenType::CloseBrace, "}".into(), position, line, column)
            }
            ',' => {
                self.advance();
                Token::new(TokenType::Comma, ",".into(), position, line, column)
            }
            '%' => {
                self.advance();
                // External constant: %identifier or %STRING
                if self.current_char == Some('\'') {
                    // %STRING
                    match self.read_string() {
                        Ok(value) => {
                            Token::new(TokenType::ExternalConstant, value, position, line, column)
                        }
                        Err(e) => {
                            Token::error(format!("String error: {}", e), position, line, column)
                        }
                    }
                } else if self.current_char == Some('`') {
                    // %`delimited-identifier`
                    match self.read_delimited_identifier() {
                        Ok(value) => {
                            Token::new(TokenType::ExternalConstant, value, position, line, column)
                        }
                        Err(e) => Token::error(
                            format!("Delimited identifier error: {}", e),
                            position,
                            line,
                            column,
                        ),
                    }
                } else {
                    // %identifier
                    let ident = self.read_identifier();
                    Token::new(TokenType::ExternalConstant, ident, position, line, column)
                }
            }
            '@' => {
                // Date/DateTime/Time literal
                match self.read_date_time() {
                    Ok((value, token_type)) => {
                        Token::new(token_type, value, position, line, column)
                    }
                    Err(e) => {
                        Token::error(format!("Date/time error: {}", e), position, line, column)
                    }
                }
            }
            '\'' => {
                // String literal
                match self.read_string() {
                    Ok(value) => {
                        Token::new(TokenType::StringLiteral, value, position, line, column)
                    }
                    Err(e) => Token::error(format!("String error: {}", e), position, line, column),
                }
            }
            '`' => {
                // Delimited identifier
                match self.read_delimited_identifier() {
                    Ok(value) => Token::new(
                        TokenType::DelimitedIdentifier,
                        value,
                        position,
                        line,
                        column,
                    ),
                    Err(e) => Token::error(
                        format!("Delimited identifier error: {}", e),
                        position,
                        line,
                        column,
                    ),
                }
            }
            '+' => {
                self.advance();
                Token::new(TokenType::Plus, "+".into(), position, line, column)
            }
            '-' => {
                self.advance();
                Token::new(TokenType::Minus, "-".into(), position, line, column)
            }
            '*' => {
                self.advance();
                Token::new(TokenType::Multiply, "*".into(), position, line, column)
            }
            '/' => {
                self.advance();
                Token::new(TokenType::Divide, "/".into(), position, line, column)
            }
            '&' => {
                self.advance();
                Token::new(TokenType::Ampersand, "&".into(), position, line, column)
            }
            '|' => {
                self.advance();
                Token::new(TokenType::Pipe, "|".into(), position, line, column)
            }
            '=' => {
                self.advance();
                Token::new(TokenType::Equal, "=".into(), position, line, column)
            }
            '~' => {
                self.advance();
                Token::new(TokenType::Equivalent, "~".into(), position, line, column)
            }
            '<' => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Token::new(
                        TokenType::LessThanOrEqual,
                        "<=".into(),
                        position,
                        line,
                        column,
                    )
                } else {
                    Token::new(TokenType::LessThan, "<".into(), position, line, column)
                }
            }
            '>' => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Token::new(
                        TokenType::GreaterThanOrEqual,
                        ">=".into(),
                        position,
                        line,
                        column,
                    )
                } else {
                    Token::new(TokenType::GreaterThan, ">".into(), position, line, column)
                }
            }
            '!' => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Token::new(TokenType::NotEqual, "!=".into(), position, line, column)
                } else if self.current_char == Some('~') {
                    self.advance();
                    Token::new(
                        TokenType::NotEquivalent,
                        "!~".into(),
                        position,
                        line,
                        column,
                    )
                } else {
                    Token::error("Unexpected '!' character".into(), position, line, column)
                }
            }
            '$' => {
                self.advance();
                let ident = self.read_identifier();
                match ident.as_str() {
                    "this" => Token::new(TokenType::This, "$this".into(), position, line, column),
                    "index" => {
                        Token::new(TokenType::Index, "$index".into(), position, line, column)
                    }
                    "total" => {
                        Token::new(TokenType::Total, "$total".into(), position, line, column)
                    }
                    _ => Token::error(
                        format!("Unknown variable: ${}", ident),
                        position,
                        line,
                        column,
                    ),
                }
            }
            _ => {
                // Check if it's a digit (number)
                if c.is_ascii_digit() {
                    let (value, is_long) = self.read_number();
                    let token_type = if is_long {
                        TokenType::LongNumberLiteral
                    } else {
                        TokenType::NumberLiteral
                    };
                    Token::new(token_type, value, position, line, column)
                }
                // Check if it's an identifier start
                else if c.is_alphabetic() || c == '_' {
                    let ident = self.read_identifier();
                    // Check for keywords
                    let token_type = match ident.as_str() {
                        "true" => TokenType::BooleanLiteral,
                        "false" => TokenType::BooleanLiteral,
                        "as" => TokenType::As,
                        "is" => TokenType::Is,
                        "div" => TokenType::Div,
                        "mod" => TokenType::Mod,
                        "in" => TokenType::In,
                        "contains" => TokenType::Contains,
                        "and" => TokenType::And,
                        "or" => TokenType::Or,
                        "xor" => TokenType::Xor,
                        "implies" => TokenType::Implies,
                        _ => TokenType::Identifier,
                    };
                    Token::new(token_type, ident, position, line, column)
                } else {
                    Token::error(
                        format!("Unexpected character: {}", c),
                        position,
                        line,
                        column,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(input.to_string());
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            let is_eof = matches!(token.token_type, TokenType::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    #[test]
    fn test_identifiers() {
        let tokens = tokenize("Patient name _test");
        assert_eq!(tokens[0].token_type, TokenType::Identifier);
        assert_eq!(tokens[0].value, "Patient");
        assert_eq!(tokens[1].token_type, TokenType::Identifier);
        assert_eq!(tokens[1].value, "name");
        assert_eq!(tokens[2].token_type, TokenType::Identifier);
        assert_eq!(tokens[2].value, "_test");
    }

    #[test]
    fn test_string_literal() {
        let tokens = tokenize("'hello' 'world'");
        assert_eq!(tokens[0].token_type, TokenType::StringLiteral);
        assert_eq!(tokens[0].value, "hello");
        assert_eq!(tokens[1].token_type, TokenType::StringLiteral);
        assert_eq!(tokens[1].value, "world");
    }

    #[test]
    fn test_string_escape() {
        let tokens = tokenize("'hello\\'world'");
        assert_eq!(tokens[0].token_type, TokenType::StringLiteral);
        assert_eq!(tokens[0].value, "hello'world");
    }

    #[test]
    fn test_numbers() {
        let tokens = tokenize("123 45.67 999L");
        assert_eq!(tokens[0].token_type, TokenType::NumberLiteral);
        assert_eq!(tokens[0].value, "123");
        assert_eq!(tokens[1].token_type, TokenType::NumberLiteral);
        assert_eq!(tokens[1].value, "45.67");
        assert_eq!(tokens[2].token_type, TokenType::LongNumberLiteral);
        assert_eq!(tokens[2].value, "999L");
    }

    #[test]
    fn test_boolean_literals() {
        let tokens = tokenize("true false");
        assert_eq!(tokens[0].token_type, TokenType::BooleanLiteral);
        assert_eq!(tokens[0].value, "true");
        assert_eq!(tokens[1].token_type, TokenType::BooleanLiteral);
        assert_eq!(tokens[1].value, "false");
    }

    #[test]
    fn test_operators() {
        let tokens = tokenize("+ - * / = != < <= > >=");
        assert_eq!(tokens[0].token_type, TokenType::Plus);
        assert_eq!(tokens[1].token_type, TokenType::Minus);
        assert_eq!(tokens[2].token_type, TokenType::Multiply);
        assert_eq!(tokens[3].token_type, TokenType::Divide);
        assert_eq!(tokens[4].token_type, TokenType::Equal);
        assert_eq!(tokens[5].token_type, TokenType::NotEqual);
        assert_eq!(tokens[6].token_type, TokenType::LessThan);
        assert_eq!(tokens[7].token_type, TokenType::LessThanOrEqual);
        assert_eq!(tokens[8].token_type, TokenType::GreaterThan);
        assert_eq!(tokens[9].token_type, TokenType::GreaterThanOrEqual);
    }

    #[test]
    fn test_keywords() {
        let tokens = tokenize("and or xor implies div mod in contains as is");
        assert_eq!(tokens[0].token_type, TokenType::And);
        assert_eq!(tokens[1].token_type, TokenType::Or);
        assert_eq!(tokens[2].token_type, TokenType::Xor);
        assert_eq!(tokens[3].token_type, TokenType::Implies);
        assert_eq!(tokens[4].token_type, TokenType::Div);
        assert_eq!(tokens[5].token_type, TokenType::Mod);
        assert_eq!(tokens[6].token_type, TokenType::In);
        assert_eq!(tokens[7].token_type, TokenType::Contains);
        assert_eq!(tokens[8].token_type, TokenType::As);
        assert_eq!(tokens[9].token_type, TokenType::Is);
    }

    #[test]
    fn test_variables() {
        let tokens = tokenize("$this $index $total");
        assert_eq!(tokens[0].token_type, TokenType::This);
        assert_eq!(tokens[1].token_type, TokenType::Index);
        assert_eq!(tokens[2].token_type, TokenType::Total);
    }

    #[test]
    fn test_path_navigation() {
        let tokens = tokenize("Patient.name.given");
        assert_eq!(tokens[0].token_type, TokenType::Identifier);
        assert_eq!(tokens[1].token_type, TokenType::Dot);
        assert_eq!(tokens[2].token_type, TokenType::Identifier);
        assert_eq!(tokens[3].token_type, TokenType::Dot);
        assert_eq!(tokens[4].token_type, TokenType::Identifier);
    }

    #[test]
    fn test_external_constant() {
        let tokens = tokenize("%resource %context");
        assert_eq!(tokens[0].token_type, TokenType::ExternalConstant);
        assert_eq!(tokens[0].value, "resource");
        assert_eq!(tokens[1].token_type, TokenType::ExternalConstant);
        assert_eq!(tokens[1].value, "context");
    }

    #[test]
    fn test_comments() {
        let tokens = tokenize("Patient // comment\nname");
        assert_eq!(tokens[0].token_type, TokenType::Identifier);
        assert_eq!(tokens[0].value, "Patient");
        assert_eq!(tokens[1].token_type, TokenType::Identifier);
        assert_eq!(tokens[1].value, "name");
    }

    #[test]
    fn test_null_literal() {
        let tokens = tokenize("{}");
        assert_eq!(tokens[0].token_type, TokenType::OpenBrace);
        assert_eq!(tokens[1].token_type, TokenType::CloseBrace);
        // Note: Null literal is recognized as {} in parser, not lexer
    }
}
