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

| # | Title | Size | Depends on |
|---|--------|------|------------|
| 1 | Scaffold Cargo workspace | S | — |
| 2 | Spans + miette diagnostics | S | 1 |
| 3 | Lexer (MVP) | M | 2 |
| 4 | AST (MVP) | M | 2 |
| 5 | Parser + grammar.md | M | 3, 4 |
| 6 | Lamina.toml config | S | 1 |
| 7 | Single-file resolve | M | 5 |
| 8 | Types: pure | M | 7 |
| 9 | Types: Stage intrinsics | M | 8 |
| 10 | Build IR + solve_set | M | 2 |
| 11 | Eval: pure | M | 6, 8, 10 |
| 12 | Eval: Stage → IR | M | 9, 11 |
| 13 | LLB lowerer + goldens | L | 10, 12 |
| 14 | CLI: check / explain / build | L | 6, 13 |
| 15 | Example hello-static | S | 14 |
| 16 | Release 0.1.0 | S | 1–15 |

Issue numbers on GitHub may differ slightly; match by title / `pr-plan:N` label.

## Post-0.1 (queued, not agent-ready until 0.1 ships)

| Theme | Notes |
|-------|--------|
| Path imports / modules | After 0.1 |
| Mounts + secrets + ssh | After LLB + client |
| Stdlib recipes | After modules + mounts |
| Lint pack | After check pipeline |
| `lamina fmt` | After parser |
| Multi-platform | After mounts |
| Lossy emit-dockerfile | Optional debug only |
| BuildKit gateway frontend | 1.x |
| Lockfile / remote modules | Open design |
| LSP | After types + fmt |

## How agents should use this

1. Filter: milestone **v0.1** + label **`agent-ready`**.  
2. Skip issues whose **Depends on** are still open.  
3. One issue per session/PR.  
4. Do not pull Post-0.1 work into 0.1 PRs.  
