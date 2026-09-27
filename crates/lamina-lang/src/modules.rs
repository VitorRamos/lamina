//! Module loading: path, stdlib, and git remotes (`use "…";`).

use crate::ast::{Block, BlockStmt, Expr, ExprKind, FnDecl, Item, Module};
use crate::diag::{CompileError, DiagnosticMsg, Result};
use crate::git_remote::{self, parse_remote_use, to_git_plus_spec};
use crate::lock::{hash_file, ModuleKind, ResolvedModule};
use crate::parser::parse;
use crate::span::{FileId, SourceFile};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};

/// Prefix for helpers copied into an importer so exported bodies can call them
/// without putting those names in the importer's scope.
const INTERNAL_DEP_PREFIX: &str = "__lamina_dep_";

#[derive(Debug, Clone)]
pub struct ModuleLoadContext {
    pub project_root: PathBuf,
    /// Directories searched for `std/…` imports (first hit wins).
    pub stdlib_paths: Vec<PathBuf>,
    /// Disallow network / git fetch (env `LAMINA_OFFLINE` also applies).
    pub offline: bool,
    /// Override module cache root (tests); else `LAMINA_MODULE_CACHE` / default.
    pub module_cache: Option<PathBuf>,
}

impl ModuleLoadContext {
    pub fn new(project_root: PathBuf) -> Self {
        let mut stdlib_paths = Vec::new();
        let mut seen = HashSet::new();
        let mut push = |p: PathBuf| {
            if seen.insert(p.clone()) {
                stdlib_paths.push(p);
            }
        };

        // 1) Explicit override
        if let Ok(p) = std::env::var("LAMINA_STDLIB") {
            push(PathBuf::from(p));
        }

        // 2) Walk ancestors: examples/stdlib-go → … → repo root/stdlib
        //    (CLI via cargo used CARGO_MANIFEST_DIR; LSP/editor does not.)
        let mut cur = project_root.clone();
        loop {
            push(cur.join("stdlib"));
            if !cur.pop() {
                break;
            }
        }

        // 3) Dev: workspace crate path → repo root
        if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
            let crate_dir = PathBuf::from(m);
            if let Some(repo) = crate_dir.parent().and_then(|p| p.parent()) {
                push(repo.join("stdlib"));
            }
        }

        // Keep only paths that exist so error messages stay honest
        // (still search non-existing last-resort via full list for first match).
        // Prefer existing dirs first for speed/clarity.
        let (existing, missing): (Vec<_>, Vec<_>) =
            stdlib_paths.into_iter().partition(|p| p.is_dir());
        let mut ordered = existing;
        ordered.extend(missing);

        Self {
            project_root,
            stdlib_paths: ordered,
            offline: false,
            module_cache: None,
        }
    }
}

pub struct LoadResult {
    pub module: Module,
    /// Deduped resolved modules (for lockfile).
    pub resolved: Vec<ResolvedModule>,
}

/// Expand `use` items: load modules and merge exported `pub fn` into the entry module.
pub fn load_and_merge(
    entry: &SourceFile,
    module: Module,
    ctx: &ModuleLoadContext,
) -> Result<LoadResult> {
    let mut visiting = HashSet::new();
    let mut cache: HashMap<PathBuf, Vec<FnDecl>> = HashMap::new();
    let mut resolved_map: HashMap<String, ResolvedModule> = HashMap::new();
    let mut next_file_id = entry.id.0 + 1;

    let entry_path = PathBuf::from(&entry.name);
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ctx.project_root.clone());

    let mut state = ExpandState {
        ctx,
        visiting: &mut visiting,
        cache: &mut cache,
        resolved_map: &mut resolved_map,
        next_file_id: &mut next_file_id,
    };
    let module = expand_uses(entry, module, &entry_dir, &mut state)?;

    let mut resolved: Vec<ResolvedModule> = resolved_map.into_values().collect();
    resolved.sort_by(|a, b| a.spec.cmp(&b.spec));

    Ok(LoadResult { module, resolved })
}

