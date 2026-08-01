//! IR-decidable lint pack (0.3).

use crate::diag::{CompileError, DiagnosticMsg, Severity};
use crate::ir::{Instr, ModuleIr, StageBase, StageId};
use std::collections::{BTreeSet, HashSet};

/// Known lint IDs (use with `[lint] deny = [...]` or `--deny`).
pub const LINT_IDS: &[&str] = &[
    "empty-stage",
    "unused-stage",
    "root-final",
    "secret-env",
    "unpinned-base",
];

#[derive(Debug, Clone)]
pub struct LintFinding {
    pub id: &'static str,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    /// Lint IDs treated as errors (fail check/build).
    pub deny: HashSet<String>,
    /// If true, all findings become errors.
    pub deny_all: bool,
}

impl LintOptions {
    pub fn from_lists(deny: &[String], deny_all: bool) -> Self {
        let set: HashSet<String> = deny.iter().cloned().collect();
        if set.iter().any(|s| s == "all" || s == "warnings") {
            // "warnings" means promote all warnings to errors
            return Self {
                deny: HashSet::new(),
                deny_all: true,
            };
        }
        Self {
            deny: set,
            deny_all,
        }
    }

    fn is_denied(&self, id: &str) -> bool {
        self.deny_all || self.deny.contains(id) || self.deny.contains("*")
    }
}

/// Run IR lints. Returns findings; converts to CompileError if any denied.
pub fn lint_ir(ir: &ModuleIr, opts: &LintOptions) -> Result<Vec<LintFinding>, CompileError> {
    let mut findings = Vec::new();
    findings.extend(lint_empty_stages(ir));
    findings.extend(lint_unused_stages(ir));
    findings.extend(lint_root_final(ir));
    findings.extend(lint_secret_env(ir));
    findings.extend(lint_unpinned_base(ir));

    let mut errors = Vec::new();
    for f in &findings {
        if opts.is_denied(f.id) || f.severity == Severity::Error {
            errors.push(DiagnosticMsg {
                severity: Severity::Error,
                message: format!("[{}] {}", f.id, f.message),
                span: None,
                help: Some(format!("deny list / --deny controls lint `{}`", f.id)),
            });
        }
    }

    if !errors.is_empty() {
        return Err(CompileError::from_diags(None, errors));
    }
    Ok(findings)
}

fn lint_empty_stages(ir: &ModuleIr) -> Vec<LintFinding> {
    let mut out = Vec::new();
    // Only named stages (explicit .name()) — intermediate StageIds are evaluator storage.
    for (id, st) in &ir.stages {
        if st.name.is_none() {
            continue;
        }
        let meaningful = st.instrs.iter().any(|i| {
            !matches!(
                i,
                Instr::Name(_) | Instr::Arg(_) | Instr::ArgDefault { .. } | Instr::Platform(_)
            )
        });
        if !meaningful {
            out.push(LintFinding {
                id: "empty-stage",
                severity: Severity::Warning,
                message: format!(
                    "stage#{} ({}) has no meaningful instructions",
                    id.0,
                    st.name.as_deref().unwrap_or("unnamed")
                ),
            });
        }
    }
    out
}

fn lint_unused_stages(ir: &ModuleIr) -> Vec<LintFinding> {
    let solve = ir.solve_set(&[]);
    let mut out = Vec::new();
    // Named stages not reachable from any target.
    for (id, st) in &ir.stages {
        if st.name.is_none() {
            continue;
        }
        if !solve.contains(id) {
            out.push(LintFinding {
                id: "unused-stage",
                severity: Severity::Warning,
                message: format!(
                    "named stage#{} ({}) is not reachable from any target (not in solve_set)",
                    id.0,
                    st.name.as_deref().unwrap_or("unnamed")
                ),
            });
        }
    }
    out
}

/// Root-final policy: export targets should not be intermediate builders that are only
/// used as copy_from sources for other targets (soft warning if a target is also a copy_from source of another *exported* stage).
fn lint_root_final(ir: &ModuleIr) -> Vec<LintFinding> {
    let mut copy_from_sources: BTreeSet<StageId> = BTreeSet::new();
    for st in ir.stages.values() {
        for instr in &st.instrs {
            if let Instr::CopyFrom { from, .. } = instr {
                copy_from_sources.insert(*from);
            }
        }
    }
    let mut out = Vec::new();
    for (name, id) in &ir.targets {
        let st = &ir.stages[id];
        // If the target stage is used as copy_from source by something in solve_set of *another* target,
        // warn — final images usually aren't also build deps of other finals.
        if copy_from_sources.contains(id) {
            // only warn if some *other* target depends on it
            let other_depends = ir
                .targets
                .iter()
                .any(|(tn, tid)| tn != name && stage_depends_on(ir, *tid, *id));
            if other_depends {
                out.push(LintFinding {
                    id: "root-final",
                    severity: Severity::Warning,
                    message: format!(
                        "target `{name}` (stage#{}) is also a copy_from source for another target — prefer dedicated builder stages",
                        id.0
                    ),
                });
            }
        }
        // Also: if named "builder" pattern and is a target, soft warn
        if st
            .name
            .as_ref()
            .map(|n| n.contains("builder") || n.contains("build"))
            .unwrap_or(false)
        {
            out.push(LintFinding {
                id: "root-final",
                severity: Severity::Warning,
                message: format!(
                    "target `{name}` looks like a builder stage (name {:?}) — export runtime image instead?",
                    st.name
                ),
            });
        }
    }
    out
}

