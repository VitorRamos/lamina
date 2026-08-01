# Lamina Language Server

`lamina-lsp` (also `lamina lsp`) provides editor integration for `.lam` files.

## Features (MVP)

| Feature | Support |
|---------|---------|
| Diagnostics | Parse + typecheck (+ module load offline) |
| Hover | Symbols, Stage/Mount intrinsics |
| Goto definition | Local `fn` / `const` / `let` / `target`; path `use` files |
| Completion | Stage methods after `.` |
| Format | Uses `lamina::fmt` |

Remote `git+` modules: offline only in the LSP (uses warm module cache). Unresolved remotes surface as diagnostics.

## Run

```bash
cargo run -p lamina-lsp
# or
cargo run -p lamina-cli -- lsp
```

## Helix

```toml
# ~/.config/helix/languages.toml
[[language]]
name = "lamina"
scope = "source.lamina"
file-types = ["lam"]
language-servers = ["lamina-lsp"]

[language-server.lamina-lsp]
command = "lamina-lsp"
# or: command = "lamina"
# args = ["lsp"]
```

Build and put `lamina-lsp` on `PATH`:

```bash
cargo install --path crates/lamina-lsp
```

## Neovim (nvim-lspconfig style)

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "lamina",
  callback = function()
    vim.lsp.start({
      name = "lamina-lsp",
      cmd = { "lamina-lsp" },
      root_dir = vim.fs.root(0, { "Lamina.toml", ".git" }),
    })
  end,
})

vim.filetype.add({ extension = { lam = "lamina" } })
```

## VS Code

Use a generic LSP extension (e.g. “LSP Link” / “vscode-glspc”) pointing at `lamina-lsp`, with file association for `*.lam`. A dedicated extension is not shipped yet.

## Project root

The server walks parents for `Lamina.toml`; otherwise it uses the file’s directory. Path `use` resolution matches the compiler.
