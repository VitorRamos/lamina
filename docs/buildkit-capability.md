# BuildKit capability matrix (0.1)

| Capability | Mechanism | Status |
|------------|-----------|--------|
| Pull/base image | `Stage.from` → image source | supported |
| Exec shell | `.run` → exec | supported |
| Local context copy | `.copy` / `.copy_many` | supported |
| Cross-stage copy | `.copy_from` | supported |
| Image config (USER/ENV/ENTRYPOINT/CMD/EXPOSE) | stage instrs | supported |
| Workdir | `.workdir` | supported |
| Build-arg binding | `arg` / `from_arg` + CLI `--build-arg` (pre-lower) | supported |
| LLB op summary | `lamina emit-llb` | supported |
| Solve via Buildx | `lamina build` → internal bridge → `docker buildx build` | supported (0.1 hybrid) |
| Raw `pb.Definition` gRPC Solve | tonic client | deferred (post-0.1 refinement) |
| Cache/secret/ssh/bind mounts | `Mount.*` + `.run_with` → Dockerfile `--mount` bridge | supported (0.2) |
| LABEL / HEALTHCHECK | `.label` / `.healthcheck` | supported (0.2) |
| Path modules + stdlib | `use` + `stdlib/` | supported (0.2) |
| Multi-platform | `.platform` + `buildx --platform` / `--push` | supported (0.3) |
| IR lints + `--deny` | empty/unused/root-final/secret-env/unpinned | supported (0.3) |
| Path lockfile | `Lamina.lock` + `--locked` | supported (1.0) |
| Lossy Dockerfile dump | `emit-dockerfile` (debug only) | supported (1.0) |
| Frontend gateway for `.lam` | | post-1.0 |

## 0.1 solve bridge note

Stock Docker Buildx speaks the Dockerfile frontend by default. Lamina **0.1** lowers IR to an **ephemeral internal Dockerfile** used only as a Solve input (temp dir, never project source). Product contract remains:

- authoring surface: `.lam` + `Lamina.toml`
- review surface: `.lam` / `explain` / `emit-llb`
- **not** committed Dockerfiles

Raw LLB `pb.Definition` Solve is the design end-state; the bridge is the documented solo-velocity hybrid.
