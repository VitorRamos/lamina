//! Path-only module loading (`use "…";`).

use crate::ast::{FnDecl, Item, Module};
use crate::diag::{CompileError, DiagnosticMsg, Result};
use crate::lock::{hash_file, ResolvedModule};
use crate::parser::parse;
use crate::span::{FileId, SourceFile};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ModuleLoadContext {
    pub project_root: PathBuf,
    /// Directories searched for `std/…` imports (first hit wins).
    pub stdlib_paths: Vec<PathBuf>,
}

impl ModuleLoadContext {
    pub fn new(project_root: PathBuf) -> Self {
        let mut stdlib_paths = Vec::new();
        if let Ok(p) = std::env::var("LAMINA_STDLIB") {
            stdlib_paths.push(PathBuf::from(p));
        }
        stdlib_paths.push(project_root.join("stdlib"));
        if let Some(parent) = project_root.parent() {
            stdlib_paths.push(parent.join("stdlib"));
        }
        if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
            let crate_dir = PathBuf::from(m);
            if let Some(repo) = crate_dir.parent().and_then(|p| p.parent()) {
                stdlib_paths.push(repo.join("stdlib"));
            }
        }
        Self {
            project_root,
            stdlib_paths,
        }
    }
}

pub struct LoadResult {
    pub module: Module,
    /// Deduped resolved modules (for lockfile).
    pub resolved: Vec<ResolvedModule>,
}

/// Expand `use` items: load path modules and merge exported `pub fn` into the entry module.
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
                let path = resolve_use_path(&u.path, from_dir, state.ctx).map_err(|msg| {
                    CompileError::single(Some(file), DiagnosticMsg::error(msg, Some(u.span)))
                })?;
                record_resolved(&u.path, &path, state.resolved_map)?;
                let exports = load_exports(&path, state)?;
                for f in exports {
                    if !seen.insert(f.name.clone()) {
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
    let expanded = expand_uses(&file, parsed, &dir, state)?;
    let mut exports = Vec::new();
    for item in &expanded.items {
        if let Item::Fn(f) = item {
            if f.is_pub {
                exports.push(f.clone());
            }
        }
    }

    state.visiting.remove(&key);
    state.cache.insert(key, exports.clone());
    Ok(exports)
}

fn resolve_use_path(
    spec: &str,
    from_dir: &Path,
    ctx: &ModuleLoadContext,
) -> std::result::Result<PathBuf, String> {
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
                    return Ok(c);
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
        if !in_root && !in_stdlib {
            return Err(format!(
                "use path escapes project root: {spec} (resolved {})",
                c.display()
            ));
        }
        if c.is_file() {
            return Ok(c);
        }
    }

    if normalized.is_file() {
        return Ok(normalized);
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
    use tempfile::tempdir;

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
    }
}
