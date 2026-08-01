//! Lexer for frozen MVP grammar.

use crate::diag::{DiagnosticMsg, Result};
use crate::span::{FileId, SourceFile, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Arg,
    Const,
    Let,
    Fn,
    Pub,
    Target,
    If,
    Else,
    For,
    In,
    True,
    False,
    // Soft keyword also used as call: param
    // Identifiers / literals
    Ident(String),
    String(String),
    /// String with `${name}` interpolation holes.
    StringInterp(Vec<StrPart>),
    Int(i64),
    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Colon,
    Dot,
    Plus,
    Arrow, // ->
    Eq,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),
    Ident(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(file: &SourceFile) -> Result<Vec<Token>> {
    let mut lx = Lexer {
        file_id: file.id,
        src: file.src.as_bytes(),
        pos: 0,
        tokens: Vec::new(),
    };
    lx.tokenize()
        .map_err(|d| crate::diag::CompileError::single(Some(file), d))?;
    Ok(lx.tokens)
}

struct Lexer<'a> {
    file_id: FileId,
    src: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.file_id, start as u32, end as u32)
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn tokenize(&mut self) -> std::result::Result<(), DiagnosticMsg> {
        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.bump();
                }
                b'/' if self.src.get(self.pos + 1) == Some(&b'/') => {
                    while let Some(ch) = self.peek() {
                        self.bump();
                        if ch == b'\n' {
                            break;
                        }
                    }
                }
                b'(' => {
                    self.bump();
                    self.push(TokenKind::LParen, start);
                }
                b')' => {
                    self.bump();
                    self.push(TokenKind::RParen, start);
                }
                b'{' => {
                    self.bump();
                    self.push(TokenKind::LBrace, start);
                }
                b'}' => {
                    self.bump();
                    self.push(TokenKind::RBrace, start);
                }
                b'[' => {
                    self.bump();
                    self.push(TokenKind::LBracket, start);
                }
                b']' => {
                    self.bump();
                    self.push(TokenKind::RBracket, start);
                }
                b',' => {
                    self.bump();
                    self.push(TokenKind::Comma, start);
                }
                b';' => {
                    self.bump();
                    self.push(TokenKind::Semicolon, start);
                }
                b':' => {
                    self.bump();
                    self.push(TokenKind::Colon, start);
                }
                b'.' => {
                    self.bump();
                    self.push(TokenKind::Dot, start);
                }
                b'+' => {
                    self.bump();
                    self.push(TokenKind::Plus, start);
                }
                b'=' => {
                    self.bump();
                    self.push(TokenKind::Eq, start);
                }
                b'-' if self.src.get(self.pos + 1) == Some(&b'>') => {
                    self.bump();
                    self.bump();
                    self.push(TokenKind::Arrow, start);
                }
                b'"' => self.string(start, false)?,
                b'r' if self.src.get(self.pos + 1) == Some(&b'"') => {
                    self.bump(); // r
                    self.string(start, true)?;
                }
                b'0'..=b'9' => self.number(start)?,
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.ident_or_kw(start),
                _ => {
                    return Err(DiagnosticMsg::error(
                        format!("unexpected character {:?}", c as char),
                        Some(self.span(start, start + 1)),
                    ));
                }
            }
        }
        let end = self.pos;
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.span(end, end),
        });
        Ok(())
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: self.span(start, self.pos),
        });
    }

    fn number(&mut self, start: usize) -> std::result::Result<(), DiagnosticMsg> {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        let n: i64 = s.parse().map_err(|_| {
            DiagnosticMsg::error(
                "integer literal out of range",
                Some(self.span(start, self.pos)),
            )
        })?;
        self.push(TokenKind::Int(n), start);
        Ok(())
    }

    fn ident_or_kw(&mut self, start: usize) {
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        ) {
            self.bump();
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        let kind = match s {
            "arg" => TokenKind::Arg,
            "const" => TokenKind::Const,
            "let" => TokenKind::Let,
            "fn" => TokenKind::Fn,
            "pub" => TokenKind::Pub,
            "target" => TokenKind::Target,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(s.to_string()),
        };
        self.push(kind, start);
    }

    fn string(&mut self, start: usize, raw: bool) -> std::result::Result<(), DiagnosticMsg> {
        // opening " already or need to consume
        if self.peek() != Some(b'"') {
            return Err(DiagnosticMsg::error(
                "expected string",
                Some(self.span(start, self.pos)),
            ));
        }
        self.bump(); // "
        if raw {
            let mut lit = String::new();
            while let Some(c) = self.peek() {
                if c == b'"' {
                    self.bump();
                    self.push(TokenKind::String(lit), start);
                    return Ok(());
                }
                lit.push(c as char);
                self.bump();
            }
            return Err(DiagnosticMsg::error(
                "unterminated raw string",
                Some(self.span(start, self.pos)),
            ));
        }

        let mut parts: Vec<StrPart> = Vec::new();
        let mut buf = String::new();
        let mut has_interp = false;

        while let Some(c) = self.peek() {
            match c {
                b'"' => {
                    self.bump();
                    if !buf.is_empty() || parts.is_empty() {
                        parts.push(StrPart::Lit(buf));
                    }
                    if has_interp {
                        // merge trailing empty? keep parts
                        if parts
                            .last()
                            .map(|p| matches!(p, StrPart::Lit(s) if s.is_empty()))
                            .unwrap_or(false)
                            && parts.len() > 1
                        {
                            parts.pop();
                        }
                        self.push(TokenKind::StringInterp(parts), start);
                    } else {
                        let s = match parts.into_iter().next() {
                            Some(StrPart::Lit(s)) => s,
                            _ => String::new(),
                        };
                        self.push(TokenKind::String(s), start);
                    }
                    return Ok(());
                }
                b'\\' => {
                    self.bump();
                    let esc = self.bump().ok_or_else(|| {
                        DiagnosticMsg::error(
                            "unterminated string escape",
                            Some(self.span(start, self.pos)),
                        )
                    })?;
                    let ch = match esc {
                        b'n' => '\n',
                        b't' => '\t',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'$' => '$',
                        other => other as char,
                    };
                    buf.push(ch);
                }
                b'$' if self.src.get(self.pos + 1) == Some(&b'{') => {
                    has_interp = true;
                    if !buf.is_empty() {
                        parts.push(StrPart::Lit(std::mem::take(&mut buf)));
                    }
                    self.bump(); // $
                    self.bump(); // {
                    let id_start = self.pos;
                    while matches!(
                        self.peek(),
                        Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
                    ) {
                        self.bump();
                    }
                    if self.peek() != Some(b'}') {
                        return Err(DiagnosticMsg::error(
                            "expected `}` after interpolation",
                            Some(self.span(id_start, self.pos)),
                        ));
                    }
                    let name = std::str::from_utf8(&self.src[id_start..self.pos])
                        .unwrap()
                        .to_string();
                    self.bump(); // }
                    parts.push(StrPart::Ident(name));
                }
                _ => {
                    buf.push(c as char);
                    self.bump();
                }
            }
        }
        Err(DiagnosticMsg::error(
            "unterminated string",
            Some(self.span(start, self.pos)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::FileId;

    fn file(src: &str) -> SourceFile {
        SourceFile::new(FileId(0), "t.lam", src)
    }

    #[test]
    fn keywords_and_idents() {
        let toks = lex(&file("let x = 1;")).unwrap();
        assert!(matches!(toks[0].kind, TokenKind::Let));
        assert!(matches!(toks[1].kind, TokenKind::Ident(ref s) if s == "x"));
    }

    #[test]
    fn string_interp() {
        let toks = lex(&file(r#""hi ${name}!""#)).unwrap();
        match &toks[0].kind {
            TokenKind::StringInterp(parts) => {
                assert_eq!(parts.len(), 3);
            }
            other => panic!("expected interp, got {other:?}"),
        }
    }
}
