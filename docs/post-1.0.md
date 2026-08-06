# Post-1.0 work (refined)

Shipped baselines:

| Issue | Theme | Version |
|-------|--------|---------|
| [#27](https://github.com/VitorRamos/lamina/issues/27) | Remote modules (`git+…`, lock, cache) | **1.1** — [`remote-modules.md`](remote-modules.md) |
| [#28](https://github.com/VitorRamos/lamina/issues/28) | LSP MVP (diagnostics, hover, goto, format) | **1.2** — [`lsp.md`](lsp.md) |

These do **not** block 1.0 (already shipped). Prefer finishing one vertical at a time; do not expand language surface without a design note. Living issue map: [`roadmap.md`](roadmap.md).

## Suggested order

Prefer **small CI/docs** and **correctness** before large BuildKit surfaces:

```text
#45 cargo-deny CI          — shipped (deny.toml + CI)
#47 CONTRIBUTING           — shipped
#48 README matrix          — shipped
#57 roadmap/post-1.0 map   — shipped (this doc)
     │
#44 Buildx integration tests (ignored)  — optional CI job later
#53 module re-export fix                — language correctness
     │
#49 VS Code extension  ──►  #50 LSP polish (tokens, rename, symbols)
     │
#51 true pb.Definition lower   — demote Dockerfile solve bridge
     │
#25 gateway.v0 frontend        — buildx -f image.lam
```

**Blocked until human input:**

| Issue | Why |
|-------|-----|
| [#54](https://github.com/VitorRamos/lamina/issues/54) | `needs-design` — string ops / richer pure operators |
| [#55](https://github.com/VitorRamos/lamina/issues/55) | `needs-decision` — multi-target `lamina build` UX |

Parallel is OK for **editor/LSP** (#49/#50) and **LLB/gateway** (#51/#25) when work is on separate crates/branches. Do not start dependents early.

## Non-negotiables (still apply)

1. LLB-primary product contract — no Dockerfile as authoring source of truth.  
2. Secrets never via params/build-args.  
3. Path sandbox for local modules; remotes only via lock + explicit fetch.  
4. Small PRs with acceptance criteria from the issue body.

## #25 — Gateway frontend (summary)

Ship a BuildKit **gateway.v0** frontend image that:

- Accepts a `.lam` entry (and project files as locals)
- Runs the Lamina compiler inside the frontend
- Returns a real `pb.Definition` Solve request to buildkitd

Success UX:

```bash
docker buildx build -f src/image.lam --build-arg BUILDKIT_SYNTAX=…/lamina-frontend:…
# or documented #syntax= equivalent for .lam
```

Client `lamina build` remains supported. The internal ephemeral Dockerfile bridge may be demoted to fallback after gateway is default-quality (see also [#51](https://github.com/VitorRamos/lamina/issues/51)).

## #27 — Remote modules (summary) — **shipped**

**Phase A:** accepted — see [`remote-modules.md`](remote-modules.md).  
**Phase B:** implemented in 1.1 (`git+https` / `ssh` / `file`, cache, lock `kind`+`commit`).

| Topic | Decision |
|-------|----------|
| Syntax | `use "git+https://host/repo.git?ref=TAG&path=mod.lam";` |
| Trust | Lockfile sha256 of file bytes; optional git commit |
| Cache | `$LAMINA_MODULE_CACHE` or `~/.cache/lamina/modules` |
| Offline | `LAMINA_OFFLINE=1` |

Not shipped: package registry, Sigstore, raw HTTP files.

## #28 — LSP (summary) — **shipped (MVP)**

MVP server (`lamina-lsp` / `lamina lsp`) over stdio:

1. `textDocument/didOpen|didChange`  
2. Diagnostics (parse + typecheck; reuse compiler)  
3. Hover on Stage methods / types  
4. Goto definition for local `fn` / `use` path modules  
5. Format (via `lamina fmt` pipeline)

Post-MVP polish: [#50](https://github.com/VitorRamos/lamina/issues/50). Editor packaging: [#49](https://github.com/VitorRamos/lamina/issues/49).

## Related open tracks (summaries)

| Issue | One-liner |
|-------|-----------|
| [#51](https://github.com/VitorRamos/lamina/issues/51) | Emit real `pb.Definition`; demote Dockerfile solve bridge |
| [#53](https://github.com/VitorRamos/lamina/issues/53) | Stop re-exporting transitive `pub fn`s through `use` |
| [#44](https://github.com/VitorRamos/lamina/issues/44) | Buildx integration tests (`#[ignore]`) + optional CI |
| [#45](https://github.com/VitorRamos/lamina/issues/45) | `cargo deny` licenses/advisories in CI — **shipped** |
| [#47](https://github.com/VitorRamos/lamina/issues/47)–[#48](https://github.com/VitorRamos/lamina/issues/48), [#57](https://github.com/VitorRamos/lamina/issues/57) | Docs / contributing / roadmap — **shipped** |

## Related docs

- Design: [`design.md`](design.md) (gateway section, composition vision)  
- User guide / lockfile: [`USER_GUIDE.md`](USER_GUIDE.md)  
- Contributing: [`../CONTRIBUTING.md`](../CONTRIBUTING.md)  
- Agents: [`../AGENTS.md`](../AGENTS.md)  
