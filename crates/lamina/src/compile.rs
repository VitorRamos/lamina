//! High-level compile pipeline: source → typed → ModuleIr.

use crate::config::LaminaToml;
use crate::diag::Result;
use crate::eval::{evaluate, EvalCaps, EvalInput};
use crate::ir::ModuleIr;
use crate::parser::parse;
use crate::span::{FileId, SourceFile};
use crate::types::typecheck;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    pub params: HashMap<String, String>,
    pub build_args: HashMap<String, String>,
    pub targets: Vec<String>,
}

pub struct Compiled {
    pub file: SourceFile,
    pub ir: ModuleIr,
    pub config: LaminaToml,
    pub root: std::path::PathBuf,
}

pub fn compile_source(
    name: &str,
    src: &str,
    config: LaminaToml,
    opts: &CompileOptions,
) -> Result<Compiled> {
    let file = SourceFile::new(FileId(0), name, src);
    let module = parse(&file)?;
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
    Ok(Compiled {
        file,
        ir,
        config,
        root: Path::new(".").to_path_buf(),
    })
}

pub fn compile_project(root: &Path, opts: &CompileOptions) -> Result<Compiled> {
    let cfg_path = root.join("Lamina.toml");
    let config = crate::config::LaminaToml::load_or_default(&cfg_path).map_err(|e| {
        crate::diag::CompileError::single(
            None,
            crate::diag::DiagnosticMsg::error(e.to_string(), None),
        )
    })?;
    let entry = config.entry_path(root);
    let src = std::fs::read_to_string(&entry).map_err(|e| {
        crate::diag::CompileError::single(
            None,
            crate::diag::DiagnosticMsg::error(
                format!("failed to read {}: {e}", entry.display()),
                None,
            ),
        )
    })?;
    let mut compiled = compile_source(entry.to_string_lossy().as_ref(), &src, config, opts)?;
    compiled.root = root.to_path_buf();
    Ok(compiled)
}

pub fn explain(compiled: &Compiled, targets: &[String]) -> String {
    compiled.ir.explain(targets)
}
