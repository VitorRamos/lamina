//! Git remote `use` resolution (`git+https` / `git+ssh` / `git+file` / `github:` / `gh:`).

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

/// Parse any supported remote `use` form into a [`GitUseSpec`].
///
/// Accepted:
/// - `git+https://…?ref=&path=` / `git+ssh://…` / `git+file://…`
/// - Shorthand: `github:owner/repo/path/to/file.lam[@ref]`
/// - Alias: `gh:owner/repo/path…[@ref]` (same as `github:`)
///
/// When `@ref` is omitted on the shorthand, **`main`** is used.
pub fn parse_remote_use(spec: &str) -> std::result::Result<GitUseSpec, String> {
    if spec.starts_with("github:") || spec.starts_with("gh:") {
        return parse_github_shorthand(spec);
    }
    if spec.starts_with("git+") {
        return parse_git_use(spec);
    }
    Err(format!(
        "unsupported remote use `{spec}` (use git+https://… or github:owner/repo/path.lam[@ref])"
    ))
}

/// `github:owner/repo/path/to/file.lam[@ref]` or `gh:…`
///
/// Expands to `git+https://github.com/owner/repo.git?ref=…&path=…`.
pub fn parse_github_shorthand(spec: &str) -> std::result::Result<GitUseSpec, String> {
    let rest = spec
        .strip_prefix("github:")
        .or_else(|| spec.strip_prefix("gh:"))
        .ok_or_else(|| "not a github: / gh: use spec".to_string())?;

    if rest.is_empty() {
        return Err("github: use requires owner/repo/path.lam".into());
    }

    // Optional @ref at the end (ref itself must not contain '@' for v1).
    let (body, git_ref) = match rest.rsplit_once('@') {
        Some((body, r)) if !r.is_empty() && !body.is_empty() => (body, r.to_string()),
        None => (rest, "main".to_string()),
        Some(_) => {
            return Err(
                "github: use looks like `@ref` is empty; use github:owner/repo/path.lam@ref".into(),
            );
        }
    };

    if body.contains('?') || body.contains('#') {
        return Err("github: shorthand does not take ?query; use @ref for the branch/tag".into());
    }

    let parts: Vec<&str> = body.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 {
        return Err(
            "github: use needs owner/repo/path.lam (e.g. github:org/repo/stdlib/rust.lam)".into(),
        );
    }
    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    let path = parts[2..].join("/");
    if path.contains("..") {
        return Err("github: path must not contain '..'".into());
    }
    if !path.ends_with(".lam") {
        return Err(format!("github: path must end with .lam (got `{path}`)"));
    }
    if owner.contains(':') || repo.contains(':') {
        return Err("invalid github: owner/repo".into());
    }

    Ok(GitUseSpec {
        url: format!("https://github.com/{owner}/{repo}.git"),
        git_ref,
        path,
        scheme: "https".into(),
    })
}

/// Canonical `git+https://…?ref=&path=` form (stable lock keys / nested rewrites).
pub fn to_git_plus_spec(g: &GitUseSpec) -> String {
    match g.scheme.as_str() {
        "https" => {
            let body = g.url.strip_prefix("https://").unwrap_or(&g.url);
            format!("git+https://{body}?ref={}&path={}", g.git_ref, g.path)
        }
        "ssh" => {
            let body = g.url.strip_prefix("ssh://").unwrap_or(&g.url);
            format!("git+ssh://{body}?ref={}&path={}", g.git_ref, g.path)
        }
        "file" => {
            let p = g.url.trim_start_matches('/');
            format!("git+file://{p}?ref={}&path={}", g.git_ref, g.path)
        }
        other => format!("git+{other}://{}?ref={}&path={}", g.url, g.git_ref, g.path),
    }
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
    module_cache_root_override(None)
}

