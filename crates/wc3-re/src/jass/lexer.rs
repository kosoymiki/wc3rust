//! JASS2 lexer.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Int(i64),
    Real(f64),
    Str(String),
    Newline,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub line: u32,
    pub message: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for LexError {}

/// Tokenize JASS source. Newlines are significant and preserved.
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    let mut line = 1u32;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\n' => {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    line,
                });
                line += 1;
                index += 1;
            }
            b'\r' => {
                index += 1;
            }
            b' ' | b'\t' => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'"' => {
                let start_line = line;
                index += 1;
                let mut value = String::new();
                let mut closed = false;
                while index < bytes.len() {
                    let ch = bytes[index];
                    if ch == b'"' {
                        index += 1;
                        closed = true;
                        break;
                    }
                    if ch == b'\\' && index + 1 < bytes.len() {
                        index += 1;
                        match bytes[index] {
                            b'n' => value.push('\n'),
                            b'r' => value.push('\r'),
                            b't' => value.push('\t'),
                            b'"' => value.push('"'),
                            b'\\' => value.push('\\'),
                            other => value.push(other as char),
                        }
                        index += 1;
                        continue;
                    }
                    if ch == b'\n' {
                        line += 1;
                    }
                    // JASS strings are byte strings; the retail scripts are
                    // ASCII, so a byte-wise copy is exact for the corpus.
                    value.push(ch as char);
                    index += 1;
                }
                if !closed {
                    return Err(LexError {
                        line: start_line,
                        message: "unterminated string literal".into(),
                    });
                }
                tokens.push(Token {
                    kind: TokenKind::Str(value),
                    line: start_line,
                });
            }
            b'\'' => {
                let start_line = line;
                index += 1;
                let start = index;
                while index < bytes.len() && bytes[index] != b'\'' && bytes[index] != b'\n' {
                    index += 1;
                }
                if index >= bytes.len() || bytes[index] != b'\'' {
                    return Err(LexError {
                        line: start_line,
                        message: "unterminated rawcode".into(),
                    });
                }
                let raw = &source[start..index];
                index += 1;
                if raw.len() != 4 {
                    return Err(LexError {
                        line: start_line,
                        message: format!("rawcode must be 4 characters, got {raw:?}"),
                    });
                }
                let mut value = 0u32;
                for byte in raw.bytes() {
                    value = (value << 8) | byte as u32;
                }
                tokens.push(Token {
                    kind: TokenKind::Int(value as i64),
                    line: start_line,
                });
            }
            b'0'..=b'9' => {
                let start_line = line;
                let (token, next) = lex_number(source, index)?;
                index = next;
                tokens.push(Token {
                    kind: token,
                    line: start_line,
                });
            }
            b'$' => {
                let start_line = line;
                index += 1;
                let start = index;
                while index < bytes.len() && (bytes[index] as char).is_ascii_hexdigit() {
                    index += 1;
                }
                if start == index {
                    return Err(LexError {
                        line: start_line,
                        message: "empty hex literal".into(),
                    });
                }
                let value = i64::from_str_radix(&source[start..index], 16).map_err(|err| LexError {
                    line: start_line,
                    message: format!("bad hex literal: {err}"),
                })?;
                tokens.push(Token {
                    kind: TokenKind::Int(value),
                    line: start_line,
                });
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = index;
                while index < bytes.len()
                    && ((bytes[index] as char).is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Ident(source[start..index].to_string()),
                    line,
                });
            }
            b'(' => {
                tokens.push(Token { kind: TokenKind::LParen, line });
                index += 1;
            }
            b')' => {
                tokens.push(Token { kind: TokenKind::RParen, line });
                index += 1;
            }
            b'[' => {
                tokens.push(Token { kind: TokenKind::LBracket, line });
                index += 1;
            }
            b']' => {
                tokens.push(Token { kind: TokenKind::RBracket, line });
                index += 1;
            }
            b',' => {
                tokens.push(Token { kind: TokenKind::Comma, line });
                index += 1;
            }
            b'=' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Token { kind: TokenKind::EqEq, line });
                    index += 2;
                } else {
                    tokens.push(Token { kind: TokenKind::Eq, line });
                    index += 1;
                }
            }
            b'!' if bytes.get(index + 1) == Some(&b'=') => {
                tokens.push(Token { kind: TokenKind::NotEq, line });
                index += 2;
            }
            b'<' if bytes.get(index + 1) == Some(&b'=') => {
                tokens.push(Token { kind: TokenKind::Le, line });
                index += 2;
            }
            b'>' if bytes.get(index + 1) == Some(&b'=') => {
                tokens.push(Token { kind: TokenKind::Ge, line });
                index += 2;
            }
            b'<' => {
                tokens.push(Token { kind: TokenKind::Lt, line });
                index += 1;
            }
            b'>' => {
                tokens.push(Token { kind: TokenKind::Gt, line });
                index += 1;
            }
            b'+' => {
                tokens.push(Token { kind: TokenKind::Plus, line });
                index += 1;
            }
            b'-' => {
                tokens.push(Token { kind: TokenKind::Minus, line });
                index += 1;
            }
            b'*' => {
                tokens.push(Token { kind: TokenKind::Star, line });
                index += 1;
            }
            b'/' => {
                tokens.push(Token { kind: TokenKind::Slash, line });
                index += 1;
            }
            other => {
                return Err(LexError {
                    line,
                    message: format!("unexpected byte 0x{other:02x}"),
                });
            }
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
    });
    Ok(tokens)
}

fn lex_number(source: &str, start: usize) -> Result<(TokenKind, usize), LexError> {
    let bytes = source.as_bytes();
    let mut index = start;
    if bytes[index] == b'0' && matches!(bytes.get(index + 1), Some(b'x' | b'X')) {
        index += 2;
        let hex_start = index;
        while index < bytes.len() && (bytes[index] as char).is_ascii_hexdigit() {
            index += 1;
        }
        if hex_start == index {
            return Err(LexError {
                line: 0,
                message: "empty 0x literal".into(),
            });
        }
        let value = i64::from_str_radix(&source[hex_start..index], 16).map_err(|err| LexError {
            line: 0,
            message: format!("bad hex literal: {err}"),
        })?;
        return Ok((TokenKind::Int(value), index));
    }
    let mut is_real = false;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        is_real = true;
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        is_real = true;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
    }
    let text = &source[start..index];
    if is_real {
        let value: f64 = text.parse().map_err(|err| LexError {
            line: 0,
            message: format!("bad real literal: {err}"),
        })?;
        Ok((TokenKind::Real(value), index))
    } else {
        let value: i64 = text.parse().map_err(|err| LexError {
            line: 0,
            message: format!("bad integer literal: {err}"),
        })?;
        Ok((TokenKind::Int(value), index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_rawcodes_strings_and_numbers() {
        let tokens = lex("set x = 'hfoo'\ncall f(1.5, $FF, 0x10, \"a\\n\\\"b\")\n").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Int(0x6866_6F6F)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Real(1.5)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Int(255)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Int(16)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Str("a\n\"b".into())));
    }

    #[test]
    fn lexes_real_corpus_file() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../analysis/war3_full3/Scripts/common.j");
        let Ok(source) = std::fs::read_to_string(path) else {
            eprintln!("skip: common.j missing");
            return;
        };
        let tokens = lex(&source).expect("lex common.j");
        assert!(tokens.len() > 10_000);
    }
}
