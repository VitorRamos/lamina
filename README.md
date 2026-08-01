# Lamina

**Lamina** is a typed programming language for building container images. It compiles to [BuildKit](https://github.com/moby/buildkit) **LLB** and produces **OCI** images—without generating Dockerfiles.

```text
.lam  →  lamina  →  LLB  →  BuildKit  →  OCI image
```

> *Lamina* means a thin layer or sheet: image builds as composed layers and stages.

## Status

**1.0.0** — path modules, mounts, lints, multi-platform, lockfile.

- User guide: [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md)
- Architecture: [`docs/design.md`](docs/design.md)
- Post-1.0 plan: [`docs/post-1.0.md`](docs/post-1.0.md)
- Agent workflow: [`AGENTS.md`](AGENTS.md)
- Roadmap: [`docs/roadmap.md`](docs/roadmap.md)
- Issues: https://github.com/VitorRamos/lamina/issues

| | |
|---|---|
| Language | Lamina |
| CLI | `lamina` |
| Config | `Lamina.toml` / `Lamina.lock` |
| Sources | `*.lam` |

## Build from source

Requirements: Rust 1.74+ (edition 2021), Cargo; Docker Buildx for `build`.

```bash
git clone git@github.com:VitorRamos/lamina.git
cd lamina
cargo build -p lamina-cli --release
cargo run -p lamina-cli -- --version
cargo test --workspace
```

## CLI (1.0)

```text
lamina check [PATH] [--deny LINT|all] [--locked] [--list-lints]
lamina lock [PATH]
lamina explain [PATH] --target NAME
lamina emit-llb [PATH] --target NAME
lamina emit-dockerfile [PATH] --target NAME   # lossy debug only
lamina build [PATH] --target NAME -t REF [--platform P] [--push] [--deny LINT] [--locked]
lamina fmt [PATH|FILE…] [--check]
lamina lsp                              # Language Server (stdio)
```

```bash
cargo run -p lamina-cli -- check examples/hello-static
cargo run -p lamina-cli -- lock examples/compose-demo
cargo run -p lamina-cli -- check examples/compose-demo --locked
cargo run -p lamina-cli -- build examples/platform-demo --target app -t platform-demo:dev --platform linux/amd64
```
## Workspace layout

```text
crates/
  lamina/       # language library
  lamina-cli/   # binary: lamina
docs/
  design.md
  roadmap.md
AGENTS.md
```

## Repository

- GitHub: https://github.com/VitorRamos/lamina  
- Clone: `git clone git@github.com:VitorRamos/lamina.git`  
- Issues: https://github.com/VitorRamos/lamina/issues  
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