struct ExpandState<'a> {
    ctx: &'a ModuleLoadContext,
    visiting: &'a mut HashSet<PathBuf>,
    cache: &'a mut HashMap<PathBuf, Vec<FnDecl>>,
    resolved_map: &'a mut HashMap<String, ResolvedModule>,
    next_file_id: &'a mut u32,
}

fn expand_uses(
    file: &SourceFile,
    module: Module,
    from_dir: &Path,
    state: &mut ExpandState<'_>,
) -> Result<Module> {
    let mut merged_fns: Vec<FnDecl> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut rest: Vec<Item> = Vec::new();

    for item in &module.items {
        if let Item::Fn(f) = item {
            seen.insert(f.name.clone());
        }
    }

    for item in module.items {
        match item {
            Item::Use(u) => {
                let (path, kind, commit, lock_spec) =
                    resolve_use_path(&u.path, from_dir, state.ctx).map_err(|msg| {
                        CompileError::single(Some(file), DiagnosticMsg::error(msg, Some(u.span)))
                    })?;
                // Prefer a stable lock key (rewrites `./foo.lam` inside a git checkout
                // to `git+…?path=…` so nested deps don't collide across remotes).
                let record_spec = lock_spec.as_deref().unwrap_or(u.path.as_str());
                record_resolved(record_spec, &path, kind, commit, state.resolved_map)?;
                let exports = load_exports(&path, state)?;
                for f in exports {
                    if !seen.insert(f.name.clone()) {
                        // Same helper inlined for two modules that share a dependency.
                        if is_internal_dep(&f.name) {
                            continue;
                        }
                        return Err(CompileError::single(
                            Some(file),
                            DiagnosticMsg::error(
                                format!(
                                    "import conflict: function `{}` already defined (from {})",
                                    f.name,
                                    path.display()
                                ),
                                Some(u.span),
                            ),
                        ));
                    }
                    merged_fns.push(f);
                }
            }
            other => rest.push(other),
        }
    }

    let mut items: Vec<Item> = merged_fns.into_iter().map(Item::Fn).collect();
    items.extend(rest);
    Ok(Module {
        items,
        span: module.span,
    })
}

fn record_resolved(
    spec: &str,
    path: &Path,
    kind: ModuleKind,
    commit: Option<String>,
    resolved_map: &mut HashMap<String, ResolvedModule>,
) -> Result<()> {
    if resolved_map.contains_key(spec) {
        return Ok(());
    }
    let sha256 = hash_file(path).map_err(|e| {
        CompileError::single(
            None,
            DiagnosticMsg::error(format!("hash {}: {e}", path.display()), None),
        )
    })?;
    resolved_map.insert(
        spec.to_string(),
        ResolvedModule {
            spec: spec.to_string(),
            path: path.to_path_buf(),
            sha256,
            commit,
            kind,
        },
    );
    Ok(())
}

