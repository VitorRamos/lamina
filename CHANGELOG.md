# Changelog

## 0.3.0 — 2026-08-01

### Added

- IR lint pack: `empty-stage`, `unused-stage`, `root-final`, `secret-env`, `unpinned-base`
- CLI `--deny <lint|all|warnings>` on `check` / `build`; `lamina check --list-lints`
- `[lint] deny = [...]` in `Lamina.toml`
- Multi-platform: `.platform("linux/amd64")`, `lamina build --platform …`, `[build] platforms`
- `--push` for multi-arch (required when multiple platforms)
- Example `platform-demo`

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
