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
| Cache/secret/ssh mounts | | 0.2 |
| Frontend gateway for `.lam` | | post-1.0 |

## 0.1 solve bridge note

Stock Docker Buildx speaks the Dockerfile frontend by default. Lamina **0.1** lowers IR to an **ephemeral internal Dockerfile** used only as a Solve input (temp dir, never project source). Product contract remains:

- authoring surface: `.lam` + `Lamina.toml`
- review surface: `.lam` / `explain` / `emit-llb`
- **not** committed Dockerfiles

Raw LLB `pb.Definition` Solve is the design end-state; the bridge is the documented solo-velocity hybrid.
