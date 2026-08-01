//! Solve images via Docker Buildx (BuildKit).
//!
//! 0.1 uses an **internal** ephemeral Dockerfile fed only to `docker buildx build`
//! so stock Docker/Buildx works without a custom gateway frontend. The Dockerfile
//! is never written into the project tree and is not a product artifact
//! (see design: hybrid solo-velocity bridge until raw LLB gRPC Solve).

use lamina::ir::ModuleIr;
use lamina_llb::{lower, render_internal_dockerfile, summary};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SolveError {
    #[error("docker/buildx failed: {0}")]
    Docker(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct SolveRequest {
    pub context: PathBuf,
    pub targets: Vec<String>,
    pub tags: Vec<String>,
    pub progress: String,
    pub builder: Option<String>,
}

pub struct SolveResult {
    pub llb_summary: String,
    pub internal_dockerfile: String,
}

pub fn plan(ir: &ModuleIr, targets: &[String]) -> SolveResult {
    let graph = lower(ir, targets);
    SolveResult {
        llb_summary: summary(&graph),
        internal_dockerfile: render_internal_dockerfile(ir, targets),
    }
}

/// Run BuildKit solve via `docker buildx build`.
pub fn solve(ir: &ModuleIr, req: &SolveRequest) -> Result<SolveResult, SolveError> {
    let plan = plan(ir, &req.targets);
    let tmp = tempfile::tempdir()?;
    let df_path = tmp.path().join("Dockerfile.lamina-internal");
    std::fs::write(&df_path, &plan.internal_dockerfile)?;

    let target = req
        .targets
        .first()
        .cloned()
        .or_else(|| {
            // default: last export name from IR
            ir.targets.keys().next().cloned()
        })
        .ok_or_else(|| SolveError::Other("no target to build".into()))?;

    // Stage name in Dockerfile may differ from target name — use stage name from IR
    let stage_name = ir
        .targets
        .get(&target)
        .and_then(|id| ir.stages.get(id))
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| target.clone());

    let mut cmd = Command::new("docker");
    cmd.arg("buildx").arg("build");
    if let Some(b) = &req.builder {
        cmd.arg("--builder").arg(b);
    }
    cmd.arg("-f").arg(&df_path);
    cmd.arg("--target").arg(&stage_name);
    for t in &req.tags {
        cmd.arg("-t").arg(t);
    }
    // load into local docker for docker driver
    cmd.arg("--load");
    cmd.arg("--progress").arg(&req.progress);
    cmd.arg(req.context.as_os_str());

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(SolveError::Docker(format!(
            "status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(plan)
}

pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn ensure_context(path: &Path) -> Result<(), SolveError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(SolveError::Other(format!(
            "build context is not a directory: {}",
            path.display()
        )))
    }
}
