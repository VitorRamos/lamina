//! Byte offset ↔ LSP position (UTF-16).

use lamina::span::{SourceFile, Span};
use tower_lsp::lsp_types::{Position, Range};

/// Convert a UTF-8 byte offset to an LSP Position (0-based line, UTF-16 character).
pub fn offset_to_position(src: &str, offset: usize) -> Position {
    let offset = offset.min(src.len());
    let mut line = 0u32;
    let mut character = 0u32;
    for (i, ch) in src.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    Position { line, character }
}

/// Convert an LSP Position to a UTF-8 byte offset.
pub fn position_to_offset(src: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut character = 0u32;
    for (i, ch) in src.char_indices() {
        if line == pos.line && character >= pos.character {
            return i;
        }
        if ch == '\n' {
            if line == pos.line {
                return i;
            }
            line += 1;
            character = 0;
        } else {
            if line == pos.line {
                character += ch.len_utf16() as u32;
                if character > pos.character {
                    return i;
                }
            }
        }
    }
    src.len()
}

pub fn span_to_range(file: &SourceFile, span: Span) -> Range {
    Range {
        start: offset_to_position(&file.src, span.start as usize),
        end: offset_to_position(&file.src, span.end as usize),
    }
}

/// Identifier-like word at byte offset (for hover/goto without full token map).
pub fn word_at_offset(src: &str, offset: usize) -> Option<(usize, usize, &str)> {
    if src.is_empty() {
        return None;
    }
    let offset = offset.min(src.len().saturating_sub(1));
    // Move to start of UTF-8 char
    let mut o = offset;
    while o > 0 && !src.is_char_boundary(o) {
        o -= 1;
    }
    let bytes = src.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    if !is_ident(bytes[o]) {
        // try one back
        if o > 0 {
            let mut p = o - 1;
            while p > 0 && !src.is_char_boundary(p) {
                p -= 1;
            }
            if is_ident(bytes[p]) {
                o = p;
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    let mut start = o;
    while start > 0 {
        let mut p = start - 1;
        while p > 0 && !src.is_char_boundary(p) {
            p -= 1;
        }
        if is_ident(bytes[p]) {
            start = p;
        } else {
            break;
        }
    }
    let mut end = o;
    while end < bytes.len() {
        if !src.is_char_boundary(end) {
            end += 1;
            continue;
        }
        if is_ident(bytes[end]) {
            end += 1;
        } else {
            break;
        }
    }
    let word = src.get(start..end)?;
    if word.is_empty() {
        None
    } else {
        Some((start, end, word))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_roundtrip_ascii() {
        let src = "let x = 1;\nStage.from";
        let pos = offset_to_position(src, 11); // start of Stage
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
        assert_eq!(position_to_offset(src, pos), 11);
    }

    #[test]
    fn word_at() {
        let src = "Stage.from(x)";
        let (s, e, w) = word_at_offset(src, 0).unwrap();
        assert_eq!(w, "Stage");
        assert_eq!(&src[s..e], "Stage");
        let (_, _, w) = word_at_offset(src, 7).unwrap();
        assert_eq!(w, "from");
    }
}
