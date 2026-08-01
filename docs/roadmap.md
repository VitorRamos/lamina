# Lamina roadmap (issue map)

Living index of GitHub milestones and issues. **Authoritative acceptance criteria live on each issue**; this file is a map for humans and agents.

See also: [`AGENTS.md`](../AGENTS.md), [`design.md`](design.md).

## Milestones

| Milestone | Goal |
|-----------|------|
| v0.1 — MVP (LLB) | Single-file `.lam` → LLB → `lamina build` → hello-static image |
| v0.2 — Composition | Path modules, mounts/secrets, stdlib, `fmt` |
| v0.3 — Hardening | Lints, platforms, more examples |
| v1.0 — Stable | Path lockfile, docs polish, stability bar |
| Post-1.0 | Frontend gateway, remote modules, optional Dockerfile export, LSP |

## v0.1 critical path (dependency order)

```text
scaffold ─┬► diagnostics ─┬► lexer ──┐
          │               └► AST ────┼► parser ─► resolve ─► types(pure) ─► types(Stage)
          │                          │                              │
          └► Lamina.toml ────────────┼──────────────────────────────┤
                                     │                              │
                         IR ◄────────┘                              │
                          │                                         │
                          └► eval(pure) ◄── types(pure), config     │
                                │                                   │
                                └► eval(Stage) ◄── types(Stage)     │
                                      │                             │
                                      └► LLB lower ─► CLI build ◄───┘
                                            │
                                            └► hello-static ─► release 0.1
```

| Issue | Title | Size | Depends on |
|-------|--------|------|------------|
| [#1](https://github.com/VitorRamos/lamina/issues/1) | Epic: v0.1 MVP | L | (tracking) |
| [#2](https://github.com/VitorRamos/lamina/issues/2) | Scaffold Cargo workspace | S | — |
| [#3](https://github.com/VitorRamos/lamina/issues/3) | Spans + miette diagnostics | S | #2 |
| [#4](https://github.com/VitorRamos/lamina/issues/4) | Lexer (MVP) | M | #3 |
| [#5](https://github.com/VitorRamos/lamina/issues/5) | AST (MVP) | M | #3 |
| [#6](https://github.com/VitorRamos/lamina/issues/6) | Parser + grammar.md | M | #4, #5 |
| [#7](https://github.com/VitorRamos/lamina/issues/7) | Lamina.toml config | S | #2 |
| [#8](https://github.com/VitorRamos/lamina/issues/8) | Single-file resolve | M | #6 |
| [#9](https://github.com/VitorRamos/lamina/issues/9) | Types: pure | M | #8 |
| [#10](https://github.com/VitorRamos/lamina/issues/10) | Types: Stage intrinsics | M | #9 |
| [#11](https://github.com/VitorRamos/lamina/issues/11) | Build IR + solve_set | M | #3 |
| [#12](https://github.com/VitorRamos/lamina/issues/12) | Eval: pure | M | #7, #9, #11 |
| [#13](https://github.com/VitorRamos/lamina/issues/13) | Eval: Stage → IR | M | #10, #12 |
| [#14](https://github.com/VitorRamos/lamina/issues/14) | LLB lowerer + goldens | L | #11, #13 |
| [#15](https://github.com/VitorRamos/lamina/issues/15) | CLI: check / explain / build | L | #7, #14 |
| [#16](https://github.com/VitorRamos/lamina/issues/16) | Example hello-static | S | #15 |
| [#17](https://github.com/VitorRamos/lamina/issues/17) | Release 0.1.0 | S | #16 (+ all above) |

**Start here:** [#2](https://github.com/VitorRamos/lamina/issues/2) (only unblocked `agent-ready` issue at bootstrap).

## v0.2 — Composition

| Issue | Theme | Status |
|-------|--------|--------|
| [#18](https://github.com/VitorRamos/lamina/issues/18) | Path imports / modules | shipped in v0.2 PR |
| [#19](https://github.com/VitorRamos/lamina/issues/19) | Mounts / secrets / ssh / label / healthcheck | shipped |
| [#20](https://github.com/VitorRamos/lamina/issues/20) | Stdlib recipes | shipped |
| [#21](https://github.com/VitorRamos/lamina/issues/21) | `lamina fmt` | shipped |

## Later

| Theme | Notes |
|-------|--------|
| Lint pack | v0.3 |
| Multi-platform | v0.3 |
| Lossy emit-dockerfile | Optional debug only |
| BuildKit gateway frontend | 1.x / post-1.0 |
| Lockfile / remote modules | Open design |
| LSP | After types + fmt |

## How agents should use this

1. Filter open milestone + label **`agent-ready`**.  
2. Skip issues whose **Depends on** are still open.  
3. One issue per session/PR when possible; milestone-closing batches OK when requested.  
4. Do not pull later-milestone work into earlier PRs.  
