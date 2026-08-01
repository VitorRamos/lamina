//! Lamina — typed language library for building container images via BuildKit LLB.
//!
//! Pipeline (product target):
//!
//! ```text
//! .lam  →  lamina  →  LLB  →  BuildKit  →  OCI image
//! ```
//!
//! This crate is a scaffold. Lexer, parser, types, eval, and IR arrive in later
//! issues. Do not add Dockerfile generation as a product path.

/// Crate version from Cargo (shared with the CLI).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Placeholder module slots for the language pipeline (filled by later PRs).
///
/// Intentionally empty — keeps module layout discoverable for agents.
pub mod placeholder {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semverish() {
        assert!(!VERSION.is_empty());
        assert!(
            VERSION.contains('.'),
            "expected dotted version, got {VERSION}"
        );
    }
}
