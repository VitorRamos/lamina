# Lamina

**Lamina** is a typed programming language for building container images. It compiles to [BuildKit](https://github.com/moby/buildkit) **LLB** and produces **OCI** images—without generating Dockerfiles.

```text
.lam  →  lamina  →  LLB  →  BuildKit  →  OCI image
```

> *Lamina* means a thin layer or sheet: image builds as composed layers and stages.

## Status

Greenfield / design phase.

- Architecture: [`docs/design.md`](docs/design.md)
- Agent workflow: [`AGENTS.md`](AGENTS.md)
- Roadmap map: [`docs/roadmap.md`](docs/roadmap.md)
- Issues: https://github.com/VitorRamos/lamina/issues
- Milestones: https://github.com/VitorRamos/lamina/milestones

| | |
|---|---|
| Language | Lamina |
| CLI | `lamina` |
| Config | `Lamina.toml` |
| Sources | `*.lam` |

## Build from source

Requirements: Rust 1.74+ (edition 2021), Cargo.

```bash
git clone git@github.com:VitorRamos/lamina.git
cd lamina
cargo build -p lamina-cli
cargo run -p lamina-cli -- --version
cargo test --workspace
```

The CLI binary is named **`lamina`** (crate `lamina-cli`).

## CLI (0.3)

```text
lamina check [PATH] [--deny LINT|all] [--list-lints]
lamina explain [PATH] --target NAME
lamina emit-llb [PATH] --target NAME
lamina build [PATH] --target NAME -t REF [--platform P] [--push] [--deny LINT]
lamina fmt [PATH|FILE…] [--check]
```

Examples:

```bash
cargo run -p lamina-cli -- check examples/hello-static
cargo run -p lamina-cli -- check examples/hello-static --deny unpinned-base
cargo run -p lamina-cli -- build examples/platform-demo --target app -t platform-demo:dev --platform linux/amd64
cargo run -p lamina-cli -- fmt examples/compose-demo
```

Modules: `use "./lib.lam";` or `use "std/golang.lam";`.  
Lints: `empty-stage`, `unused-stage`, `root-final`, `secret-env`, `unpinned-base`.
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

TBD.
