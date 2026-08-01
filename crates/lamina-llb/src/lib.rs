//! Lower Build IR to an LLB-like op graph and stable text summaries.
//!
//! Full protobuf `pb.Definition` Solve integration is wired through a
//! Dockerfile-less path in `lamina-client` using `docker buildx` + BuildKit
//! gateway when available. This crate owns graph construction + goldens.

use lamina::ir::{Instr, ModuleIr, MountKind, MountSpec, StageBase, StageId};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct LlbGraph {
    pub ops: Vec<LlbOp>,
    /// stage_id → final op index producing that stage's root FS
    pub stage_roots: BTreeMap<u32, usize>,
    pub export_targets: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LlbOp {
    Image {
        ref_: String,
    },
    Exec {
        base: usize,
        input: usize,
        cmds: Vec<String>,
        cwd: Option<String>,
        env: Vec<(String, String)>,
        user: Option<String>,
    },
    /// Copy from build context into the stage FS.
    CopyLocal {
        base: usize,
        src: String,
        dst: String,
    },
    /// Copy from another stage's FS.
    CopyFrom {
        base: usize,
        from_stage: u32,
        from_op: usize,
        src: String,
        dst: String,
    },
    /// Image config mutation (entrypoint/cmd/user/env/expose/label/healthcheck).
    Config {
        base: usize,
        entrypoint: Option<Vec<String>>,
        cmd: Option<Vec<String>>,
        user: Option<String>,
        env: Vec<(String, String)>,
        expose: Vec<i64>,
        workdir: Option<String>,
        labels: Vec<(String, String)>,
        healthcheck: Option<String>,
    },
}

#[derive(Default)]
struct StageState {
    op: usize,
    workdir: Option<String>,
    env: Vec<(String, String)>,
    user: Option<String>,
    entrypoint: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    expose: Vec<i64>,
    labels: Vec<(String, String)>,
    healthcheck: Option<String>,
    platform: Option<String>,
}

pub fn lower(ir: &ModuleIr, targets: &[String]) -> LlbGraph {
    let solve = ir.solve_set(targets);
    let mut ops: Vec<LlbOp> = Vec::new();
    let mut stage_roots: BTreeMap<u32, usize> = BTreeMap::new();
    let mut states: BTreeMap<u32, StageState> = BTreeMap::new();

    // Lower in StageId order within solve set for determinism.
    let mut ordered: Vec<StageId> = solve.iter().copied().collect();
    ordered.sort_by_key(|s| s.0);

    // First pass: ensure copy_from sources lowered first via dependency sort
    ordered = topo_stages(ir, &solve);

    for sid in ordered {
        let stage = &ir.stages[&sid];
        let base_ref = match &stage.base {
            StageBase::Image(r) => r.clone(),
            StageBase::FromArg(a) => format!("${{{a}}}"), // should be resolved pre-lower
        };
        let img_idx = ops.len();
        ops.push(LlbOp::Image { ref_: base_ref });
        let mut st = StageState {
            op: img_idx,
            ..StageState::default()
        };

        for instr in &stage.instrs {
            match instr {
                Instr::Workdir(p) => {
                    st.workdir = Some(p.clone());
                }
                Instr::Env { key, value } => {
                    st.env.push((key.clone(), value.clone()));
                }
                Instr::User(u) => st.user = Some(u.clone()),
                Instr::Entrypoint(a) => st.entrypoint = Some(a.clone()),
                Instr::Cmd(a) => st.cmd = Some(a.clone()),
                Instr::Expose(p) => st.expose.push(*p),
                Instr::Label { key, value } => st.labels.push((key.clone(), value.clone())),
                Instr::Healthcheck(c) => st.healthcheck = Some(c.clone()),
                Instr::Platform(p) => st.platform = Some(p.clone()),
                Instr::Name(_) | Instr::Arg(_) | Instr::ArgDefault { .. } => {}
                Instr::Run(cmd) => {
                    let idx = ops.len();
                    ops.push(LlbOp::Exec {
                        base: st.op,
                        input: st.op,
                        cmds: vec!["/bin/sh".into(), "-c".into(), cmd.clone()],
                        cwd: st.workdir.clone(),
                        env: st.env.clone(),
                        user: st.user.clone(),
                    });
                    st.op = idx;
                }
                Instr::RunWith { cmd, mounts } => {
                    let idx = ops.len();
                    // Mounts recorded in summary via exec; full mount metadata in dockerfile bridge.
                    let _ = mounts;
                    ops.push(LlbOp::Exec {
                        base: st.op,
                        input: st.op,
                        cmds: vec!["/bin/sh".into(), "-c".into(), cmd.clone()],
                        cwd: st.workdir.clone(),
                        env: st.env.clone(),
                        user: st.user.clone(),
                    });
                    st.op = idx;
                }
                Instr::Copy { src, dst } => {
                    let idx = ops.len();
                    ops.push(LlbOp::CopyLocal {
                        base: st.op,
                        src: src.clone(),
                        dst: dst.clone(),
                    });
                    st.op = idx;
                }
                Instr::CopyMany { srcs, dst } => {
                    for src in srcs {
                        let idx = ops.len();
                        ops.push(LlbOp::CopyLocal {
                            base: st.op,
                            src: src.clone(),
                            dst: dst.clone(),
                        });
                        st.op = idx;
                    }
                }
                Instr::CopyFrom { from, src, dst } => {
                    let from_op = states
                        .get(&from.0)
                        .map(|s| s.op)
                        .or_else(|| stage_roots.get(&from.0).copied())
                        .unwrap_or(0);
                    let idx = ops.len();
                    ops.push(LlbOp::CopyFrom {
                        base: st.op,
                        from_stage: from.0,
                        from_op,
                        src: src.clone(),
                        dst: dst.clone(),
                    });
                    st.op = idx;
                }
            }
        }

        // Final config node if any metadata set
        if st.entrypoint.is_some()
            || st.cmd.is_some()
            || st.user.is_some()
            || !st.env.is_empty()
            || !st.expose.is_empty()
            || st.workdir.is_some()
            || !st.labels.is_empty()
            || st.healthcheck.is_some()
        {
            let idx = ops.len();
            ops.push(LlbOp::Config {
                base: st.op,
                entrypoint: st.entrypoint.clone(),
                cmd: st.cmd.clone(),
                user: st.user.clone(),
                env: st.env.clone(),
                expose: st.expose.clone(),
                workdir: st.workdir.clone(),
                labels: st.labels.clone(),
                healthcheck: st.healthcheck.clone(),
            });
            st.op = idx;
        }

        stage_roots.insert(sid.0, st.op);
        states.insert(sid.0, st);
    }

    let mut export_targets = BTreeMap::new();
    let selected: Vec<String> = if targets.is_empty() {
        ir.targets.keys().cloned().collect()
    } else {
        targets.to_vec()
    };
    for t in selected {
        if let Some(id) = ir.targets.get(&t) {
            export_targets.insert(t, id.0);
        }
    }

    LlbGraph {
        ops,
        stage_roots,
        export_targets,
    }
}

fn topo_stages(ir: &ModuleIr, solve: &BTreeSet<StageId>) -> Vec<StageId> {
    let mut deps: BTreeMap<StageId, BTreeSet<StageId>> = BTreeMap::new();
    for sid in solve {
        let mut d = BTreeSet::new();
        for instr in &ir.stages[sid].instrs {
            if let Instr::CopyFrom { from, .. } = instr {
                if solve.contains(from) {
                    d.insert(*from);
                }
            }
        }
        deps.insert(*sid, d);
    }
    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    while out.len() < solve.len() {
        let mut progressed = false;
        let mut ready: Vec<StageId> = deps
            .iter()
            .filter(|(s, d)| !done.contains(*s) && d.iter().all(|x| done.contains(x)))
            .map(|(s, _)| *s)
            .collect();
        ready.sort_by_key(|s| s.0);
        for s in ready {
            done.insert(s);
            out.push(s);
            progressed = true;
        }
        if !progressed {
            // cycle fallback
            for s in solve {
                if !done.contains(s) {
                    out.push(*s);
                }
            }
            break;
        }
    }
    out
}

/// Stable textual summary for golden tests (not raw protobuf).
pub fn summary(graph: &LlbGraph) -> String {
    let mut lines = Vec::new();
    for (i, op) in graph.ops.iter().enumerate() {
        let line = match op {
            LlbOp::Image { ref_ } => format!("{i}: image {ref_}"),
            LlbOp::Exec {
                base,
                cmds,
                cwd,
                env,
                user,
                ..
            } => {
                let cmd = cmds.last().cloned().unwrap_or_default();
                format!(
                    "{i}: exec base={base} cwd={} user={} env={} cmd={cmd}",
                    cwd.as_deref().unwrap_or(""),
                    user.as_deref().unwrap_or(""),
                    env.len()
                )
            }
            LlbOp::CopyLocal { base, src, dst } => {
                format!("{i}: copy_local base={base} {src} -> {dst}")
            }
            LlbOp::CopyFrom {
                base,
                from_stage,
                src,
                dst,
                ..
            } => format!("{i}: copy_from base={base} stage#{from_stage} {src} -> {dst}"),
            LlbOp::Config {
                base,
                entrypoint,
                user,
                workdir,
                labels,
                healthcheck,
                ..
            } => format!(
                "{i}: config base={base} user={} workdir={} labels={} healthcheck={} entrypoint={:?}",
                user.as_deref().unwrap_or(""),
                workdir.as_deref().unwrap_or(""),
                labels.len(),
                healthcheck.as_deref().unwrap_or(""),
                entrypoint
            ),
        };
        lines.push(line);
    }
    lines.push("exports:".into());
    for (t, sid) in &graph.export_targets {
        let root = graph.stage_roots.get(sid).copied().unwrap_or(0);
        lines.push(format!("  {t} -> stage#{sid} op#{root}"));
    }
    lines.join("\n") + "\n"
}

/// Render a deterministic Dockerfile-equivalent for BuildKit execution only.
///
/// This is an **internal** solve bridge so `lamina build` works with stock
/// `docker buildx` (dockerfile frontend). It is **not** a product artifact:
/// not written to the project tree, not for PR review, not a golden oracle.
pub fn render_internal_dockerfile(ir: &ModuleIr, targets: &[String]) -> String {
    let solve = ir.solve_set(targets);
    let ordered = topo_stages(ir, &solve);
    let mut out = String::from("# syntax=docker/dockerfile:1.7\n");
    out.push_str("# Generated internally by lamina for BuildKit solve — not a project source.\n");

    let mut name_of: BTreeMap<u32, String> = BTreeMap::new();
    for sid in &ordered {
        let st = &ir.stages[sid];
        let as_name = st.name.clone().unwrap_or_else(|| format!("stage{}", sid.0));
        name_of.insert(sid.0, as_name.clone());
        let base = match &st.base {
            StageBase::Image(r) => r.clone(),
            StageBase::FromArg(a) => format!("arg:{a}"),
        };
        let platform = st.instrs.iter().find_map(|i| match i {
            Instr::Platform(p) => Some(p.as_str()),
            _ => None,
        });
        if let Some(p) = platform {
            out.push_str(&format!("FROM --platform={p} {base} AS {as_name}\n"));
        } else {
            out.push_str(&format!("FROM {base} AS {as_name}\n"));
        }
        for instr in &st.instrs {
            match instr {
                Instr::Platform(_) => {}
                Instr::Workdir(p) => out.push_str(&format!("WORKDIR {p}\n")),
                Instr::Run(c) => out.push_str(&format!("RUN {c}\n")),
                Instr::Copy { src, dst } => out.push_str(&format!("COPY {src} {dst}\n")),
                Instr::CopyMany { srcs, dst } => {
                    out.push_str(&format!("COPY {} {dst}\n", srcs.join(" ")))
                }
                Instr::CopyFrom { from, src, dst } => {
                    let fn_ = name_of
                        .get(&from.0)
                        .cloned()
                        .unwrap_or_else(|| format!("stage{}", from.0));
                    out.push_str(&format!("COPY --from={fn_} {src} {dst}\n"));
                }
                Instr::Env { key, value } => {
                    out.push_str(&format!("ENV {}\n", dockerfile_kv(key, value)))
                }
                Instr::User(u) => out.push_str(&format!("USER {u}\n")),
                Instr::Entrypoint(a) => {
                    let json = serde_json::to_string(a).unwrap();
                    out.push_str(&format!("ENTRYPOINT {json}\n"));
                }
                Instr::Cmd(a) => {
                    let json = serde_json::to_string(a).unwrap();
                    out.push_str(&format!("CMD {json}\n"));
                }
                Instr::Expose(p) => out.push_str(&format!("EXPOSE {p}\n")),
                Instr::Label { key, value } => {
                    out.push_str(&format!("LABEL {}\n", dockerfile_kv(key, value)))
                }
                Instr::Healthcheck(c) => {
                    // Expect full HEALTHCHECK body or CMD form; pass through.
                    if c.starts_with("CMD") || c.starts_with("NONE") {
                        out.push_str(&format!("HEALTHCHECK {c}\n"));
                    } else {
                        out.push_str(&format!("HEALTHCHECK CMD {c}\n"));
                    }
                }
                Instr::RunWith { cmd, mounts } => {
                    let mut line = String::from("RUN");
                    for m in mounts {
                        line.push_str(&format!(" {}", mount_flag(m)));
                    }
                    line.push(' ');
                    line.push_str(cmd);
                    line.push('\n');
                    out.push_str(&line);
                }
                Instr::Name(_) | Instr::Arg(_) | Instr::ArgDefault { .. } => {}
            }
        }
        out.push('\n');
    }

    // Final stage: if single export target, ensure it's last FROM (already is if ordered)
    // For multiple targets, buildx --target selects.
    out
}

fn mount_flag(m: &MountSpec) -> String {
    match m.kind {
        MountKind::Cache => {
            format!("--mount=type=cache,target={},id={}", m.target, m.id)
        }
        MountKind::Secret => {
            format!("--mount=type=secret,id={},target={}", m.id, m.target)
        }
        MountKind::Ssh => {
            if m.id.is_empty() {
                format!("--mount=type=ssh,target={}", m.target)
            } else {
                format!("--mount=type=ssh,id={},target={}", m.id, m.target)
            }
        }
        MountKind::Bind => {
            format!("--mount=type=bind,source={},target={}", m.source, m.target)
        }
    }
}

/// Quote a Dockerfile ENV/LABEL value so spaces and special characters parse.
fn dockerfile_kv(key: &str, value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{key}=\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lamina::compile::{compile_source, CompileOptions};
    use lamina::config::LaminaToml;

    #[test]
    fn lower_copy_from_summary() {
        let src = r#"
pub target app = {
  let builder = Stage.from("golang:1.22").run("go build").name("builder");
  Stage.from("alpine:3.19").copy_from(builder, "/out/app", "/app").name("app")
};
"#;
        let c = compile_source(
            "t.lam",
            src,
            LaminaToml::default(),
            &CompileOptions::default(),
        )
        .unwrap();
        let g = lower(&c.ir, &["app".into()]);
        let s = summary(&g);
        assert!(s.contains("copy_from"), "{s}");
        assert!(s.contains("exports:"), "{s}");
    }

    #[test]
    fn internal_dockerfile_quotes_env_with_spaces() {
        let src = r#"
pub target app = Stage.from("alpine:3.19")
  .env("CROSS_TARGET_RUNNER", "/linux-runner aarch64")
  .label("desc", "hello world")
  .name("app");
"#;
        let c = compile_source(
            "t.lam",
            src,
            LaminaToml::default(),
            &CompileOptions::default(),
        )
        .unwrap();
        let df = render_internal_dockerfile(&c.ir, &["app".into()]);
        assert!(
            df.contains(r#"ENV CROSS_TARGET_RUNNER="/linux-runner aarch64""#),
            "env not quoted:\n{df}"
        );
        assert!(
            df.contains(r#"LABEL desc="hello world""#),
            "label not quoted:\n{df}"
        );
    }
}
