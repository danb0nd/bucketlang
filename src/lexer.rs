use crate::error::{Error, Result};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Ident,
    Number,
    String,
    Address,
    Arrow,
    AtEntry,
    AtTest,
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    EqEq,
    FatArrow,
    BangEq,
    Eq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Bang,
    AmpAmp,
    Pipe,
    PipePipe,
    Colon,
    ColonColon,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Dot,
    Eof,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut col = 1;

    while i < chars.len() {
        let start_line = line;
        let start_col = col;
        let c = chars[i];

        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            col += 1;
            continue;
        }
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '>' {
            tokens.push(Token {
                kind: TokenKind::Arrow,
                text: "->".into(),
                line: start_line,
                col: start_col,
            });
            i += 2;
            col += 2;
            continue;
        }
        if c == '=' {
            if i + 1 < chars.len() && chars[i + 1] == '=' {
                tokens.push(Token {
                    kind: TokenKind::EqEq,
                    text: "==".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            if i + 1 < chars.len() && chars[i + 1] == '>' {
                tokens.push(Token {
                    kind: TokenKind::FatArrow,
                    text: "=>".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Eq,
                text: "=".into(),
                line: start_line,
                col: start_col,
            });
            i += 1;
            col += 1;
            continue;
        }
        if c == '!' {
            if i + 1 < chars.len() && chars[i + 1] == '=' {
                tokens.push(Token {
                    kind: TokenKind::BangEq,
                    text: "!=".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Bang,
                text: "!".into(),
                line: start_line,
                col: start_col,
            });
            i += 1;
            col += 1;
            continue;
        }
        if c == '<' {
            if i + 1 < chars.len() && chars[i + 1] == '=' {
                tokens.push(Token {
                    kind: TokenKind::LtEq,
                    text: "<=".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Lt,
                text: "<".into(),
                line: start_line,
                col: start_col,
            });
            i += 1;
            col += 1;
            continue;
        }
        if c == '>' {
            if i + 1 < chars.len() && chars[i + 1] == '=' {
                tokens.push(Token {
                    kind: TokenKind::GtEq,
                    text: ">=".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Gt,
                text: ">".into(),
                line: start_line,
                col: start_col,
            });
            i += 1;
            col += 1;
            continue;
        }
        if c == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
            tokens.push(Token {
                kind: TokenKind::AmpAmp,
                text: "&&".into(),
                line: start_line,
                col: start_col,
            });
            i += 2;
            col += 2;
            continue;
        }
        if c == '|' {
            if i + 1 < chars.len() && chars[i + 1] == '|' {
                tokens.push(Token {
                    kind: TokenKind::PipePipe,
                    text: "||".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Pipe,
                text: "|".into(),
                line: start_line,
                col: start_col,
            });
            i += 1;
            col += 1;
            continue;
        }
        if c == '@' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let word: String = chars[i + 1..j].iter().collect();
            let kind = match word.as_str() {
                "entry" => TokenKind::AtEntry,
                "test" => TokenKind::AtTest,
                _ => {
                    return Err(Error::at(
                        "lex",
                        start_line,
                        start_col,
                        format!("unknown annotation @{word}"),
                    ));
                }
            };
            tokens.push(Token {
                kind,
                text: format!("@{word}"),
                line: start_line,
                col: start_col,
            });
            col += j - i;
            i = j;
            continue;
        }
        if c == '#' {
            let mut j = i + 1;
            if j >= chars.len()
                || !(chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '.')
            {
                return Err(Error::at(
                    "lex",
                    start_line,
                    start_col,
                    "expected address after #",
                ));
            }
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric()
                    || chars[j] == '_'
                    || chars[j] == '.'
                    || chars[j] == '/')
            {
                j += 1;
            }
            let text: String = chars[i..j].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Address,
                text,
                line: start_line,
                col: start_col,
            });
            col += j - i;
            i = j;
            continue;
        }
        if c == '"' {
            let mut j = i + 1;
            let mut out = String::new();
            while j < chars.len() {
                let ch = chars[j];
                if ch == '"' {
                    break;
                }
                if ch == '\\' {
                    j += 1;
                    if j >= chars.len() {
                        return Err(Error::at("lex", start_line, start_col, "unterminated string"));
                    }
                    match chars[j] {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        other => out.push(other),
                    }
                    j += 1;
                    continue;
                }
                if ch == '\n' {
                    return Err(Error::at("lex", start_line, start_col, "newline in string"));
                }
                out.push(ch);
                j += 1;
            }
            if j >= chars.len() || chars[j] != '"' {
                return Err(Error::at("lex", start_line, start_col, "unterminated string"));
            }
            tokens.push(Token {
                kind: TokenKind::String,
                text: out,
                line: start_line,
                col: start_col,
            });
            col += j - i + 1;
            i = j + 1;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '.' {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let text: String = chars[i..j].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Number,
                text,
                line: start_line,
                col: start_col,
            });
            col += j - i;
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let text: String = chars[i..j].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Ident,
                text,
                line: start_line,
                col: start_col,
            });
            col += j - i;
            i = j;
            continue;
        }

        if c == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            tokens.push(Token {
                kind: TokenKind::StarStar,
                text: "**".into(),
                line: start_line,
                col: start_col,
            });
            i += 2;
            col += 2;
            continue;
        }

        if c == ':' {
            if i + 1 < chars.len() && chars[i + 1] == ':' {
                tokens.push(Token {
                    kind: TokenKind::ColonColon,
                    text: "::".into(),
                    line: start_line,
                    col: start_col,
                });
                i += 2;
                col += 2;
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Colon,
                text: ":".into(),
                line: start_line,
                col: start_col,
            });
            i += 1;
            col += 1;
            continue;
        }

        let kind = match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            ',' => TokenKind::Comma,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '.' => TokenKind::Dot,
            _ => {
                return Err(Error::at(
                    "lex",
                    start_line,
                    start_col,
                    format!("unexpected character {c:?}"),
                ));
            }
        };
        tokens.push(Token {
            kind,
            text: c.to_string(),
            line: start_line,
            col: start_col,
        });
        i += 1;
        col += 1;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        text: String::new(),
        line,
        col,
    });
    Ok(tokens)
}
