# Contributing to Lamina

Thanks for contributing. This doc is the short path for humans and agents.
Full agent workflow, labels, and non-negotiables live in [`AGENTS.md`](AGENTS.md).
Product and architecture source of truth: [`docs/design.md`](docs/design.md).

## Before you start

1. Prefer an open issue labeled **`agent-ready`** whose dependencies are closed.
2. Skip **`needs-design`**, **`needs-decision`**, and **`human-only`** unless assigned.
3. One issue ≈ one PR. Link `Closes #N` in the description.

Issue labels and milestone rules: see **Labels** and **Picking work** in [`AGENTS.md`](AGENTS.md).

## Branch and PR style

```text
branch:  agent/<issue-number>-short-slug
         # or feat/…, fix/…, docs/…, chore/…
```

**Commit / PR title:**

```text
type(scope): short summary
```

Types: `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `release`.

**PR body:** what / why / how to test; end with `Closes #N`.

## Verify before opening a PR

From the repo root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                          # licenses + advisories (see deny.toml)
cargo run -p lamina-cli -- --version
cargo run -p lamina-cli -- check examples/hello-static
```

Optional when you touch format / IR:

```bash
cargo run -p lamina-cli -- fmt --check examples/
# golden / insta updates only when IR or LLB summaries change
```

CI (fmt, clippy, tests, example `check`, `cargo deny`) must stay green on PR and `main`.  
See [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Non-negotiables (short)

- **LLB-primary** — no Dockerfile as the product correctness path.
- **No secrets in params** — never put credentials in `--param` / `--build-arg`.
- **No drive-by refactors** — only touch what the issue needs.
- **Design wins** — if code and `docs/design.md` disagree, fix code or change design first.

## Security

- Do **not** commit secrets, tokens, or private keys.
- Do **not** force-push `main` or run destructive git unless the maintainers ask.
- Dependency policy: root [`deny.toml`](deny.toml). Local refresh of the RustSec DB:

  ```bash
  cargo install cargo-deny --locked   # once
  cargo deny fetch
  cargo deny check
  ```

  CI re-fetches the advisory database on each run; there is no vendored DB in-repo.

## Release checklist

Full detail: [`docs/RELEASE.md`](docs/RELEASE.md). Short version:

1. Bump `[workspace.package] version` (and path crate versions) in root `Cargo.toml`.
2. Update [`CHANGELOG.md`](CHANGELOG.md) (move Unreleased → version section).
3. Merge to `main`.
4. Tag and push: `vX.Y.Z` (GitHub Release automation is **major-only**, e.g. `v2.0.0` — see release doc).
5. Optional: publish crates.io packages in dependency order (`lamina-lang` → … → `lamina-cli`).

## Docs map

| Doc | Use |
|-----|-----|
| [`AGENTS.md`](AGENTS.md) | Agent loop, labels, verify commands |
| [`docs/design.md`](docs/design.md) | Architecture + language contract |
| [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) | User-facing CLI / language |
| [`docs/roadmap.md`](docs/roadmap.md) | Issue map by milestone |
| [`docs/RELEASE.md`](docs/RELEASE.md) | CI and release process |
| [`docs/lsp.md`](docs/lsp.md) | Language server |
| [`docs/remote-modules.md`](docs/remote-modules.md) | Git `use` modules |

Questions about product policy → open or comment on a GitHub issue with `needs-decision`; do not invent policy in a PR.