fn load_exports(path: &Path, state: &mut ExpandState<'_>) -> Result<Vec<FnDecl>> {
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(v) = state.cache.get(&key) {
        return Ok(v.clone());
    }
    if !state.visiting.insert(key.clone()) {
        return Err(CompileError::single(
            None,
            DiagnosticMsg::error(format!("cyclic module import: {}", path.display()), None),
        ));
    }

    let src = std::fs::read_to_string(path).map_err(|e| {
        CompileError::single(
            None,
            DiagnosticMsg::error(
                format!("failed to read module {}: {e}", path.display()),
                None,
            ),
        )
    })?;
    let file = SourceFile::new(FileId(*state.next_file_id), path.display().to_string(), src);
    *state.next_file_id += 1;
    let parsed = parse(&file)?;
    let dir = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // Only `pub fn`s declared in this file are part of the public export.
    // Helpers they call (local private fns, or fns brought in by `use`) are
    // copied under stable internal names so bodies still resolve, but those
    // names are not re-exported to the next importer.
    let mut direct: HashMap<String, FnDecl> = HashMap::new();
    let mut use_items = Vec::new();
    for item in &parsed.items {
        match item {
            Item::Fn(f) => {
                direct.insert(f.name.clone(), f.clone());
            }
            Item::Use(u) => use_items.push(u.clone()),
            _ => {}
        }
    }

    let mut imported_pub: HashMap<String, (PathBuf, FnDecl)> = HashMap::new();
    let mut imported_support: HashMap<String, FnDecl> = HashMap::new();
    for u in &use_items {
        let (dep_path, kind, commit, lock_spec) = resolve_use_path(&u.path, &dir, state.ctx)
            .map_err(|msg| {
                CompileError::single(Some(&file), DiagnosticMsg::error(msg, Some(u.span)))
            })?;
        let record_spec = lock_spec.as_deref().unwrap_or(u.path.as_str());
        record_resolved(record_spec, &dep_path, kind, commit, state.resolved_map)?;
        let dep_key = dep_path.canonicalize().unwrap_or_else(|_| dep_path.clone());
        let dep_exports = load_exports(&dep_path, state)?;
        for f in dep_exports {
            if f.is_pub {
                if direct.contains_key(&f.name) || imported_pub.contains_key(&f.name) {
                    return Err(CompileError::single(
                        Some(&file),
                        DiagnosticMsg::error(
                            format!(
                                "import conflict: function `{}` already defined (from {})",
                                f.name,
                                dep_path.display()
                            ),
                            Some(u.span),
                        ),
                    ));
                }
                imported_pub.insert(f.name.clone(), (dep_key.clone(), f));
            } else {
                imported_support.entry(f.name.clone()).or_insert(f);
            }
        }
    }

    let mut needed: Vec<String> = direct
        .values()
        .filter(|f| f.is_pub)
        .map(|f| f.name.clone())
        .collect();
    let mut seen_needed: HashSet<String> = needed.iter().cloned().collect();
    let mut idx = 0;
    while idx < needed.len() {
        let name = needed[idx].clone();
        idx += 1;
        let body = if let Some(f) = direct.get(&name) {
            f
        } else if let Some((_, f)) = imported_pub.get(&name) {
            f
        } else if let Some(f) = imported_support.get(&name) {
            f
        } else {
            continue;
        };
        for callee in call_callees(body) {
            let known = direct.contains_key(&callee)
                || imported_pub.contains_key(&callee)
                || imported_support.contains_key(&callee);
            if known && seen_needed.insert(callee.clone()) {
                needed.push(callee);
            }
        }
    }

    let mut direct_rename: HashMap<String, String> = HashMap::new();
    for (n, f) in &direct {
        if !f.is_pub {
            direct_rename.insert(n.clone(), internal_dep_name(&key, n));
        }
    }
    for (n, (origin, _)) in &imported_pub {
        direct_rename.insert(n.clone(), internal_dep_name(origin, n));
    }

    let mut exports = Vec::new();
    for name in &needed {
        if let Some(f) = direct.get(name) {
            let mut out = f.clone();
            rewrite_fn(&mut out, &direct_rename);
            if !out.is_pub {
                out.name = internal_dep_name(&key, name);
            }
            exports.push(out);
        } else if let Some((origin, f)) = imported_pub.get(name) {
            let mut out = f.clone();
            out.is_pub = false;
            out.name = internal_dep_name(origin, name);
            let mut rename = HashMap::new();
            for (n, (o, _)) in &imported_pub {
                if o == origin {
                    rename.insert(n.clone(), internal_dep_name(o, n));
                }
            }
            rewrite_fn(&mut out, &rename);
            exports.push(out);
        } else if let Some(f) = imported_support.get(name) {
            let mut out = f.clone();
            out.is_pub = false;
            exports.push(out);
        }
    }

    state.visiting.remove(&key);
    state.cache.insert(key, exports.clone());
    Ok(exports)
}

