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
    Use,
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
    EqEq,     // ==
    BangEq,   // !=
    AmpAmp,   // &&
    PipePipe, // ||
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
                    if self.peek() == Some(b'=') {
                        self.bump();
                        self.push(TokenKind::EqEq, start);
                    } else {
                        self.push(TokenKind::Eq, start);
                    }
                }
                b'!' => {
                    if self.src.get(self.pos + 1) == Some(&b'=') {
                        self.bump();
                        self.bump();
                        self.push(TokenKind::BangEq, start);
                    } else {
                        return Err(DiagnosticMsg::error(
                            "unexpected character '!'; use `!=` for inequality",
                            Some(self.span(start, start + 1)),
                        ));
                    }
                }
                b'&' => {
                    if self.src.get(self.pos + 1) == Some(&b'&') {
                        self.bump();
                        self.bump();
                        self.push(TokenKind::AmpAmp, start);
                    } else {
                        return Err(DiagnosticMsg::error(
                            "unexpected character '&'; use `&&` for logical and",
                            Some(self.span(start, start + 1)),
                        ));
                    }
                }
                b'|' => {
                    if self.src.get(self.pos + 1) == Some(&b'|') {
                        self.bump();
                        self.bump();
                        self.push(TokenKind::PipePipe, start);
                    } else {
                        return Err(DiagnosticMsg::error(
                            "unexpected character '|'; use `||` for logical or",
                            Some(self.span(start, start + 1)),
                        ));
                    }
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
            "use" => TokenKind::Use,
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
        self.bump(); // first "
                     // Triple-quoted multiline: """ … """ or r""" … """
        let triple = self.peek() == Some(b'"') && self.src.get(self.pos + 1) == Some(&b'"');
        if triple {
            self.bump(); // second "
            self.bump(); // third "
            return self.string_triple(start, raw);
        }

        if raw {
            let mut lit = String::new();
            while let Some(c) = self.peek() {
                if c == b'"' {
                    self.bump();
                    self.push(TokenKind::String(lit), start);
                    return Ok(());
                }
                if c == b'\n' {
                    return Err(DiagnosticMsg::error(
                        "newline in single-line raw string; use r\"\"\"…\"\"\" for multiline",
                        Some(self.span(start, self.pos)),
                    ));
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
                b'\n' => {
                    return Err(DiagnosticMsg::error(
                        "newline in single-line string; use \"\"\"…\"\"\" for multiline",
                        Some(self.span(start, self.pos)),
                    ));
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

    /// `"""…"""` or `r"""…"""` — multiline with common-indent strip.
    fn string_triple(&mut self, start: usize, raw: bool) -> std::result::Result<(), DiagnosticMsg> {
        if raw {
            let mut lit = String::new();
            while self.pos < self.src.len() {
                if self.peek() == Some(b'"')
                    && self.src.get(self.pos + 1) == Some(&b'"')
                    && self.src.get(self.pos + 2) == Some(&b'"')
                {
                    self.bump();
                    self.bump();
                    self.bump();
                    let lit = dedent_multiline(&lit);
                    self.push(TokenKind::String(lit), start);
                    return Ok(());
                }
                lit.push(self.bump().unwrap() as char);
            }
            return Err(DiagnosticMsg::error(
                "unterminated raw multiline string (r\"\"\")",
                Some(self.span(start, self.pos)),
            ));
        }

        let mut parts: Vec<StrPart> = Vec::new();
        let mut buf = String::new();
        let mut has_interp = false;

        while self.pos < self.src.len() {
            // Closing """
            if self.peek() == Some(b'"')
                && self.src.get(self.pos + 1) == Some(&b'"')
                && self.src.get(self.pos + 2) == Some(&b'"')
            {
                self.bump();
                self.bump();
                self.bump();
                if !buf.is_empty() || parts.is_empty() {
                    parts.push(StrPart::Lit(std::mem::take(&mut buf)));
                }
                if has_interp {
                    let mut parts = dedent_interp_parts(parts);
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
                        Some(StrPart::Lit(s)) => dedent_multiline(&s),
                        _ => String::new(),
                    };
                    self.push(TokenKind::String(s), start);
                }
                return Ok(());
            }

            let c = self.peek().unwrap();
            match c {
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
            "unterminated multiline string (\"\"\")",
            Some(self.span(start, self.pos)),
        ))
    }
}

/// Strip a leading newline and common leading indentation (spaces only).
pub fn dedent_multiline(s: &str) -> String {
    let s = s.strip_prefix('\n').unwrap_or(s);
    // Drop a single trailing newline that only exists so `"""` can sit on its own line.
    let s = s.strip_suffix('\n').unwrap_or(s);

    let lines: Vec<&str> = s.split('\n').collect();
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                ""
            } else if l.len() >= min_indent && l.as_bytes()[..min_indent].iter().all(|&b| b == b' ')
            {
                &l[min_indent..]
            } else {
                *l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn reconstruct_for_dedent(parts: &[StrPart]) -> String {
    let mut s = String::new();
    for p in parts {
        match p {
            StrPart::Lit(t) => s.push_str(t),
            StrPart::Ident(n) => {
                s.push_str("${");
                s.push_str(n);
                s.push('}');
            }
        }
    }
    s
}

/// Dedent multiline content that contains `${…}` by computing indent from the
/// full reconstructed text, then stripping that many spaces from each line of
/// each literal part (idents unchanged).
fn dedent_interp_parts(parts: Vec<StrPart>) -> Vec<StrPart> {
    let full = reconstruct_for_dedent(&parts);
    let dedented = dedent_multiline(&full);
    // If lengths of non-interp structure diverge, fall back to per-lit dedent.
    if !full.contains("${") {
        return vec![StrPart::Lit(dedented)];
    }
    // Apply same min-indent to each line of each lit segment independently using
    // global min indent from full text.
    let body = full.strip_prefix('\n').unwrap_or(full.as_str());
    let body = body.strip_suffix('\n').unwrap_or(body);
    let min_indent = body
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0);

    parts
        .into_iter()
        .map(|p| match p {
            StrPart::Ident(n) => StrPart::Ident(n),
            StrPart::Lit(lit) => {
                let stripped = lit
                    .split('\n')
                    .map(|l| {
                        if l.trim().is_empty() {
                            ""
                        } else if l.len() >= min_indent
                            && l.as_bytes()[..min_indent].iter().all(|&b| b == b' ')
                        {
                            &l[min_indent..]
                        } else {
                            l
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                // Also strip one leading newline from the first lit if present
                // (already handled by global strip on full; per-part first lit
                // may still start with \n).
                StrPart::Lit(stripped)
            }
        })
        .collect()
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
    fn comparison_and_logic_tokens() {
        let toks = lex(&file(r#"a == b != c && d || e"#)).unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::Ident(s) if s == "a"));
        assert!(matches!(kinds[1], TokenKind::EqEq));
        assert!(matches!(kinds[2], TokenKind::Ident(s) if s == "b"));
        assert!(matches!(kinds[3], TokenKind::BangEq));
        assert!(matches!(kinds[4], TokenKind::Ident(s) if s == "c"));
        assert!(matches!(kinds[5], TokenKind::AmpAmp));
        assert!(matches!(kinds[6], TokenKind::Ident(s) if s == "d"));
        assert!(matches!(kinds[7], TokenKind::PipePipe));
        assert!(matches!(kinds[8], TokenKind::Ident(s) if s == "e"));
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

    #[test]
    fn multiline_dedent() {
        let src = "\"\"\"\n  set -eux\n  echo hi\n\"\"\"";
        let toks = lex(&file(src)).unwrap();
        match &toks[0].kind {
            TokenKind::String(s) => assert_eq!(s, "set -eux\necho hi"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn raw_multiline_keeps_dollar() {
        let src = "r\"\"\"\n  echo ${SHELL}\n\"\"\"";
        let toks = lex(&file(src)).unwrap();
        match &toks[0].kind {
            TokenKind::String(s) => assert_eq!(s, "echo ${SHELL}"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn dedent_helper() {
        assert_eq!(dedent_multiline("\n  a\n  b\n"), "a\nb");
    }
}
