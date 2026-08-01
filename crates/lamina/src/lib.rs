//! Lamina — typed language library for building container images via BuildKit LLB.
//!
//! ```text
//! .lam  →  lamina  →  IR / LLB  →  BuildKit  →  OCI image
//! ```

pub mod ast;
pub mod compile;
pub mod config;
pub mod diag;
pub mod eval;
pub mod fmt;
pub mod ir;
pub mod lexer;
pub mod lint;
pub mod lock;
pub mod modules;
pub mod parser;
pub mod span;
pub mod types;

/// Crate version from Cargo (shared with the CLI).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
