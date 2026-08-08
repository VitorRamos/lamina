# Lamina

**Lamina** is a typed programming language for building container images. It compiles to [BuildKit](https://github.com/moby/buildkit) **LLB** and produces **OCI** images—without generating Dockerfiles.

```text
.lam  →  lamina  →  LLB  →  BuildKit  →  OCI image
```

> *Lamina* means a thin layer or sheet: image builds as composed layers and stages.

## Status

**1.4.0** (stable 1.x + post-1.0 features) — path modules, mounts/secrets, IR lints (`--deny`), multi-platform, `Lamina.lock`, **git remotes**, **LSP**, multiline strings, list concat, `lamina clear`, stdlib recipes, `.lamina/` project discovery.

| | |
|---|---|
| Language | Lamina |
| CLI | `lamina` |
| Config | `Lamina.toml` / `Lamina.lock` |
| Sources | `*.lam` |
| License | MIT OR Apache-2.0 |

### Docs

| Topic | Link |
|-------|------|
| User guide | [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) |
| Architecture | [`docs/design.md`](docs/design.md) |
| Remote git modules | [`docs/remote-modules.md`](docs/remote-modules.md) |
| Language server | [`docs/lsp.md`](docs/lsp.md) |
| Contributing / PR workflow | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Agent workflow | [`AGENTS.md`](AGENTS.md) |
| Roadmap | [`docs/roadmap.md`](docs/roadmap.md) |
| Post-1.0 plan | [`docs/post-1.0.md`](docs/post-1.0.md) |
| Releases | [`docs/RELEASE.md`](docs/RELEASE.md) |
| Issues | https://github.com/VitorRamos/lamina/issues |

## Build from source

Requirements: Rust 1.74+ (edition 2021), Cargo; Docker Buildx for `build`.

```bash
git clone git@github.com:VitorRamos/lamina.git
cd lamina
cargo build -p lamina-cli --release
cargo run -p lamina-cli -- --version
cargo test --workspace
```

**CI:** every PR and push to `main` must stay green (fmt, clippy, tests, example `lamina check`, `cargo deny`). See [`.github/workflows/ci.yml`](.github/workflows/ci.yml). Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md). Releases: [`docs/RELEASE.md`](docs/RELEASE.md).

## CLI

```text
lamina check [PATH] [--deny LINT|all] [--locked] [--list-lints]
lamina lock [PATH]
lamina explain [PATH] [--target NAME] [--all-targets]
lamina emit-llb [PATH] [--target NAME] [--all-targets]
lamina emit-dockerfile [PATH] [--target NAME] [--all-targets]   # lossy debug only
lamina build [PATH] [--target NAME] [--all-targets] -t REF [--platform P] [--push] [--deny LINT] [--locked]
lamina clear [PATH] [-t REF] [--dry-run]   # remove project images + build cache
lamina fmt [PATH|FILE…] [--check]
lamina lsp                              # Language Server (stdio)
```

```bash
cargo run -p lamina-cli -- check examples/hello-static
cargo run -p lamina-cli -- lock examples/compose-demo
cargo run -p lamina-cli -- check examples/compose-demo --locked
cargo run -p lamina-cli -- build examples/platform-demo --target app -t platform-demo:dev --platform linux/amd64
# language tour (keywords + Stage/Mount surface):
cargo run -p lamina-cli -- check examples/kitchen-sink
cargo run -p lamina-cli -- explain examples/kitchen-sink --target app
```
## Workspace layout

```text
crates/
  lamina-lang/    # language library (crates.io name; was name-squatted as `lamina`)
  lamina-cli/     # binary: lamina
  lamina-llb/     # IR → LLB summary / lower helpers
  lamina-client/  # BuildKit / Buildx solve client
  lamina-lsp/     # language server (also: lamina lsp)
docs/
  design.md
  USER_GUIDE.md
  remote-modules.md
  lsp.md
  roadmap.md
  post-1.0.md
  RELEASE.md
stdlib/           # std/*.lam recipes (rust, python, golang, node)
examples/         # hello-static, compose-demo, kitchen-sink, …
AGENTS.md
CONTRIBUTING.md
deny.toml         # cargo-deny (licenses + advisories)
```

## Repository

- GitHub: https://github.com/VitorRamos/lamina  
- Clone: `git clone git@github.com:VitorRamos/lamina.git`  
- Issues: https://github.com/VitorRamos/lamina/issues  
- Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md)  
- Agent guide: [`AGENTS.md`](AGENTS.md)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Lamina by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
