use super::super::bind::push_text;
use super::super::BindValue;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    And,
    Or,
    Not,
    Phrase(String),
    Word(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Term { value: String, phrase: bool },
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

pub(in crate::db::search::query_builder) fn compile_fhir_text_query(
    raw: &str,
    bind_params: &mut Vec<BindValue>,
) -> Option<String> {
    let mut p = Parser::new(raw);
    let expr = p.parse_or()?;
    if p.peek().is_some() {
        return None;
    }
    Some(compile_expr(&expr, bind_params))
}

fn compile_expr(expr: &Expr, bind_params: &mut Vec<BindValue>) -> String {
    match expr {
        Expr::Term { value, phrase } => {
            let idx = push_text(bind_params, value.clone());
            if *phrase {
                format!("phraseto_tsquery('simple', ${})", idx)
            } else {
                format!("plainto_tsquery('simple', ${})", idx)
            }
        }
        Expr::And(a, b) => format!(
            "({} && {})",
            compile_expr(a, bind_params),
            compile_expr(b, bind_params)
        ),
        Expr::Or(a, b) => format!(
            "({} || {})",
            compile_expr(a, bind_params),
            compile_expr(b, bind_params)
        ),
        Expr::Not(inner) => format!("!!({})", compile_expr(inner, bind_params)),
    }
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
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

    fn next_tok(&mut self) -> Option<Tok> {
        self.skip_ws();
        let c = self.peek_char()?;

        match c {
            '(' => {
                self.consume_char();
                return Some(Tok::LParen);
            }
            ')' => {
                self.consume_char();
                return Some(Tok::RParen);
            }
            '"' => {
                return self.lex_phrase();
            }
            _ => {}
        }

        self.lex_word()
    }

    fn lex_phrase(&mut self) -> Option<Tok> {
        if self.consume_char() != Some('"') {
            return None;
        }

        let mut out = String::new();
        let mut escaped = false;
        while let Some(c) = self.consume_char() {
            if escaped {
                out.push(c);
                escaped = false;
                continue;
            }
            match c {
                '\\' => escaped = true,
                '"' => return Some(Tok::Phrase(out)),
                _ => out.push(c),
            }
        }

        None
    }

    fn lex_word(&mut self) -> Option<Tok> {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                break;
            }
            self.consume_char();
        }
        let raw = self.input[start..self.pos].trim();
        if raw.is_empty() {
            return None;
        }

        match raw.to_ascii_uppercase().as_str() {
            "AND" => Some(Tok::And),
            "OR" => Some(Tok::Or),
            "NOT" => Some(Tok::Not),
            _ => Some(Tok::Word(raw.to_string())),
        }
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    peeked: Option<Tok>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lexer: Lexer::new(input),
            peeked: None,
        }
    }

    fn peek(&mut self) -> Option<&Tok> {
        if self.peeked.is_none() {
            self.peeked = self.lexer.next_tok();
        }
        self.peeked.as_ref()
    }

    fn next(&mut self) -> Option<Tok> {
        if let Some(tok) = self.peeked.take() {
            return Some(tok);
        }
        self.lexer.next_tok()
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Tok::Or)) {
            self.next();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::And) => {
                    self.next();
                }
                Some(Tok::Or) | Some(Tok::RParen) | None => break,
                // Implicit AND for adjacent terms/parentheses, e.g. "bone metastases".
                Some(Tok::Word(_) | Tok::Phrase(_) | Tok::LParen | Tok::Not) => {}
            }

            let right = match self.peek() {
                Some(Tok::Or) | Some(Tok::RParen) | None => break,
                _ => self.parse_unary()?,
            };
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if matches!(self.peek(), Some(Tok::Not)) {
            self.next();
            let inner = self.parse_unary()?;
            return Some(Expr::Not(Box::new(inner)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        match self.next()? {
            Tok::LParen => {
                let inner = self.parse_or()?;
                if self.next() != Some(Tok::RParen) {
                    return None;
                }
                Some(inner)
            }
            Tok::Phrase(s) => Some(Expr::Term {
                value: s,
                phrase: true,
            }),
            Tok::Word(s) => Some(Expr::Term {
                value: s,
                phrase: false,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_parentheses_and_or() {
        let mut binds = Vec::new();
        let sql = compile_fhir_text_query("(bone OR liver) AND metastases", &mut binds).unwrap();
        assert!(sql.contains("||"));
        assert!(sql.contains("&&"));
        assert!(sql.contains("plainto_tsquery"));
        assert_eq!(binds.len(), 3);
    }

    #[test]
    fn compiles_not_and_phrase() {
        let mut binds = Vec::new();
        let sql = compile_fhir_text_query("NOT \"bone metastases\"", &mut binds).unwrap();
        assert!(sql.contains("!!("));
        assert!(sql.contains("phraseto_tsquery"));
        assert_eq!(binds.len(), 1);
    }

    #[test]
    fn compiles_implicit_and() {
        let mut binds = Vec::new();
        let sql = compile_fhir_text_query("bone metastases", &mut binds).unwrap();
        assert!(sql.contains("&&"));
        assert_eq!(binds.len(), 2);
    }
}
