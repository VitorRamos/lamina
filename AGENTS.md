# AGENTS.md — Lamina

Instructions for AI coding agents (and humans) working in this repository.

**Source of truth for product & architecture:** [`docs/design.md`](docs/design.md)  
**Tracking:** GitHub [milestones](https://github.com/VitorRamos/lamina/milestones) + [issues](https://github.com/VitorRamos/lamina/issues)

---

## What Lamina is

Lamina is a **typed language for building container images**. It compiles to BuildKit **LLB** and produces **OCI** images.

```text
.lam  →  lamina  →  LLB  →  BuildKit  →  OCI image
```

It does **not** generate Dockerfiles as the product path.

| Role | Name |
|------|------|
| Language | Lamina |
| CLI | `lamina` |
| Config | `Lamina.toml` |
| Sources | `*.lam` |
| Crates | `lamina` (lib), `lamina-cli` (bin), `lamina-llb`, `lamina-client` |

---

## Non-negotiables (read before coding)

1. **LLB-primary.** Lower Build IR → `pb.Definition` → BuildKit Solve. No Dockerfile emitter as correctness oracle or v1 contract.
2. **Stages are immutable values.** Methods return new stages; `solve_set` = targets ∪ `copy_from` sources only.
3. **Frozen MVP grammar** is in `docs/design.md` Appendix A. Do not invent `import`, block-stage sugar, or `image` keywords in 0.1.
4. **Small PRs.** One issue ≈ one mergeable PR. Prefer completing the critical path of milestone **0.1** over scope creep.
5. **Design wins disputes.** If code and `docs/design.md` disagree, change code—or open an issue to change the design *first*.
6. **No drive-by refactors.** Only touch what the issue needs.
7. **Secrets never via params.** No credentials in `--param` / `--build-arg`; secret mounts are 0.2+.

---

## Agentic workflow

### Default loop

```text
1. Sync main
2. Pick ONE agent-ready issue (labels + milestone rules below)
3. Create branch: agent/<issue-number>-short-slug
4. Implement only that issue’s acceptance criteria
5. Verify (commands below)
6. Open PR → link “Closes #N”
7. Stop. Do not chain the next issue unless the user says so.
```

### Picking work

| Priority | Rule |
|----------|------|
| 1 | Open issues in milestone **v0.1 — MVP (LLB)** with label `agent-ready` |
| 2 | Respect **blocked-by** / dependency comments; do not start dependents early |
| 3 | Prefer **critical path**: scaffold → diagnostics → lexer/AST/parser → types → IR → eval → LLB → CLI build → example → release |
| 4 | Skip issues labeled `needs-design` or `human-only` unless the user assigns them |

**Parallel-safe pairs** (can run as concurrent agents *on separate worktrees/branches* only if neither depends on the other):

| Track A | Track B |
|---------|---------|
| PR/issue: Lexer | PR/issue: AST |
| PR/issue: IR | PR/issue: Lamina.toml config |
| PR/issue: pure types | (wait for resolve) |

**Do not parallelize** anything that both touch the same crate root without coordination—prefer sequential on solo velocity.

### Issue contract (every implementation issue)

Each issue body should contain:

- **Goal** — one sentence  
- **Acceptance criteria** — checklist  
- **Out of scope** — explicit  
- **Depends on** — issue numbers  
- **Design refs** — section or appendix in `docs/design.md`  
- **Verify** — exact commands  

If an issue is missing these, add them in a comment or fix the issue before coding (`label: needs-triage` until fixed).

### Definition of done

- [ ] Acceptance criteria checked off  
- [ ] `cargo test` / `cargo clippy` clean for touched crates (once workspace exists)  
- [ ] No Dockerfile generation path introduced  
- [ ] Public API / CLI flags match design names (`lamina`, `Lamina.toml`, `.lam`)  
- [ ] PR description: what / why / how to test; `Closes #N`  
- [ ] Golden tests updated when IR/LLB summaries change (`insta` once present)  

### When stuck

1. Re-read the relevant section of `docs/design.md`.  
2. Prefer the **simpler** option that preserves LLB-primary.  
3. If product ambiguity remains → comment on the issue with options; label `needs-decision`; **stop**—do not invent product policy.  

---

## Repository map (target)

```text
lamina/
  AGENTS.md                 # this file
  README.md
  Cargo.toml                # workspace
  crates/
    lamina-cli/             # binary: lamina
    lamina/                 # syntax, sema, eval, ir
    lamina-llb/             # IR → pb.Definition
    lamina-client/          # BuildKit gRPC Solve
  docs/
    design.md               # architecture + PR plan
    grammar.md              # frozen MVP (when added)
    buildkit-capability.md  # capability matrix (when added)
  examples/hello-static/
  tests/golden-llb/
```

Until scaffold lands, only `docs/` + tracking files exist—that is expected.

---

## Verify commands

Once the workspace exists:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p lamina-cli -- --version
cargo run -p lamina-cli -- check          # when implemented
cargo run -p lamina-cli -- explain --target app
# integration (needs BuildKit / buildx):
cargo test -p lamina-cli -- --ignored
```

Before BuildKit exists, unit + golden tests are enough for language/IR/LLB-summary work.

---

## Milestones (work division)

| Milestone | Theme | Exit |
|-----------|--------|------|
| **v0.1 — MVP (LLB)** | Single-file language + IR + LLB lower + `lamina build` + hello-static | Example image builds |
| **v0.2 — Composition** | Path modules, mounts/secrets, stdlib recipes, fmt | Multi-file + mounts |
| **v0.3 — Hardening** | Lint pack, platforms, more examples | CI `--deny` usable |
| **v1.0 — Stable** | Path lockfile, docs, stability | Tagged 1.0 |
| **Post-1.0** | Frontend gateway, remote modules, optional emit-dockerfile, LSP | Not blocking 1.0 |

Full issue DAG: [`docs/roadmap.md`](docs/roadmap.md). Epic [#1](https://github.com/VitorRamos/lamina/issues/1); implement **#2 → #17** for 0.1.

---

## Labels (how agents filter)

| Label | Meaning |
|-------|---------|
| `agent-ready` | Safe for an agent to implement without further product debate |
| `needs-design` | Design/docs change required first |
| `needs-decision` | Blocked on human product choice |
| `human-only` | Auth, secrets policy, releases, legal, force-push, etc. |
| `epic` | Tracking umbrella; do not implement the epic itself |
| `area/compiler` | lexer, parser, types, eval, IR |
| `area/llb` | lowerer, protos, goldens |
| `area/cli` | `lamina` commands, config |
| `area/buildkit` | gRPC client, solve, exporters |
| `area/docs` | design, grammar, README, AGENTS |
| `area/examples` | example projects |
| `type/feat` `type/chore` `type/docs` `type/release` | Kind |
| `size/S` `size/M` `size/L` | Rough effort (S ≤ ~1 session, M ~1–2, L multi-session) |

---

## Coding conventions

- **Rust** edition 2021+, workspace dependency hygiene, `thiserror` / `miette` for user-facing errors.  
- **Spans everywhere** on AST/diagnostics once diagnostics land.  
- **No unwrap in library paths** that become user errors—surface diagnostics.  
- **CLI:** `clap`, subcommands named as in design (`check`, `explain`, `build`, `emit-llb`).  
- **Tests:** pure unit tests for solve_set / types / eval; `insta` for explain & LLB summaries; integration `#[ignore]` without daemon.  
- **Protos:** pin BuildKit proto versions; document in capability matrix.  

### Naming

- Language/user-facing: Lamina, `lamina`, `Lamina.toml`, `.lam`  
- Never reintroduce Docklang / `dockc` / `Dock.toml` / `.dk` in new code  

---

## PR hygiene

```text
type(scope): short summary

# body: why + what
# Closes #N
```

Types: `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `release`.

One logical change per PR. Do not bundle “while I was here” cleanups.

---

## Security & safety for agents

- Do **not** push with `--force` to `main`.  
- Do **not** commit secrets, tokens, or private keys.  
- Do **not** run destructive git (`reset --hard`, force-push) unless the user explicitly asks.  
- Do **not** expand scope into remote module registries, Dockerfile export, or CI platforms in 0.1.  
- Network to BuildKit/Docker only when the issue requires solve/integration tests.  

---

## Suggested agent session prompts

**Implement next ready issue:**

> Read AGENTS.md, docs/design.md, and docs/roadmap.md. List open milestone v0.1 issues labeled `agent-ready` whose dependencies are closed (see issue comments / Depends on). Pick the lowest issue number (first session: #2 scaffold). Implement only that issue. Open a PR with Closes #N.

**Review mode:**

> Review the current branch against its linked issue acceptance criteria and AGENTS.md non-negotiables. Do not implement; report blockers.

**Design change:**

> Propose a design.md edit as a docs PR first; do not implement language surface changes until the design PR merges.

---

## Quick links

- Design: [`docs/design.md`](docs/design.md)  
- Issues: https://github.com/VitorRamos/lamina/issues  
- Milestones: https://github.com/VitorRamos/lamina/milestones  
- Repo: https://github.com/VitorRamos/lamina  