pub fn module_cache_root_override(override_dir: Option<&Path>) -> PathBuf {
    if let Some(p) = override_dir {
        return p.to_path_buf();
    }
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
    cache_override: Option<&Path>,
) -> std::result::Result<(PathBuf, Option<String>), String> {
    let cache = module_cache_root_override(cache_override);
    let repo_id = short_id(&format!("{}@{}", git.url, git.git_ref));
    let repo_dir = cache.join("git").join(&repo_id);
    let blob_dir = cache.join("blob");
    std::fs::create_dir_all(&blob_dir).map_err(|e| e.to_string())?;

    // If already checked out and file exists, use it.
    let file_path = repo_dir.join(&git.path);
    if file_path.is_file() {
        let commit = git_rev_parse(&repo_dir).ok();
        let _ = write_remote_meta(&repo_dir, git);
        return Ok((file_path, commit));
    }

    if is_offline(offline) {
        return Err(format!(
            "offline: git module not in cache ({}); run `lamina lock` online first",
            git.url
        ));
    }

    fetch_repo(&git.url, &git.git_ref, &repo_dir)?;
    write_remote_meta(&repo_dir, git)?;
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

const REMOTE_META: &str = ".lamina-remote.json";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteMeta {
    pub url: String,
    pub git_ref: String,
    pub scheme: String,
}

fn write_remote_meta(repo_dir: &Path, git: &GitUseSpec) -> std::result::Result<(), String> {
    let meta = RemoteMeta {
        url: git.url.clone(),
        git_ref: git.git_ref.clone(),
        scheme: git.scheme.clone(),
    };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(repo_dir.join(REMOTE_META), text).map_err(|e| e.to_string())
}

/// Walk parents of `from` looking for a git checkout managed by Lamina.
pub fn find_remote_checkout(from: &Path) -> Option<(PathBuf, RemoteMeta)> {
    let mut cur = from.to_path_buf();
    loop {
        let meta_path = cur.join(REMOTE_META);
        if meta_path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<RemoteMeta>(&text) {
                    return Some((cur, meta));
                }
            }
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

/// Build a stable `git+…` use-spec for a file inside a Lamina git checkout.
pub fn git_spec_for_path(repo_dir: &Path, meta: &RemoteMeta, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(repo_dir).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    let url_body = match meta.scheme.as_str() {
        "https" => meta.url.strip_prefix("https://").unwrap_or(&meta.url),
        "ssh" => meta.url.strip_prefix("ssh://").unwrap_or(&meta.url),
        "file" => {
            // file URLs stored as bare path in meta.url
            return Some(format!(
                "git+file://{}?ref={}&path={}",
                meta.url.trim_start_matches('/'),
                meta.git_ref,
                rel
            ));
        }
        _ => &meta.url,
    };
    Some(format!(
        "git+{}://{}?ref={}&path={}",
        meta.scheme, url_body, meta.git_ref, rel
    ))
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

    #[test]
    fn parse_github_shorthand_default_main() {
        let g = parse_remote_use("github:VitorRamos/lamina/stdlib/rust.lam").unwrap();
        assert_eq!(g.url, "https://github.com/VitorRamos/lamina.git");
        assert_eq!(g.git_ref, "main");
        assert_eq!(g.path, "stdlib/rust.lam");
        assert_eq!(g.scheme, "https");
        assert_eq!(
            to_git_plus_spec(&g),
            "git+https://github.com/VitorRamos/lamina.git?ref=main&path=stdlib/rust.lam"
        );
    }

    #[test]
    fn parse_gh_shorthand_with_ref() {
        let g = parse_remote_use("gh:acme/imgs/rust/mod.lam@v1.2.0").unwrap();
        assert_eq!(g.url, "https://github.com/acme/imgs.git");
        assert_eq!(g.git_ref, "v1.2.0");
        assert_eq!(g.path, "rust/mod.lam");
    }

    #[test]
    fn github_shorthand_needs_path() {
        assert!(parse_remote_use("github:org/repo").is_err());
        assert!(parse_remote_use("github:org/repo@main").is_err());
    }
}
