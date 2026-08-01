//! Backend-agnostic Build IR and solve_set algorithm.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StageId(pub u32);

#[derive(Debug, Clone)]
pub struct ModuleIr {
    pub stages: BTreeMap<StageId, StageIr>,
    pub targets: BTreeMap<String, StageId>,
    /// Global build-args declared via `arg "NAME"`.
    pub build_args: Vec<BuildArgDecl>,
}

#[derive(Debug, Clone)]
pub struct BuildArgDecl {
    pub name: String,
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StageIr {
    pub id: StageId,
    pub name: Option<String>,
    pub base: StageBase,
    pub instrs: Vec<Instr>,
}

#[derive(Debug, Clone)]
pub enum StageBase {
    Image(String),
    /// Resolved to Image at bind time in 0.1.
    FromArg(String),
}

#[derive(Debug, Clone)]
pub enum Instr {
    Workdir(String),
    Run(String),
    Copy {
        src: String,
        dst: String,
    },
    CopyMany {
        srcs: Vec<String>,
        dst: String,
    },
    CopyFrom {
        from: StageId,
        src: String,
        dst: String,
    },
    Env {
        key: String,
        value: String,
    },
    Arg(String),
    ArgDefault {
        name: String,
        default: String,
    },
    User(String),
    Entrypoint(Vec<String>),
    Cmd(Vec<String>),
    Expose(i64),
    Name(String),
    Label {
        key: String,
        value: String,
    },
    Healthcheck(String),
    /// RUN with BuildKit mounts (cache/secret/ssh/bind).
    RunWith {
        cmd: String,
        mounts: Vec<MountSpec>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountKind {
    Cache,
    Secret,
    Ssh,
    Bind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountSpec {
    pub kind: MountKind,
    /// cache id / secret id / ssh id (optional empty)
    pub id: String,
    pub target: String,
    /// bind source path (host/context); empty for non-bind
    pub source: String,
}

impl ModuleIr {
    /// Rule 6: solve_set = roots ∪ copy_from sources (not linear parents).
    pub fn solve_set(&self, root_targets: &[String]) -> BTreeSet<StageId> {
        let mut roots = BTreeSet::new();
        if root_targets.is_empty() {
            for id in self.targets.values() {
                roots.insert(*id);
            }
        } else {
            for t in root_targets {
                if let Some(id) = self.targets.get(t) {
                    roots.insert(*id);
                }
            }
        }
        let mut solve = roots.clone();
        let mut work: VecDeque<StageId> = roots.into_iter().collect();
        while let Some(id) = work.pop_front() {
            let Some(stage) = self.stages.get(&id) else {
                continue;
            };
            for instr in &stage.instrs {
                if let Instr::CopyFrom { from, .. } = instr {
                    if solve.insert(*from) {
                        work.push_back(*from);
                    }
                }
            }
        }
        solve
    }

    pub fn explain(&self, root_targets: &[String]) -> String {
        let set = self.solve_set(root_targets);
        let mut out = String::new();
        out.push_str("targets:\n");
        for (name, id) in &self.targets {
            out.push_str(&format!("  {name} -> stage#{}\n", id.0));
        }
        out.push_str("solve_set:\n");
        for id in &set {
            let st = &self.stages[id];
            let label = st.name.clone().unwrap_or_else(|| format!("stage#{}", id.0));
            out.push_str(&format!("  [{label}] stage#{}\n", id.0));
            match &st.base {
                StageBase::Image(img) => out.push_str(&format!("    from {img}\n")),
                StageBase::FromArg(a) => out.push_str(&format!("    from_arg {a}\n")),
            }
            for instr in &st.instrs {
                out.push_str(&format!("    {}\n", instr_summary(instr)));
            }
        }
        out
    }
}

fn instr_summary(i: &Instr) -> String {
    match i {
        Instr::Workdir(p) => format!("workdir {p}"),
        Instr::Run(c) => format!("run {c}"),
        Instr::Copy { src, dst } => format!("copy {src} -> {dst}"),
        Instr::CopyMany { srcs, dst } => format!("copy_many {srcs:?} -> {dst}"),
        Instr::CopyFrom { from, src, dst } => {
            format!("copy_from stage#{} {src} -> {dst}", from.0)
        }
        Instr::Env { key, value } => format!("env {key}={value}"),
        Instr::Arg(n) => format!("arg {n}"),
        Instr::ArgDefault { name, default } => format!("arg_default {name}={default}"),
        Instr::User(u) => format!("user {u}"),
        Instr::Entrypoint(a) => format!("entrypoint {a:?}"),
        Instr::Cmd(a) => format!("cmd {a:?}"),
        Instr::Expose(p) => format!("expose {p}"),
        Instr::Name(n) => format!("name {n}"),
        Instr::Label { key, value } => format!("label {key}={value}"),
        Instr::Healthcheck(c) => format!("healthcheck {c}"),
        Instr::RunWith { cmd, mounts } => format!("run_with mounts={} cmd={cmd}", mounts.len()),
    }
}

/// Mutable builder used during eval.
#[derive(Debug, Default)]
pub struct IrBuilder {
    next_id: u32,
    stages: HashMap<StageId, StageIr>,
}

impl IrBuilder {
    pub fn new_stage(&mut self, base: StageBase) -> StageId {
        let id = StageId(self.next_id);
        self.next_id += 1;
        self.stages.insert(
            id,
            StageIr {
                id,
                name: None,
                base,
                instrs: Vec::new(),
            },
        );
        id
    }

    /// Extend creates a new StageId forked from parent with additional instr.
    pub fn extend(&mut self, parent: StageId, instr: Instr) -> StageId {
        let parent_st = self.stages.get(&parent).expect("parent stage").clone();
        let id = StageId(self.next_id);
        self.next_id += 1;
        let mut instrs = parent_st.instrs;
        let mut name = parent_st.name;
        if let Instr::Name(ref n) = instr {
            name = Some(n.clone());
        }
        instrs.push(instr);
        self.stages.insert(
            id,
            StageIr {
                id,
                name,
                base: parent_st.base,
                instrs,
            },
        );
        id
    }

    pub fn finish(
        self,
        targets: BTreeMap<String, StageId>,
        build_args: Vec<BuildArgDecl>,
    ) -> ModuleIr {
        let mut stages = BTreeMap::new();
        for (id, st) in self.stages {
            stages.insert(id, st);
        }
        ModuleIr {
            stages,
            targets,
            build_args,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_set_linear_chain_single_root() {
        let mut b = IrBuilder::default();
        let a = b.new_stage(StageBase::Image("alpine".into()));
        let a2 = b.extend(a, Instr::Run("echo 1".into()));
        let a3 = b.extend(a2, Instr::Run("echo 2".into()));
        let mut targets = BTreeMap::new();
        targets.insert("t".into(), a3);
        let m = b.finish(targets, vec![]);
        let set = m.solve_set(&["t".into()]);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&a3));
    }

    #[test]
    fn solve_set_copy_from_includes_source() {
        let mut b = IrBuilder::default();
        let builder = b.new_stage(StageBase::Image("golang".into()));
        let builder = b.extend(builder, Instr::Run("go build".into()));
        let app = b.new_stage(StageBase::Image("distroless".into()));
        let app = b.extend(
            app,
            Instr::CopyFrom {
                from: builder,
                src: "/out".into(),
                dst: "/app".into(),
            },
        );
        let mut targets = BTreeMap::new();
        targets.insert("app".into(), app);
        let m = b.finish(targets, vec![]);
        let set = m.solve_set(&["app".into()]);
        assert!(set.contains(&app));
        assert!(set.contains(&builder));
        assert_eq!(set.len(), 2);
    }
}
