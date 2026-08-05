//! Solve images via Docker Buildx (BuildKit).
//!
//! Uses an **internal** ephemeral Dockerfile fed only to `docker buildx build`
//! so stock Docker/Buildx works without a custom gateway frontend.
//!
//! Builds label images and write a project-local layer cache under
//! `.lamina/build-cache` so `lamina clear` can remove this project's images
//! and cache without a full builder wipe.

use lamina_lang::config::NESTED_PROJECT_DIR;
use lamina_lang::ir::ModuleIr;
use lamina_llb::{lower, render_internal_dockerfile, summary};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

/// OCI label: Lamina package name (`Lamina.toml` `[package].name`).
pub const LABEL_PROJECT: &str = "com.lamina.project";
/// OCI label: absolute project root (directory containing `Lamina.toml`).
pub const LABEL_PROJECT_ROOT: &str = "com.lamina.project-root";

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
    /// e.g. `["linux/amd64"]` or multi `["linux/amd64","linux/arm64"]`
    pub platforms: Vec<String>,
    /// Multi-platform local load is unsupported; push to registry instead.
    pub push: bool,
    /// Absolute (or project-resolved) root that owns `Lamina.toml`.
    pub project_root: PathBuf,
    /// `[package].name` — used for default tags and image labels.
    pub package_name: String,
}

pub struct SolveResult {
    pub llb_summary: String,
    pub internal_dockerfile: String,
}

/// Where `lamina build` stores project-local BuildKit cache (`cache-to` local).
///
/// - Project at `app/` → `app/.lamina/build-cache`
/// - Project at `app/.lamina/` → `app/.lamina/build-cache`
pub fn project_build_cache_dir(project_root: &Path) -> PathBuf {
    if project_root.file_name().and_then(|n| n.to_str()) == Some(NESTED_PROJECT_DIR) {
        project_root.join("build-cache")
    } else {
        project_root.join(NESTED_PROJECT_DIR).join("build-cache")
    }
}

