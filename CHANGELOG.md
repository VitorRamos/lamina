# Changelog

## 0.1.0 — 2026-08-01

### Added

- Lamina language MVP: lexer, parser, types, compile-time eval, Build IR, `solve_set`
- CLI: `lamina check`, `explain`, `emit-llb`, `build`
- LLB op summaries + golden-friendly text format
- `examples/hello-static` multi-stage image
- Design docs, AGENTS.md, grammar, capability matrix

### Notes

- `lamina build` uses an internal BuildKit/buildx bridge (ephemeral); not a Dockerfile product path
