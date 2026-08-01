//! Diagnostics (miette-backed).

use crate::span::{SourceFile, Span};
use miette::{Diagnostic, LabeledSpan, SourceCode};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct DiagnosticMsg {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
    pub help: Option<String>,
}

impl DiagnosticMsg {
    pub fn error(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            help: None,
        }
    }

    pub fn warning(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span,
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CompileError {
    pub message: String,
    pub diagnostics: Vec<DiagnosticMsg>,
    /// Source file text for miette snippets (not an Error::source chain).
    pub source_text: Option<String>,
    pub source_name: Option<String>,
}

impl CompileError {
    pub fn from_diags(file: Option<&SourceFile>, diagnostics: Vec<DiagnosticMsg>) -> Self {
        let message = diagnostics
            .first()
            .map(|d| d.message.clone())
            .unwrap_or_else(|| "compilation failed".into());
        Self {
            message,
            diagnostics,
            source_text: file.map(|f| f.src.clone()),
            source_name: file.map(|f| f.name.clone()),
        }
    }

    pub fn single(file: Option<&SourceFile>, diag: DiagnosticMsg) -> Self {
        Self::from_diags(file, vec![diag])
    }
}

impl Diagnostic for CompileError {
    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.diagnostics
            .first()
            .and_then(|d| d.help.as_ref())
            .map(|h| Box::new(h.clone()) as Box<dyn fmt::Display>)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let labels: Vec<_> = self
            .diagnostics
            .iter()
            .filter_map(|d| {
                let span = d.span?;
                let len = span.end.saturating_sub(span.start) as usize;
                Some(LabeledSpan::new(
                    Some(d.message.clone()),
                    span.start as usize,
                    if len == 0 { 1 } else { len },
                ))
            })
            .collect();
        if labels.is_empty() {
            None
        } else {
            Some(Box::new(labels.into_iter()))
        }
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.source_text.as_ref().map(|s| s as &dyn SourceCode)
    }
}

pub type Result<T> = std::result::Result<T, CompileError>;
