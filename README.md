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

## Planned CLI

```text
lamina check
lamina explain [--target NAME]
lamina build  [--target NAME] [-t REF]
lamina emit-llb [--target NAME]   # debug
```

## Repository

- GitHub: https://github.com/VitorRamos/lamina  
- Clone: `git clone git@github.com:VitorRamos/lamina.git`

## License

TBD.