fn is_internal_dep(name: &str) -> bool {
    name.starts_with(INTERNAL_DEP_PREFIX)
}

fn internal_dep_name(origin: &Path, name: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    origin.hash(&mut hasher);
    name.hash(&mut hasher);
    format!("{INTERNAL_DEP_PREFIX}{:016x}_{name}", hasher.finish())
}

fn call_callees(f: &FnDecl) -> Vec<String> {
    let mut out = Vec::new();
    collect_block(&f.body, &mut out);
    out
}

fn collect_block(block: &Block, out: &mut Vec<String>) {
    for stmt in &block.stmts {
        match stmt {
            BlockStmt::Let(l) => collect_expr(&l.value, out),
            BlockStmt::Assign { value, .. } => collect_expr(value, out),
            BlockStmt::Expr(e) => collect_expr(e, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr(tail, out);
    }
}

fn collect_expr(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            out.push(callee.clone());
            for a in args {
                collect_expr(a, out);
            }
        }
        ExprKind::Method { recv, args, .. } => {
            collect_expr(recv, out);
            for a in args {
                collect_expr(a, out);
            }
        }
        ExprKind::List(xs) => {
            for x in xs {
                collect_expr(x, out);
            }
        }
        ExprKind::StageFrom { image } => collect_expr(image, out),
        ExprKind::MountCtor { args, .. } => {
            for a in args {
                collect_expr(a, out);
            }
        }
        ExprKind::Param { default, .. } => {
            if let Some(d) = default {
                collect_expr(d, out);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expr(left, out);
            collect_expr(right, out);
        }
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_expr(cond, out);
            collect_block(then_block, out);
            collect_block(else_block, out);
        }
        ExprKind::For { iter, body, .. } => {
            collect_expr(iter, out);
            collect_block(body, out);
        }
        ExprKind::Block(b) => collect_block(b, out),
        ExprKind::String(_)
        | ExprKind::StringInterp(_)
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::StageFromArg { .. } => {}
    }
}

fn rewrite_fn(f: &mut FnDecl, rename: &HashMap<String, String>) {
    rewrite_block(&mut f.body, rename);
}

fn rewrite_block(block: &mut Block, rename: &HashMap<String, String>) {
    for stmt in &mut block.stmts {
        match stmt {
            BlockStmt::Let(l) => rewrite_expr(&mut l.value, rename),
            BlockStmt::Assign { value, .. } => rewrite_expr(value, rename),
            BlockStmt::Expr(e) => rewrite_expr(e, rename),
        }
    }
    if let Some(tail) = &mut block.tail {
        rewrite_expr(tail, rename);
    }
}

fn rewrite_expr(expr: &mut Expr, rename: &HashMap<String, String>) {
    match &mut expr.kind {
        ExprKind::Call { callee, args } => {
            if let Some(n) = rename.get(callee) {
                *callee = n.clone();
            }
            for a in args {
                rewrite_expr(a, rename);
            }
        }
        ExprKind::Method { recv, args, .. } => {
            rewrite_expr(recv, rename);
            for a in args {
                rewrite_expr(a, rename);
            }
        }
        ExprKind::List(xs) => {
            for x in xs {
                rewrite_expr(x, rename);
            }
        }
        ExprKind::StageFrom { image } => rewrite_expr(image, rename),
        ExprKind::MountCtor { args, .. } => {
            for a in args {
                rewrite_expr(a, rename);
            }
        }
        ExprKind::Param { default, .. } => {
            if let Some(d) = default {
                rewrite_expr(d, rename);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            rewrite_expr(left, rename);
            rewrite_expr(right, rename);
        }
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            rewrite_expr(cond, rename);
            rewrite_block(then_block, rename);
            rewrite_block(else_block, rename);
        }
        ExprKind::For { iter, body, .. } => {
            rewrite_expr(iter, rename);
            rewrite_block(body, rename);
        }
        ExprKind::Block(b) => rewrite_block(b, rename),
        ExprKind::String(_)
        | ExprKind::StringInterp(_)
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::StageFromArg { .. } => {}
    }
}

