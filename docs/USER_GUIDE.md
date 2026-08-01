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
| `lamina fmt [PATH]` | Format sources |

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
