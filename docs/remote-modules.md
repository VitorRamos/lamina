# Remote modules (1.1)

**Status:** Phase A decisions **accepted** (product defaults). Implementation tracks [#27](https://github.com/VitorRamos/lamina/issues/27).

## Phase A decisions

| ID | Decision |
|----|----------|
| **A1** | Syntax **option A**: `use "git+https://host/repo.git?ref=TAG&path=file.lam";` also `git+ssh://…`, and `git+file://…` (tests/local only). **Shorthand (GitHub):** `use "github:owner/repo/path.lam[@ref]";` (alias `gh:`; default ref `main`) expands to `git+https://github.com/…` |
| **A2** | Lock: `spec`, `resolved` (canonical), `sha256` of file bytes, optional `commit`, `kind` (`path` / `std` / `git`) |
| **A3** | Only `git+https` / `git+ssh` / `git+file` (no bare `http://`). Cache: `LAMINA_MODULE_CACHE` or `~/.cache/lamina/modules`. `LAMINA_OFFLINE=1` or `--locked` with warm content cache: no network. |
| **A4** | `std/…` stays local/bundled only |

### Update behavior

- `lamina lock` — resolve all `use` (may fetch), rewrite `Lamina.lock`
- `lamina check --locked` / `build --locked` — verify hashes; may use content-addressed cache without network if blobs present
- No automatic major upgrades

## Syntax

```lam
// Full form (any host)
use "git+https://github.com/acme/images.git?ref=v1.0.0&path=rust/mod.lam";
use "git+ssh://git@github.com/acme/images.git?ref=main&path=lib.lam";

// GitHub shorthand (same as the first example with ref=main)
use "github:acme/images/rust/mod.lam";
use "github:acme/images/rust/mod.lam@v1.0.0";
use "gh:acme/images/lib.lam@main";   // alias
```

### Full `git+` form — query parameters

| Param | Required | Meaning |
|-------|----------|---------|
| `path` | yes | Path inside the repo to a `.lam` file |
| `ref` | yes* | Branch, tag, or commit (`*` required for non-file remotes; `git+file` may default to `HEAD`) |

### `github:` / `gh:` shorthand

| Piece | Meaning |
|-------|---------|
| `owner/repo` | First two path segments → `https://github.com/owner/repo.git` |
| rest | Path inside the repo (must end in `.lam`) |
| `@ref` | Optional branch, tag, or commit; **defaults to `main`** |

Lockfiles always store the **canonical** `git+https://…?ref=&path=` form, so shorthand and full form share one lock entry.

### Nested deps inside the same repo

A remote module **may** `use "./sibling.lam"` or `use "../other/mod.lam"` as long as the path stays **inside the cloned repository**. Those relative imports:

1. Resolve against the file’s directory in the module cache (not your project root).
2. Are locked as stable `git+…?path=…` keys (not bare `./sibling.lam`), so they don’t collide with your app’s local paths.
3. Re-export `pub fn` transitively the same way path modules do.

Cross-repo deps from a remote package should use another `git+…` `use` (or publish a single entrypoint that re-exports).

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