/// Returns (filesystem path, kind, optional git commit, optional stable lock key).
fn resolve_use_path(
    spec: &str,
    from_dir: &Path,
    ctx: &ModuleLoadContext,
) -> std::result::Result<(PathBuf, ModuleKind, Option<String>, Option<String>), String> {
    // Full form: git+https://…?ref=&path=
    // Shorthand: github:owner/repo/path.lam[@ref]  (alias: gh:)
    if spec.starts_with("git+") || spec.starts_with("github:") || spec.starts_with("gh:") {
        let git = parse_remote_use(spec)?;
        let (path, commit) =
            git_remote::resolve_git_module(&git, ctx.offline, ctx.module_cache.as_deref())?;
        // Always lock under the canonical git+ form so github: and git+ share one key.
        let lock_key = to_git_plus_spec(&git);
        return Ok((path, ModuleKind::Git, commit, Some(lock_key)));
    }

    if let Some(rest) = spec.strip_prefix("std/") {
        let rel = PathBuf::from(rest);
        for root in &ctx.stdlib_paths {
            let candidates = [
                root.join(&rel),
                root.join(&rel).with_extension("lam"),
                root.join(format!("{}.lam", rel.display())),
            ];
            for c in candidates {
                if c.is_file() {
                    return Ok((c, ModuleKind::Std, None, None));
                }
            }
        }
        return Err(format!(
            "stdlib module not found: {spec} (set LAMINA_STDLIB or add repo stdlib/)"
        ));
    }

    if Path::new(spec).is_absolute() {
        return Err(
            "absolute use paths are not allowed (path-only modules under project root)".into(),
        );
    }

    let joined = from_dir.join(spec);
    let normalized = normalize_path(&joined);

    if let Ok(c) = normalized.canonicalize() {
        let root = ctx
            .project_root
            .canonicalize()
            .unwrap_or_else(|_| ctx.project_root.clone());
        let in_root = c.starts_with(&root);
        let in_stdlib = ctx.stdlib_paths.iter().any(|s| {
            s.canonicalize()
                .map(|sc| c.starts_with(sc))
                .unwrap_or(false)
        });
        let cache_root = git_remote::module_cache_root_override(ctx.module_cache.as_deref());
        let cache_root = cache_root.canonicalize().unwrap_or(cache_root);
        let in_cache = c.starts_with(&cache_root);

        // Relative imports from a git-fetched module live under the module cache,
        // not the consumer project root — that is intentional and allowed.
        if !in_root && !in_stdlib && !in_cache {
            return Err(format!(
                "use path escapes project root: {spec} (resolved {})",
                c.display()
            ));
        }
        if c.is_file() {
            // Nested `./dep.lam` inside a remote checkout → treat as git + stable lock key.
            if let Some((repo_dir, meta)) = git_remote::find_remote_checkout(&c) {
                // Stay inside the same clone (block `../../` out of the repo).
                if !c.starts_with(&repo_dir) {
                    return Err(format!("use path escapes remote module repository: {spec}"));
                }
                let lock_spec = git_remote::git_spec_for_path(&repo_dir, &meta, &c);
                let commit = std::process::Command::new("git")
                    .current_dir(&repo_dir)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    });
                return Ok((c, ModuleKind::Git, commit, lock_spec));
            }

            let kind = if in_stdlib && !in_root {
                ModuleKind::Std
            } else {
                ModuleKind::Path
            };
            return Ok((c, kind, None, None));
        }
    }

    if normalized.is_file() {
        return Ok((normalized, ModuleKind::Path, None, None));
    }
    Err(format!("module not found: {spec}"))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn stdlib_found_from_nested_example_project() {
        // Mimic examples/stdlib-go layout without needing cargo env.
        let dir = tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("stdlib")).unwrap();
        std::fs::write(
            repo.join("stdlib/golang.lam"),
            r#"
pub fn from_version(version: String) -> Stage {
  Stage.from("golang:" + version)
}
"#,
        )
        .unwrap();
        let proj = repo.join("examples/stdlib-go");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::write(
            proj.join("src/image.lam"),
            r#"
use "std/golang.lam";
pub target app = from_version("1.22-alpine").run("true").name("app");
"#,
        )
        .unwrap();
        let entry = proj.join("src/image.lam");
        let src = std::fs::read_to_string(&entry).unwrap();
        let file = SourceFile::new(FileId(0), entry.display().to_string(), src);
        let module = parse(&file).unwrap();
        let ctx = ModuleLoadContext::new(proj);
        let loaded = load_and_merge(&file, module, &ctx).unwrap();
        assert!(
            loaded
                .module
                .items
                .iter()
                .any(|i| matches!(i, Item::Fn(f) if f.name == "from_version")),
            "stdlib from_version should resolve from ancestor stdlib/"
        );
    }

    #[test]
    fn load_path_module_pub_fn() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let lib = root.join("lib.lam");
        std::fs::write(
            &lib,
            r#"
pub fn tag(s: Stage, n: String) -> Stage {
  s.name(n)
}
fn hidden(s: Stage) -> Stage { s }
"#,
        )
        .unwrap();
        let entry = root.join("image.lam");
        let mut f = std::fs::File::create(&entry).unwrap();
        writeln!(
            f,
            r#"
use "./lib.lam";
pub target app = tag(Stage.from("alpine:3.19"), "app");
"#
        )
        .unwrap();
        let src = std::fs::read_to_string(&entry).unwrap();
        let file = SourceFile::new(FileId(0), entry.display().to_string(), src);
        let module = parse(&file).unwrap();
        let ctx = ModuleLoadContext::new(root.to_path_buf());
        let loaded = load_and_merge(&file, module, &ctx).unwrap();
        let names: Vec<_> = loaded
            .module
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Fn(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"tag"));
        assert!(!names.contains(&"hidden"));
        assert_eq!(loaded.resolved.len(), 1);
        assert_eq!(loaded.resolved[0].spec, "./lib.lam");
        assert_eq!(loaded.resolved[0].kind, ModuleKind::Path);
    }

    #[test]
    fn use_does_not_reexport_transitive_pub_fns() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("base.lam"),
            r#"