fn stage_depends_on(ir: &ModuleIr, root: StageId, needle: StageId) -> bool {
    let mut seen = BTreeSet::new();
    let mut work = vec![root];
    while let Some(id) = work.pop() {
        if !seen.insert(id) {
            continue;
        }
        if id == needle && id != root {
            return true;
        }
        if let Some(st) = ir.stages.get(&id) {
            for instr in &st.instrs {
                if let Instr::CopyFrom { from, .. } = instr {
                    if *from == needle {
                        return true;
                    }
                    work.push(*from);
                }
            }
        }
    }
    false
}

fn lint_secret_env(ir: &ModuleIr) -> Vec<LintFinding> {
    const SUSPICIOUS: &[&str] = &[
        "PASSWORD",
        "SECRET",
        "TOKEN",
        "API_KEY",
        "APIKEY",
        "PRIVATE_KEY",
        "AWS_SECRET",
        "CREDENTIAL",
    ];
    let solve = ir.solve_set(&[]);
    let mut out = Vec::new();
    let mut seen_keys = HashSet::new();
    for id in &solve {
        let st = &ir.stages[id];
        for instr in &st.instrs {
            if let Instr::Env { key, .. } = instr {
                let upper = key.to_ascii_uppercase();
                if SUSPICIOUS.iter().any(|s| upper.contains(s)) && seen_keys.insert(key.clone()) {
                    out.push(LintFinding {
                        id: "secret-env",
                        severity: Severity::Warning,
                        message: format!(
                            "stage#{} env key `{key}` looks secret-like — use Mount.secret instead of ENV",
                            id.0
                        ),
                    });
                }
            }
        }
    }
    out
}

fn lint_unpinned_base(ir: &ModuleIr) -> Vec<LintFinding> {
    let solve = ir.solve_set(&[]);
    let mut out = Vec::new();
    let mut seen_images = HashSet::new();
    for id in &solve {
        let st = &ir.stages[id];
        if let StageBase::Image(ref img) = st.base {
            if is_unpinned(img) && seen_images.insert(img.clone()) {
                out.push(LintFinding {
                    id: "unpinned-base",
                    severity: Severity::Warning,
                    message: format!(
                        "stage#{} base `{img}` is unpinned (no digest @sha256:… and floating tag like latest)",
                        id.0
                    ),
                });
            }
        }
    }
    out
}

fn is_unpinned(image: &str) -> bool {
    if image.contains("@sha256:") {
        return false;
    }
    // no tag at all → defaults to latest
    let after_slash = image.rsplit('/').next().unwrap_or(image);
    if !after_slash.contains(':') {
        return true;
    }
    let tag = after_slash.rsplit(':').next().unwrap_or("");
    matches!(tag, "latest" | "stable" | "current" | "nightly" | "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{compile_source, CompileOptions};
    use crate::config::LaminaToml;

    #[test]
    fn detects_secret_env() {
        let src = r#"
pub target app = Stage.from("alpine:3.19")
  .env("DB_PASSWORD", "x")
  .name("app");
"#;
        let c = compile_source(
            "t.lam",
            src,
            LaminaToml::default(),
            &CompileOptions::default(),
        )
        .unwrap();
        let findings = lint_ir(&c.ir, &LintOptions::default()).unwrap();
        assert!(findings.iter().any(|f| f.id == "secret-env"));
    }

    #[test]
    fn deny_promotes_to_error() {
        let src = r#"
pub target app = Stage.from("alpine:latest").name("app");
"#;
        let c = compile_source(
            "t.lam",
            src,
            LaminaToml::default(),
            &CompileOptions::default(),
        )
        .unwrap();
        let opts = LintOptions::from_lists(&["unpinned-base".into()], false);
        let err = lint_ir(&c.ir, &opts).unwrap_err();
        assert!(
            err.message.contains("unpinned")
                || err
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("unpinned"))
        );
    }

    #[test]
    fn unpinned_latest() {
        assert!(is_unpinned("alpine:latest"));
        assert!(is_unpinned("alpine"));
        assert!(!is_unpinned("alpine:3.19"));
        assert!(!is_unpinned("alpine@sha256:abc"));
    }
}
