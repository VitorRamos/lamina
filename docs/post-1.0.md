# Post-1.0 work (refined)

Tracking issues: [#25](https://github.com/VitorRamos/lamina/issues/25) (gateway), [#27](https://github.com/VitorRamos/lamina/issues/27) (remote modules), [#28](https://github.com/VitorRamos/lamina/issues/28) (LSP).

These do **not** block 1.0. Prefer finishing one vertical at a time; do not expand language surface without a design note.

## Suggested order

```text
#28 LSP (MVP)     — improves authoring now; no BuildKit change
     │
#25 Gateway       — true buildx -f image.lam; can retire solve bridge later
     │
#27 Remote modules — needs design decisions first; builds on Lamina.lock
```

Parallel is OK for **LSP** and **gateway** (different crates). Remote modules should not start until syntax + trust model are decided.

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

Client `lamina build` remains supported. The internal ephemeral Dockerfile bridge may be demoted to fallback after gateway is default-quality.

## #27 — Remote modules (summary)

**Phase A:** accepted — see [`remote-modules.md`](remote-modules.md).  
**Phase B:** implemented in 1.1 (`git+https` / `ssh` / `file`, cache, lock `kind`+`commit`).

| Topic | Decision |
|-------|----------|
| Syntax | `use "git+https://host/repo.git?ref=TAG&path=mod.lam";` |
| Trust | Lockfile sha256 of file bytes; optional git commit |
| Cache | `$LAMINA_MODULE_CACHE` or `~/.cache/lamina/modules` |
| Offline | `LAMINA_OFFLINE=1` |

Not shipped: package registry, Sigstore, raw HTTP files.

## #28 — LSP (summary)

MVP server (`lamina-lsp` or `lamina lsp`) over stdio:

1. `textDocument/didOpen|didChange`  
2. Diagnostics (parse + typecheck; reuse compiler)  
3. Hover on Stage methods / types  
4. Goto definition for local `fn` / `use` path modules  

Out of scope for MVP: full rename, remote module resolution, semantic tokens polish, inlay hints.

## Related docs

- Design: [`design.md`](design.md) (gateway section, composition vision)  
- Lockfile (1.0): [`USER_GUIDE.md`](USER_GUIDE.md)  
- Agents: [`../AGENTS.md`](../AGENTS.md)  
