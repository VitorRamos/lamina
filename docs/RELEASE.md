# Releases

## Continuous integration

Every **pull request** and every push to **`main`** runs [`.github/workflows/ci.yml`](../.github/workflows/ci.yml):

- `cargo fmt --check`
- `cargo clippy -D warnings`
- `cargo test --workspace`
- `lamina check` on each `examples/*` project

No Docker/BuildKit is required for CI. Green checks are expected before merge.

## Versioning

Workspace version lives in root `Cargo.toml` (`[workspace.package] version`).

| Kind | Example tags | Automation |
|------|----------------|------------|
| **Major** | `v2.0.0`, `v3.0.0` | [`.github/workflows/release.yml`](../.github/workflows/release.yml) builds `lamina` and opens a **GitHub Release** with a linux amd64 binary |
| **Minor / patch** | `v1.2.0`, `v1.2.1` | **No** release workflow — cut notes in `CHANGELOG.md` and tag manually if you want a git tag only |
| **crates.io** | any | **Manual only** (not wired to CI). Name on crates.io is `lamina-lang` (not `lamina`). |

### Why major-only GitHub Releases?

Publishing and artifact pipelines are still settling. Major tags are rare and intentional; minor/patch stay lightweight until we want every tag to ship binaries.

To try a release build without tagging:

```text
Actions → Release (major) → Run workflow → dry_run = true
```

## Manual crates.io publish (optional)

After a release (any version), from a clean `main` matching the version:

```bash
cargo publish -p lamina-lang
cargo publish -p lamina-llb
cargo publish -p lamina-client
cargo publish -p lamina-lsp
cargo publish -p lamina-cli
```

Wait for the index to pick up each crate before the next. Requires crates.io login and ownership of those names.

## Checklist (major)

1. Update `[workspace.package] version` and dependency versions in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Merge to `main`, tag `vX.0.0`, push tag
4. Confirm GitHub Release assets
5. (Optional) crates.io publish steps above
