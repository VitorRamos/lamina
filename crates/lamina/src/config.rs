//! Lamina.toml loading.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

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
}

impl Default for BuildSection {
    fn default() -> Self {
        Self {
            context: default_context(),
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
}
