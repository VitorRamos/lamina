# Lamina user guide (1.0)

Lamina is a typed language for building container images. Sources (`.lam`) compile to a Build IR / LLB graph and solve through BuildKit (via Docker Buildx).

```text
.lam  →  lamina  →  IR / LLB  →  BuildKit  →  OCI image
```

## Install / build

```bash
git clone git@github.com:VitorRamos/lamina.git
cd lamina
cargo build -p lamina-cli --release
# binary: target/release/lamina
```

Requires Docker with Buildx for `lamina build`.

## Project layout

```text
myapp/
  Lamina.toml
  Lamina.lock          # optional; from `lamina lock`
  src/
    image.lam
    helpers.lam        # path module
  .dockerignore
```

CLI commands that take a project path (`check`, `build`, `explain`, …) look for
`Lamina.toml` in that directory, then in a nested **`.lamina/`** directory.
So from an application repo root you can keep the Lamina project out of the way:

```text
myapp/
  Cargo.toml           # (or go.mod, package.json, …)
  src/                 # application source
  .lamina/
    Lamina.toml
    src/
      image.lam
```

```bash
# either works when Lamina.toml lives under .lamina/
lamina check
lamina check .lamina
```

If both `./Lamina.toml` and `./.lamina/Lamina.toml` exist, the root file wins.
Set `[build] context = ".."` in `.lamina/Lamina.toml` when sources live in the parent repo.

### Lamina.toml

```toml
[package]
name = "myapp"
entry = "src/image.lam"

[params]
# compile-time defaults for param("key", default)

[build]
context = "."
platforms = ["linux/amd64"]

[lint]
deny = ["secret-env", "unpinned-base"]
```

## Language essentials

```lam
use "./helpers.lam";
use "std/golang.lam";

pub target app = {
  let builder = Stage.from("golang:1.22-bookworm")
    .workdir("/src")
    .run("go build -o /out/app")
    .name("builder");

  Stage.from("gcr.io/distroless/static-debian12:nonroot")
    .copy_from(builder, "/out/app", "/app")
    .entrypoint(["/app"])
    .name("app")
};
```

- **Stages** are immutable values; methods return new stages.
- **`pub target`** is an exportable image root.
- **`use`** imports `pub fn` from path modules or `std/…`.
- **Mounts:** `Mount.cache` / `secret` / `ssh` / `bind` with `.run_with(cmd, mounts)`.
- **Platform:** `.platform("linux/amd64")`.
- **Lists:** literals `["a", "b"]`; concat with `+` when element types match (`base + more`).

### Stdlib recipes (`std/…`)

| Module | Import | Highlights |
|--------|--------|------------|
| Go | `use "std/golang.lam";` | `from_version`, `with_modules`, `build_release`, `build_with_cache` |
| Node | `use "std/node.lam";` | `from_version`, `npm_ci`, `npm_ci_with_cache` |
| Rust | `use "std/rust.lam";` | `from_version`, `cargo_registry_mounts`, `cargo_build_mounts`, `build_release_bin` / `_musl`, `rustup_target` |
| Python | `use "std/python.lam";` | `from_version`, `pip_install`, `with_app` |

Rust cache mounts intentionally **omit** `registry/src` (concurrent unpack races). Pass a unique `target_id` per parallel build stage to `cargo_build_mounts`. For monorepos that bind-mount sources (instead of `.copy`), use the mount helpers with your own `.run_with` — see `examples/stdlib-rust`.

See `docs/grammar.md` and `docs/design.md` for full surface.

## CLI

| Command | Purpose |
|---------|---------|
| `lamina check [PATH]` | Parse, typecheck, IR, lints |
| `lamina check --deny all` | Fail on any lint |
| `lamina check --locked` | Verify `Lamina.lock` |
| `lamina lock [PATH]` | Write `Lamina.lock` |
| `lamina explain --target NAME` | Solve-set / stage summary |
| `lamina emit-llb --target NAME` | Stable LLB op text |
| `lamina emit-dockerfile --target NAME` | **Lossy debug only** |
| `lamina build --target NAME -t REF` | Build via Buildx |
| `lamina build --platform a,b --push` | Multi-arch (push required) |
| `lamina clear [PATH]` | Remove project images + local build cache |
| `lamina clear --dry-run` | Show what would be removed |
| `lamina fmt [PATH]` | Format sources |

### Clearing build artifacts

`lamina build` loads images into the local Docker engine and writes a project-local
BuildKit layer cache under `.lamina/build-cache`. Images are labeled with
`com.lamina.project` and `com.lamina.project-root` so they can be found later.

```bash
lamina build --target app -t myapp:dev
lamina clear                 # remove labeled images, myapp:dev, and .lamina/build-cache
lamina clear -t myapp:other  # also drop extra tags
lamina clear --dry-run       # preview
```

This does **not** wipe the shared Docker/Buildx builder cache used by other
projects. For a full builder prune: `docker buildx prune`.

## Lockfile

`lamina lock` records every `use` resolution (spec → path + sha256, plus git commit when remote).

```bash
lamina lock examples/compose-demo
lamina check examples/compose-demo --locked
```

Commit `Lamina.lock` for CI reproducibility of path/stdlib/**git** modules.

## Remote modules (git)

Syntax (see also [`remote-modules.md`](remote-modules.md)):

```lam
use "git+https://github.com/acme/images.git?ref=v1.0.0&path=rust/mod.lam";
```

- **Schemes:** `git+https://`, `git+ssh://`, `git+file://` (local tests)
- **Required query:** `ref=…`, `path=…` (to a `.lam` file)
- **Cache:** `LAMINA_MODULE_CACHE` or `~/.cache/lamina/modules`
- **Offline:** `LAMINA_OFFLINE=1` uses existing cache only (no `git clone`)

```bash
lamina lock          # may fetch
LAMINA_OFFLINE=1 lamina check --locked
```

Private repos use normal Git credentials / SSH agent — do not put tokens in `use` strings.

## Lints

| ID | Meaning |
|----|---------|
| `empty-stage` | Named stage with no real work |
| `unused-stage` | Named stage not in solve_set |
| `root-final` | Export looks like a builder |
| `secret-env` | ENV key looks secret-like |
| `unpinned-base` | Floating tag / no digest |

## What not to do

- Do not treat `emit-dockerfile` output as source of truth.
- Do not pass secrets via `--param` / `--build-arg`; use `Mount.secret`.
- Do not expect plain `docker build -f Dockerfile` on `.lam` sources without `lamina`.

## Language server

```bash
lamina lsp
# or: lamina-lsp
```

See [`lsp.md`](lsp.md) for Helix / Neovim / VS Code setup (diagnostics, hover, goto, format).

## Further reading

- Architecture: [`design.md`](design.md)
- Grammar: [`grammar.md`](grammar.md)
- Capability matrix: [`buildkit-capability.md`](buildkit-capability.md)
- Remote modules: [`remote-modules.md`](remote-modules.md)
- Agents / contributors: [`../AGENTS.md`](../AGENTS.md)