pub fn cross_base(s: Stage) -> Stage {
  s.name("base")
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("gnu.lam"),
            r#"
use "./base.lam";
fn local_name(s: Stage) -> Stage { cross_base(s) }
pub fn gnu_image(s: Stage) -> Stage {
  local_name(s).name("gnu")
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("musl.lam"),
            r#"
use "./base.lam";
pub fn musl_image(s: Stage) -> Stage {
  cross_base(s).name("musl")
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("image.lam"),
            r#"
use "./gnu.lam";
use "./musl.lam";
pub target app = musl_image(gnu_image(Stage.from("alpine:3.19")));
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("Lamina.toml"),
            r#"
[package]
name = "families"
entry = "image.lam"
"#,
        )
        .unwrap();

        let entry = root.join("image.lam");
        let src = std::fs::read_to_string(&entry).unwrap();
        let file = SourceFile::new(FileId(0), entry.display().to_string(), src);
        let module = parse(&file).unwrap();
        let ctx = ModuleLoadContext::new(root.to_path_buf());
        let loaded = load_and_merge(&file, module, &ctx).unwrap();
        let names: Vec<_> = loaded
            .module
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Fn(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"gnu_image"));
        assert!(names.contains(&"musl_image"));
        assert!(
            !names.contains(&"cross_base"),
            "shared helper must not be re-exported: {names:?}"
        );
        assert!(!names.contains(&"local_name"));

        let opts = crate::compile::CompileOptions {
            params: HashMap::new(),
            build_args: HashMap::new(),
            targets: Vec::new(),
            stdlib_paths: Vec::new(),
            lint_deny: Vec::new(),
            run_lints: false,
            locked: false,
        };
        crate::compile::compile_project(root, &opts).unwrap();
    }

    #[test]
    fn load_git_nested_relative_use() {
        // Remote package with internal ./ dependency must resolve inside the clone.
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join("pkg")).unwrap();
        std::fs::write(
            repo.join("pkg/helpers.lam"),
            r#"
pub fn label_me(s: Stage) -> Stage {
  s.name("nested")
}
"#,
        )
        .unwrap();
        std::fs::write(
            repo.join("pkg/mod.lam"),
            r#"
use "./helpers.lam";
pub fn entry(s: Stage) -> Stage {
  label_me(s)
}
"#,
        )
        .unwrap();
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .status()
            .unwrap();

        let cache = dir.path().join("cache");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        let repo_url = format!("git+file://{}?ref=main&path=pkg/mod.lam", repo.display());
        std::fs::write(
            proj.join("src/image.lam"),
            format!(
                r#"
use "{repo_url}";
pub target app = entry(Stage.from("alpine:3.19"));
"#
            ),
        )
        .unwrap();

        let entry = proj.join("src/image.lam");
        let src = std::fs::read_to_string(&entry).unwrap();
        let file = SourceFile::new(FileId(0), entry.display().to_string(), src);
        let module = parse(&file).unwrap();
        let mut ctx = ModuleLoadContext::new(proj.clone());
        ctx.module_cache = Some(cache);
        let loaded = load_and_merge(&file, module, &ctx).unwrap();
        assert!(
            loaded
                .module
                .items
                .iter()
                .any(|i| matches!(i, Item::Fn(f) if f.name == "entry")),
            "entry from remote should import"
        );
        assert!(
            loaded
                .module
                .items
                .iter()
                .all(|i| !matches!(i, Item::Fn(f) if f.name == "label_me")),
            "nested ./helpers.lam must stay internal to the remote module"
        );
        // Nested dep locked under a git+ key, not bare "./helpers.lam"
        assert!(
            loaded
                .resolved
                .iter()
                .any(|r| r.spec.contains("path=pkg/helpers.lam") || r.spec.contains("helpers.lam")),
            "resolved specs: {:?}",
            loaded.resolved.iter().map(|r| &r.spec).collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_git_file_remote() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(
            repo.join("remote.lam"),
            r#"
pub fn remote_tag(s: Stage) -> Stage {
  s.name("from-git")
}
"#,
        )
        .unwrap();
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .status()
            .unwrap();

        let cache = dir.path().join("cache");
        let proj = dir.path().join("proj");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        let repo_url = format!("git+file://{}?ref=main&path=remote.lam", repo.display());
        std::fs::write(
            proj.join("src/image.lam"),
            format!(
                r#"
use "{repo_url}";
pub target app = remote_tag(Stage.from("alpine:3.19"));
"#
            ),
        )
        .unwrap();
        std::fs::write(
            proj.join("Lamina.toml"),
            r#"
[package]
name = "remote-demo"
entry = "src/image.lam"
"#,
        )
        .unwrap();

        let entry = proj.join("src/image.lam");
        let src = std::fs::read_to_string(&entry).unwrap();
        let file = SourceFile::new(FileId(0), entry.display().to_string(), src);
        let module = parse(&file).unwrap();
        let mut ctx = ModuleLoadContext::new(proj.clone());
        ctx.module_cache = Some(cache);
        let loaded = load_and_merge(&file, module, &ctx).unwrap();
        assert!(loaded.resolved.iter().any(|r| r.kind == ModuleKind::Git));
        assert!(loaded
            .module
            .items
            .iter()
            .any(|i| matches!(i, Item::Fn(f) if f.name == "remote_tag")));
    }
}
