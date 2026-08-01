# Remote modules (1.1)

**Status:** Phase A decisions **accepted** (product defaults). Implementation tracks [#27](https://github.com/VitorRamos/lamina/issues/27).

## Phase A decisions

| ID | Decision |
|----|----------|
| **A1** | Syntax **option A**: `use "git+https://host/repo.git?ref=TAG&path=file.lam";` also `git+ssh://…`, and `git+file://…` (tests/local only) |
| **A2** | Lock: `spec`, `resolved` (canonical), `sha256` of file bytes, optional `commit`, `kind` (`path` / `std` / `git`) |
| **A3** | Only `git+https` / `git+ssh` / `git+file` (no bare `http://`). Cache: `LAMINA_MODULE_CACHE` or `~/.cache/lamina/modules`. `LAMINA_OFFLINE=1` or `--locked` with warm content cache: no network. |
| **A4** | `std/…` stays local/bundled only |

### Update behavior

- `lamina lock` — resolve all `use` (may fetch), rewrite `Lamina.lock`
- `lamina check --locked` / `build --locked` — verify hashes; may use content-addressed cache without network if blobs present
- No automatic major upgrades

## Syntax

```lam
use "git+https://github.com/acme/images.git?ref=v1.0.0&path=rust/mod.lam";
use "git+ssh://git@github.com/acme/images.git?ref=main&path=lib.lam";
```

Query parameters:

| Param | Required | Meaning |
|-------|----------|---------|
| `path` | yes | Path inside the repo to a `.lam` file |
| `ref` | yes* | Branch, tag, or commit (`*` required for non-file remotes; `git+file` may default to `HEAD`) |

## Cache layout

```text
$LAMINA_MODULE_CACHE/   # or ~/.cache/lamina/modules
  git/<id>/             # shallow clone keyed by url
  blob/<sha256>         # content-addressed module source (offline)
```

## Security notes

- Prefer tags/commits in `ref`; avoid floating `main` in production locks
- Private repos: system Git credentials / SSH agent (not Lamina config)
- Do not put tokens in `use` strings (they land in lockfiles and logs)

## Out of scope (this ship)

- Package registry / `lamina publish`
- Sigstore / signed modules
- Raw HTTP file URLs
- Nested git submodules
