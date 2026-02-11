//! Search value escaping helpers (FHIR "Encoding Note").
//!
//! FHIR search values may escape special separator characters using `\`:
//! - `\,` (comma in values)
//! - `\|` (token system/code separator)
//! - `\$` (composite tuple separator)
//! - `\\` (literal backslash)

pub(crate) fn split_unescaped(input: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        match bytes[i] as char {
            '\\' => {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
            }
            c if c == sep => {
                out.push(&input[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push(&input[start..]);
    out
}

pub(crate) fn unescape_search_value(input: &str) -> Result<String, ()> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err(());
        };
        match next {
            '\\' | ',' | '$' | '|' => out.push(next),
            _ => return Err(()),
        }
    }
    Ok(out)
}
