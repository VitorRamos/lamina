//! Source locations for diagnostics.

use std::fmt;

/// Opaque file identity within a compilation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

/// Byte range within a file (UTF-8 byte offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    pub fn dummy() -> Self {
        Self {
            file: FileId(0),
            start: 0,
            end: 0,
        }
    }

    pub fn merge(self, other: Span) -> Span {
        debug_assert_eq!(self.file, other.file);
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// In-memory source file.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub name: String,
    pub src: String,
}

impl SourceFile {
    pub fn new(id: FileId, name: impl Into<String>, src: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            src: src.into(),
        }
    }

    pub fn slice(&self, span: Span) -> &str {
        let start = (span.start as usize).min(self.src.len());
        let end = (span.end as usize).min(self.src.len());
        &self.src[start..end]
    }

    /// 1-based line/column for a byte offset.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 1u32;
        for (i, ch) in self.src.char_indices() {
            if i as u32 >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}
