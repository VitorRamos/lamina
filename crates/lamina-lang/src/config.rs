//! Lamina.toml loading and project root discovery.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Config file name at a project root.
pub const CONFIG_FILE: &str = "Lamina.toml";

/// Nested project directory tried when the given path has no `Lamina.toml`.
///
/// Layout: put the Lamina project under `.lamina/` of an application repo so
/// `lamina check` / `build` from the repo root still find it.
pub const NESTED_PROJECT_DIR: &str = ".lamina";

/// Resolve a path argument to the project root directory.
///
/// Search order (first match wins):
/// 1. `path/Lamina.toml` → `path`
/// 2. `path/.lamina/Lamina.toml` → `path/.lamina`
/// 3. otherwise `path` unchanged (defaults / missing-config behavior)
pub fn resolve_project_root(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.join(CONFIG_FILE).is_file() {
        return path.to_path_buf();
    }
    let nested = path.join(NESTED_PROJECT_DIR);
    if nested.join(CONFIG_FILE).is_file() {
        return nested;
    }
    path.to_path_buf()
}

/// Whether `dir` is a Lamina project root (`Lamina.toml` present).
pub fn is_project_root(dir: impl AsRef<Path>) -> bool {
    dir.as_ref().join(CONFIG_FILE).is_file()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LaminaToml {
    #[serde(default)]
    pub package: Package,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub build: BuildSection,
    #[serde(default)]
    pub eval: EvalSection,
    #[serde(default)]
    pub lint: LintSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
}

impl Default for Package {
    fn default() -> Self {
        Self {
            name: default_name(),
            version: default_version(),
            entry: default_entry(),
        }
    }
}

fn default_name() -> String {
    "app".into()
}
fn default_version() -> String {
    "0.1.0".into()
}
fn default_entry() -> String {
    "src/image.lam".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildSection {
    #[serde(default = "default_context")]
    pub context: String,
    /// Default platforms for `lamina build` (e.g. `["linux/amd64"]`).
    #[serde(default)]
    pub platforms: Vec<String>,
}

impl Default for BuildSection {
    fn default() -> Self {
        Self {
            context: default_context(),
            platforms: Vec::new(),
        }
    }
}

fn default_context() -> String {
    ".".into()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EvalSection {
    pub max_loop_iters: Option<usize>,
    pub max_stages: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LintSection {
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid Lamina.toml: {0}")]
    Parse(#[from] toml::de::Error),
}

impl LaminaToml {
    /// Load from path; if missing, return defaults (caller may treat as optional).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(toml::from_str(&text)?)
    }

    pub fn load_or_default(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }

    pub fn entry_path(&self, root: &Path) -> PathBuf {
        root.join(&self.package.entry)
    }

    pub fn context_path(&self, root: &Path) -> PathBuf {
        root.join(&self.build.context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_minimal() {
        let t = r#"
[package]
name = "hello"
entry = "src/image.lam"

[params]
base = "alpine:3.19"
"#;
        let cfg: LaminaToml = toml::from_str(t).unwrap();
        assert_eq!(cfg.package.name, "hello");
        assert_eq!(cfg.params.get("base").unwrap(), "alpine:3.19");
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lamina-config-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_prefers_root_lamina_toml() {
        let root = temp_dir("root");
        fs::write(root.join(CONFIG_FILE), "[package]\nname = \"root\"\n").unwrap();
        let nested = root.join(NESTED_PROJECT_DIR);
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(CONFIG_FILE), "[package]\nname = \"nested\"\n").unwrap();
        assert_eq!(resolve_project_root(&root), root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_falls_back_to_dot_lamina() {
        let root = temp_dir("nested-only");
        let nested = root.join(NESTED_PROJECT_DIR);
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(CONFIG_FILE), "[package]\nname = \"nested\"\n").unwrap();
        assert_eq!(resolve_project_root(&root), nested);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_unchanged_when_missing() {
        let root = temp_dir("missing");
        assert_eq!(resolve_project_root(&root), root);
        let _ = fs::remove_dir_all(&root);
    }
}
