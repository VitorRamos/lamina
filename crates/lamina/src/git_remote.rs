//! Git remote `use` resolution (`git+https` / `git+ssh` / `git+file`).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitUseSpec {
    /// e.g. https://github.com/acme/repo.git or git@host:path or file:///...
    pub url: String,
    /// Branch, tag, or commit
    pub git_ref: String,
    /// Path within repo to .lam file
    pub path: String,
    /// Original scheme without git+ (https, ssh, file)
    pub scheme: String,
}

/// Parse `git+https://…?ref=&path=` / `git+ssh://…` / `git+file://…`.
pub fn parse_git_use(spec: &str) -> std::result::Result<GitUseSpec, String> {
    let rest = spec
        .strip_prefix("git+")
        .ok_or_else(|| "not a git+ use spec".to_string())?;

    if rest.starts_with("http://") {
        return Err("insecure git+http:// is not allowed; use git+https:// or git+ssh://".into());
    }

    let (scheme, after_scheme) = if let Some(r) = rest.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = rest.strip_prefix("ssh://") {
        ("ssh", r)
    } else if let Some(r) = rest.strip_prefix("file://") {
        ("file", r)
    } else {
        return Err("unsupported git+ URL (use git+https://, git+ssh://, or git+file://)".into());
    };

    let (authority_and_path, query) = match after_scheme.split_once('?') {
        Some((a, q)) => (a, q),
        None => (after_scheme, ""),
    };

    let mut git_ref = None;
    let mut path = None;
    if !query.is_empty() {
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k {
                    "ref" => git_ref = Some(urlencoding_decode(v)),
                    "path" => path = Some(urlencoding_decode(v)),
                    _ => {}
                }
            }
        }
    }

    let path =
        path.ok_or_else(|| "git+ use requires query param path=… to a .lam file".to_string())?;
    if path.contains("..") {
        return Err("git+ path must not contain '..'".into());
    }

    let git_ref = match git_ref {
        Some(r) => r,
        None if scheme == "file" => "HEAD".into(),
        None => return Err("git+ use requires query param ref=… (branch, tag, or commit)".into()),
    };

    let url = match scheme {
        "https" => format!("https://{authority_and_path}"),
        "ssh" => format!("ssh://{authority_and_path}"),
        "file" => {
            // file:///abs/path or file://localhost/abs
            let p = if authority_and_path.starts_with('/') {
                authority_and_path.to_string()
            } else if let Some(rest) = authority_and_path.strip_prefix("localhost") {
                rest.to_string()
            } else {
                format!("/{authority_and_path}")
            };
            p
        }
        _ => unreachable!(),
    };

    Ok(GitUseSpec {
        url,
        git_ref,
        path,
        scheme: scheme.into(),
    })
}

fn urlencoding_decode(s: &str) -> String {
    // Minimal: only handle %2F and %40 etc. common cases; leave others as-is if invalid
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (from_hex(b[i + 1]), from_hex(b[i + 2])) {
                out.push((h << 4 | l) as char);
                i += 3;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub fn module_cache_root() -> PathBuf {
    if let Ok(p) = std::env::var("LAMINA_MODULE_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/lamina/modules");
    }
    PathBuf::from(".lamina-module-cache")
}

pub fn is_offline(ctx_offline: bool) -> bool {
    if ctx_offline {
        return true;
    }
    matches!(
        std::env::var("LAMINA_OFFLINE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Ensure `spec` is available on disk; return path to the `.lam` file and git commit (if any).
pub fn resolve_git_module(
    git: &GitUseSpec,
    offline: bool,
) -> std::result::Result<(PathBuf, Option<String>), String> {
    let cache = module_cache_root();
    let repo_id = short_id(&format!("{}@{}", git.url, git.git_ref));
    let repo_dir = cache.join("git").join(&repo_id);
    let blob_dir = cache.join("blob");
    std::fs::create_dir_all(&blob_dir).map_err(|e| e.to_string())?;

    // If already checked out and file exists, use it.
    let file_path = repo_dir.join(&git.path);
    if file_path.is_file() {
        let commit = git_rev_parse(&repo_dir).ok();
        return Ok((file_path, commit));
    }

    if is_offline(offline) {
        return Err(format!(
            "offline: git module not in cache ({}); run `lamina lock` online first",
            git.url
        ));
    }

    fetch_repo(&git.url, &git.git_ref, &repo_dir)?;
    let file_path = repo_dir.join(&git.path);
    if !file_path.is_file() {
        return Err(format!(
            "path `{}` not found in {}@{}",
            git.path, git.url, git.git_ref
        ));
    }

    // Content-addressed blob for offline re-use / lock verification helpers
    if let Ok(bytes) = std::fs::read(&file_path) {
        let mut h = Sha256::new();
        h.update(&bytes);
        let hex = hex::encode(h.finalize());
        let blob = blob_dir.join(&hex);
        if !blob.exists() {
            let _ = std::fs::write(&blob, bytes);
        }
    }

    let commit = git_rev_parse(&repo_dir).ok();
    Ok((file_path, commit))
}

/// When --locked and we already know sha256, prefer blob cache (no network).
pub fn resolve_from_blob(sha256: &str) -> Option<PathBuf> {
    let blob = module_cache_root().join("blob").join(sha256);
    if blob.is_file() {
        Some(blob)
    } else {
        None
    }
}

fn short_id(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())[..16].to_string()
}

fn fetch_repo(url: &str, git_ref: &str, dest: &Path) -> std::result::Result<(), String> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Prefer shallow clone by branch/tag name.
    let status = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            git_ref,
            url,
            dest.to_str().unwrap_or("."),
        ])
        .status()
        .map_err(|e| format!("failed to spawn git: {e}"))?;

    if status.success() {
        return Ok(());
    }

    // Fallback: clone default branch then checkout ref (works for commits).
    let status = Command::new("git")
        .args(["clone", url, dest.to_str().unwrap_or(".")])
        .status()
        .map_err(|e| format!("failed to spawn git: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git clone failed for {url} (ref {git_ref}); is git installed and the ref valid?"
        ));
    }
    let status = Command::new("git")
        .current_dir(dest)
        .args(["checkout", git_ref])
        .status()
        .map_err(|e| format!("git checkout failed: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git checkout {git_ref} failed in {}",
            dest.display()
        ));
    }
    Ok(())
}

fn git_rev_parse(repo: &Path) -> std::result::Result<String, String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("git rev-parse failed".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https() {
        let g = parse_git_use("git+https://github.com/acme/imgs.git?ref=v1.2.0&path=rust/mod.lam")
            .unwrap();
        assert_eq!(g.url, "https://github.com/acme/imgs.git");
        assert_eq!(g.git_ref, "v1.2.0");
        assert_eq!(g.path, "rust/mod.lam");
        assert_eq!(g.scheme, "https");
    }

    #[test]
    fn reject_http() {
        assert!(parse_git_use("git+http://example.com/r.git?ref=main&path=a.lam").is_err());
    }

    #[test]
    fn require_path() {
        assert!(parse_git_use("git+https://example.com/r.git?ref=main").is_err());
    }
}
