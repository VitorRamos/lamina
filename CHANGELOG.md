# Changelog

## 0.2.0 — 2026-08-01

### Added

- Path-only modules: `use "./lib.lam";` and `use "std/….lam";` (`pub fn` exports)
- Mount type + constructors: `Mount.cache` / `secret` / `ssh` / `bind`
- Stage methods: `.run_with`, `.label`, `.healthcheck`
- Stdlib starters: `stdlib/golang.lam`, `stdlib/node.lam`
- `lamina fmt` / `lamina fmt --check`
- Examples: `compose-demo` (path module), `stdlib-go` (stdlib smoke)

## 0.1.0 — 2026-08-01

### Added

- Lamina language MVP: lexer, parser, types, compile-time eval, Build IR, `solve_set`
- CLI: `lamina check`, `explain`, `emit-llb`, `build`
- LLB op summaries + golden-friendly text format
- `examples/hello-static` multi-stage image
- Design docs, AGENTS.md, grammar, capability matrix

### Notes

- `lamina build` uses an internal BuildKit/buildx bridge (ephemeral); not a Dockerfile product path
