//! High-level compile pipeline: source → modules → typed → ModuleIr.

use crate::config::LaminaToml;
use crate::diag::Result;
use crate::eval::{evaluate, EvalCaps, EvalInput};
use crate::ir::ModuleIr;
use crate::lint::{lint_ir, LintFinding, LintOptions};
use crate::lock::{LaminaLock, ResolvedModule, LOCK_FILE_NAME};
use crate::modules::{load_and_merge, ModuleLoadContext};
use crate::parser::parse;
use crate::span::{FileId, SourceFile};
use crate::types::typecheck;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    pub params: HashMap<String, String>,
    pub build_args: HashMap<String, String>,
    pub targets: Vec<String>,
    pub stdlib_paths: Vec<PathBuf>,
    pub lint_deny: Vec<String>,
    pub run_lints: bool,
    /// Require Lamina.lock and verify hashes.
    pub locked: bool,
}

pub struct Compiled {
    pub file: SourceFile,
    pub ir: ModuleIr,
    pub config: LaminaToml,
    pub root: PathBuf,
    pub lint_findings: Vec<LintFinding>,
    pub resolved_modules: Vec<ResolvedModule>,
}

pub fn compile_source(
    name: &str,
    src: &str,
    config: LaminaToml,
    opts: &CompileOptions,
) -> Result<Compiled> {
    let root = PathBuf::from(".");
    compile_source_in(name, src, config, opts, &root)
}

pub fn compile_source_in(
    name: &str,
    src: &str,
    config: LaminaToml,
    opts: &CompileOptions,
    project_root: &Path,
) -> Result<Compiled> {
    let file = SourceFile::new(FileId(0), name, src);
    let module = parse(&file)?;
    let mut ctx = ModuleLoadContext::new(project_root.to_path_buf());
    for p in opts.stdlib_paths.iter().rev() {
        ctx.stdlib_paths.insert(0, p.clone());
    }
    // LAMINA_OFFLINE=1: never git fetch (path/stdlib still work; use warm module cache).
    ctx.offline = matches!(
        std::env::var("LAMINA_OFFLINE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    );
    let loaded = load_and_merge(&file, module, &ctx)?;
    let module = loaded.module;
    let resolved_modules = loaded.resolved;

    if opts.locked {
        let lock_path = project_root.join(LOCK_FILE_NAME);
        if !lock_path.is_file() {
            return Err(crate::diag::CompileError::single(
                None,
                crate::diag::DiagnosticMsg::error(
                    format!("--locked requires {LOCK_FILE_NAME} (run `lamina lock`)"),
                    None,
                ),
            ));
        }
        let lock = LaminaLock::load(&lock_path).map_err(|e| {
            crate::diag::CompileError::single(
                None,
                crate::diag::DiagnosticMsg::error(format!("read {LOCK_FILE_NAME}: {e}"), None),
            )
        })?;
        lock.verify(&resolved_modules, project_root)?;
    } else {
        // Soft check: warn via... we don't have warn channel; skip unless lock exists and mismatch?
        // Optional: if lock present, verify strictly only with --locked. Document that.
    }

    typecheck(&file, &module)?;

    let mut params = config.params.clone();
    params.extend(opts.params.clone());

    let caps = EvalCaps {
        max_loop_iters: config.eval.max_loop_iters.unwrap_or(10_000),
        max_stages: config.eval.max_stages.unwrap_or(10_000),
    };

    let input = EvalInput {
        params,
        build_args: opts.build_args.clone(),
        caps,
    };
    let ir = evaluate(&file, &module, &input)?;

    let mut lint_findings = Vec::new();
    if opts.run_lints {
        let mut deny = config.lint.deny.clone();
        deny.extend(opts.lint_deny.clone());
        let lint_opts = LintOptions::from_lists(&deny, false);
        lint_findings = lint_ir(&ir, &lint_opts)?;
    }

    Ok(Compiled {
        file,
        ir,
        config,
        root: project_root.to_path_buf(),
        lint_findings,
        resolved_modules,
    })
}

pub fn compile_project(root: &Path, opts: &CompileOptions) -> Result<Compiled> {
    let root = crate::config::resolve_project_root(root);
    let cfg_path = root.join(crate::config::CONFIG_FILE);
    let config = crate::config::LaminaToml::load_or_default(&cfg_path).map_err(|e| {
        crate::diag::CompileError::single(
            None,
            crate::diag::DiagnosticMsg::error(e.to_string(), None),
        )
    })?;
    let entry = config.entry_path(&root);
    let src = std::fs::read_to_string(&entry).map_err(|e| {
        crate::diag::CompileError::single(
            None,
            crate::diag::DiagnosticMsg::error(
                format!("failed to read {}: {e}", entry.display()),
                None,
            ),
        )
    })?;
    compile_source_in(entry.to_string_lossy().as_ref(), &src, config, opts, &root)
}

/// Write `Lamina.lock` for the project based on current `use` graph.
pub fn write_lockfile(root: &Path, opts: &CompileOptions) -> Result<PathBuf> {
    let mut opts = opts.clone();
    opts.locked = false;
    opts.run_lints = false;
    let compiled = compile_project(root, &opts)?;
    // Use the resolved project root (may be `path/.lamina`).
    let root = &compiled.root;
    let lock = LaminaLock::from_resolved(&compiled.resolved_modules, root);
    let path = root.join(LOCK_FILE_NAME);
    lock.save(&path).map_err(|e| {
        crate::diag::CompileError::single(
            None,
            crate::diag::DiagnosticMsg::error(format!("write {LOCK_FILE_NAME}: {e}"), None),
        )
    })?;
    Ok(path)
}

pub fn explain(compiled: &Compiled, targets: &[String]) -> String {
    compiled.ir.explain(targets)
}
