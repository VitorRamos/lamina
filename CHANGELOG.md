# Changelog

## Unreleased

## 1.4.0 — 2026-08-06

### Added

- **`lamina clear`**: remove local images and project BuildKit cache from
  `lamina build` (labels `com.lamina.project` / `com.lamina.project-root`,
  default `{package}:dev`, `.lamina/build-cache`; `--dry-run`, `-t` extras).
- **List concatenation:** `List[T] + List[T]` (same element type), e.g. `["curl"] + ["jq", "git"]`.
- **Stdlib:** `std/rust.lam` (Cargo cache mounts, release/musl build helpers) and
  `std/python.lam` (`pip_install`); examples `stdlib-rust`, `stdlib-python`.
- **GitHub `use` shorthand:** `github:owner/repo/path.lam[@ref]` (alias `gh:`),
  default ref `main`; expands to `git+https://github.com/…` (stable lock key).
- **Multiline strings:** `"""…"""` (dedent + `${ident}`) and `r"""…"""` (raw).
- **`.run` / `.run_with`:** accept `List[String]` (joined with newlines) as well as `String`.
- **Stdlib `rust.lam`:** multi-step recipes use command lists + `set -eux` (not `"a && b"`).
- CI: `cargo deny` (licenses + RustSec advisories) via `deny.toml`
- `CONTRIBUTING.md` and refreshed README / Post-1.0 roadmap docs

### Fixed

- **`lamina build`**: skip `--cache-to type=local` on the stock Buildx **`docker`
  driver** (cache export unsupported); still uses local cache on
  `docker-container` and other export-capable drivers.
- **`lamina fmt`**: single-method chains stay inline (`s = s.run("…")`).
- **`lamina fmt`**: expand multi-element lists one item per line.

## 1.3.1 — 2026-08-05

### Added

- Project path discovery: if `Lamina.toml` is missing in the given directory,
  CLI/LSP also look under **`.lamina/`** (root `Lamina.toml` still wins when both exist).

## 1.3.0 — 2026-08-02

### Added

- **Assignment** in blocks: `name = expr;` (accumulate `Stage` across `for` loops)
- **`for` / `if` as statements** may omit trailing `;`
- Example **`examples/kitchen-sink`**: language tour multi-stage Go service
- CI: fmt, clippy, test, example `lamina check` on every PR / `main`
- Major-only GitHub Release workflow (`v*.0.0`) + `docs/RELEASE.md`
- Dual license files (`LICENSE-MIT`, `LICENSE-APACHE`)

### Fixed

- `lamina build` streams Buildx progress (no buffered output)
- Internal Dockerfile bridge quotes `ENV`/`LABEL` values with spaces
- `Stage.arg` parses (keywords allowed as method names after `.`)
- Secret/SSH mounts emit `required=false` so demos solve without credentials
- `lamina fmt`: dense `use`/`const`/`let` grouping (hand-written style); method chains still break

### Changed

- Crates.io library package renamed **`lamina` → `lamina-lang`** (name taken on crates.io).
  Path: `crates/lamina-lang`. Rust imports: `lamina_lang::…`.
  CLI binary remains **`lamina`** (`lamina-cli`).

## 1.2.0 — 2026-08-01

### Added

- **Language server:** `lamina-lsp` / `lamina lsp` (stdio)
  - Diagnostics (parse + typecheck)
  - Hover, goto definition, Stage method completion, format
- Docs: `docs/lsp.md`

## 1.1.0 — 2026-08-01

### Added

- Remote git modules: `use "git+https://…?ref=&path=";` (`git+ssh`, `git+file`)
- Module cache (`LAMINA_MODULE_CACHE` / `~/.cache/lamina/modules`) + content blobs
- `LAMINA_OFFLINE=1` for no-fetch resolution
- Lockfile records `kind` + optional `commit` for git modules
- Docs: `docs/remote-modules.md` (Phase A decisions accepted)

## 1.0.0 — 2026-08-01

### Added

- Path module lockfile: `Lamina.lock` via `lamina lock`, verify with `--locked`
- `lamina emit-dockerfile` — **lossy debug export only** (warned; not product)
- User guide: `docs/USER_GUIDE.md`
- Example lock committed for `examples/compose-demo`

### Stability

- Language surface through 0.3 (modules, mounts, lints, platforms) is 1.0
- Remote modules / registry remain post-1.0

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