/// Canonical string used in `com.lamina.project-root` labels.
pub fn project_root_label(project_root: &Path) -> String {
    project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Default local image tag for a package (`name:dev`).
pub fn default_image_tag(package_name: &str) -> String {
    format!("{package_name}:dev")
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
        .or_else(|| ir.targets.keys().next().cloned())
        .ok_or_else(|| SolveError::Other("no target to build".into()))?;

    let stage_name = ir
        .targets
        .get(&target)
        .and_then(|id| ir.stages.get(id))
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| target.clone());

    let multi = req.platforms.len() > 1;
    if multi && !req.push {
        return Err(SolveError::Other(
            "multi-platform build requires --push (docker cannot --load multi-arch images locally)"
                .into(),
        ));
    }

    let root_label = project_root_label(&req.project_root);
    let cache_dir = project_build_cache_dir(&req.project_root);
    if let Some(parent) = cache_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }

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
    // Labels so `lamina clear` can find images for this project.
    cmd.arg("--label")
        .arg(format!("{LABEL_PROJECT}={}", req.package_name));
    cmd.arg("--label")
        .arg(format!("{LABEL_PROJECT_ROOT}={root_label}"));

    // Project-local layer cache (cleared by `lamina clear`).
    if cache_dir.is_dir() {
        cmd.arg("--cache-from")
            .arg(format!("type=local,src={}", cache_dir.display()));
    }
    cmd.arg("--cache-to")
        .arg(format!("type=local,dest={},mode=max", cache_dir.display()));

    if !req.platforms.is_empty() {
        cmd.arg("--platform").arg(req.platforms.join(","));
    }
    if req.push {
        cmd.arg("--push");
    } else {
        // Single-platform (or no platform flag): load into local docker
        cmd.arg("--load");
    }
    cmd.arg("--progress").arg(&req.progress);
    cmd.arg(req.context.as_os_str());

    // Stream Buildx/BuildKit progress live (do not buffer with `.output()`).
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = cmd.status()?;
    if !status.success() {
        return Err(SolveError::Docker(format!(
            "docker buildx build exited with status {:?}",
            status.code()
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

/// Options for [`clear`].
#[derive(Debug, Clone)]
pub struct ClearRequest {
    pub project_root: PathBuf,
    pub package_name: String,
    /// Also remove these explicit image refs (in addition to labeled / default tag).
    pub extra_tags: Vec<String>,
    pub dry_run: bool,
}

/// Summary of what [`clear`] removed (or would remove).
#[derive(Debug, Clone, Default)]
pub struct ClearResult {
    /// Image IDs or `repo:tag` refs removed.
    pub removed_images: Vec<String>,
    /// Path of project build cache that was removed (if any).
    pub removed_cache: Option<PathBuf>,
    pub dry_run: bool,
}

/// Remove local Docker images and the project BuildKit cache for a Lamina project.
///
/// Images are matched by:
/// 1. Label `com.lamina.project-root` = absolute project root (set on `lamina build`)
/// 2. Default tag `{package}:dev`
/// 3. Any `--tag` values passed to clear
///
/// Build cache is the project-local directory written by `lamina build`
/// (see [`project_build_cache_dir`]).
pub fn clear(req: &ClearRequest) -> Result<ClearResult, SolveError> {
    let mut result = ClearResult {
        dry_run: req.dry_run,
        ..ClearResult::default()
    };

    let root_label = project_root_label(&req.project_root);
    let mut refs: Vec<String> = Vec::new();

    // Labeled images from prior lamina builds.
    if docker_available() {
        let labeled = docker_images_with_label(LABEL_PROJECT_ROOT, &root_label)?;
        refs.extend(labeled);

        // Default tag + extras (may already be labeled; de-dupe later).
        refs.push(default_image_tag(&req.package_name));
        refs.extend(req.extra_tags.iter().cloned());

        // De-dupe while preserving order.
        let mut seen = std::collections::HashSet::new();
        refs.retain(|r| seen.insert(r.clone()));

        // Only keep refs that exist locally (for nicer dry-run / output).
        let existing: Vec<String> = refs
            .into_iter()
            .filter(|r| docker_image_exists(r))
            .collect();

        if req.dry_run {
            result.removed_images = existing;
        } else if !existing.is_empty() {
            docker_rmi(&existing)?;
            result.removed_images = existing;
        }
    }

    // Project-local build cache.
    let cache_dir = project_build_cache_dir(&req.project_root);
    if cache_dir.exists() {
        if req.dry_run {
            result.removed_cache = Some(cache_dir);
        } else {
            std::fs::remove_dir_all(&cache_dir)?;
            result.removed_cache = Some(cache_dir);
        }
    }

    Ok(result)
}

fn docker_images_with_label(key: &str, value: &str) -> Result<Vec<String>, SolveError> {
    let filter = format!("label={key}={value}");
    let out = Command::new("docker")
        .args([
            "image",
            "ls",
            "--filter",
            &filter,
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ])
        .output()
        .map_err(|e| SolveError::Docker(format!("docker image ls: {e}")))?;
    if !out.status.success() {
        return Err(SolveError::Docker(format!(
            "docker image ls failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "<none>:<none>")
        .map(str::to_string)
        .collect())
}

fn docker_image_exists(image_ref: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image_ref])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn docker_rmi(refs: &[String]) -> Result<(), SolveError> {
    let mut cmd = Command::new("docker");
    cmd.arg("image").arg("rm").arg("-f");
    for r in refs {
        cmd.arg(r);
    }
    let out = cmd
        .output()
        .map_err(|e| SolveError::Docker(format!("docker image rm: {e}")))?;
    if !out.status.success() {
        // Partial failure is OK if some tags already gone; only hard-fail when nothing worked.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("No such image") && out.stdout.is_empty() {
            return Err(SolveError::Docker(format!(
                "docker image rm failed: {stderr}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn build_cache_dir_for_root_project() {
        let root = Path::new("/tmp/myapp");
        assert_eq!(
            project_build_cache_dir(root),
            PathBuf::from("/tmp/myapp/.lamina/build-cache")
        );
    }

    #[test]
    fn build_cache_dir_for_nested_project() {
        let root = Path::new("/tmp/myapp/.lamina");
        assert_eq!(
            project_build_cache_dir(root),
            PathBuf::from("/tmp/myapp/.lamina/build-cache")
        );
    }

    #[test]
    fn default_tag() {
        assert_eq!(default_image_tag("hello-static"), "hello-static:dev");
    }

    #[test]
    fn clear_removes_local_cache_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        let cache = project_build_cache_dir(&root);
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("marker"), b"x").unwrap();
        assert!(cache.is_dir());

        let res = clear(&ClearRequest {
            project_root: root,
            package_name: "proj".into(),
            extra_tags: vec![],
            dry_run: false,
        })
        .unwrap();
        assert_eq!(res.removed_cache.as_ref(), Some(&cache));
        assert!(!cache.exists());
    }

    #[test]
    fn clear_dry_run_keeps_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        let cache = project_build_cache_dir(&root);
        fs::create_dir_all(&cache).unwrap();

        let res = clear(&ClearRequest {
            project_root: root,
            package_name: "proj".into(),
            extra_tags: vec![],
            dry_run: true,
        })
        .unwrap();
        assert!(res.dry_run);
        assert_eq!(res.removed_cache.as_ref(), Some(&cache));
        assert!(cache.is_dir());
    }
}
