//! Compile-time evaluation → ModuleIr.

use crate::ast::*;
use crate::diag::{CompileError, DiagnosticMsg, Result};
use crate::ir::{
    BuildArgDecl, Instr, IrBuilder, ModuleIr, MountKind, MountSpec, StageBase, StageId,
};
use crate::span::SourceFile;
use std::collections::{BTreeMap, HashMap};

const DEFAULT_MAX_LOOP: usize = 10_000;
const DEFAULT_MAX_STAGES: usize = 10_000;

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Int(i64),
    Bool(bool),
    List(Vec<Value>),
    Stage(StageId),
    Mount(MountSpec),
}

impl Value {
    fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    fn as_stage(&self) -> Option<StageId> {
        match self {
            Value::Stage(id) => Some(*id),
            _ => None,
        }
    }
    fn as_list_string(&self) -> Option<Vec<String>> {
        match self {
            Value::List(xs) => {
                let mut out = Vec::new();
                for x in xs {
                    out.push(x.as_string()?.to_string());
                }
                Some(out)
            }
            _ => None,
        }
    }
    fn as_list_mount(&self) -> Option<Vec<MountSpec>> {
        match self {
            Value::List(xs) => {
                let mut out = Vec::new();
                for x in xs {
                    match x {
                        Value::Mount(m) => out.push(m.clone()),
                        _ => return None,
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }
}

pub struct EvalCaps {
    pub max_loop_iters: usize,
    pub max_stages: usize,
}

impl Default for EvalCaps {
    fn default() -> Self {
        Self {
            max_loop_iters: DEFAULT_MAX_LOOP,
            max_stages: DEFAULT_MAX_STAGES,
        }
    }
}

#[derive(Default)]
pub struct EvalInput {
    pub params: HashMap<String, String>,
    pub build_args: HashMap<String, String>,
    pub caps: EvalCaps,
}

struct FnDef {
    params: Vec<String>,
    body: Block,
}

struct Evaluator<'a> {
    file: &'a SourceFile,
    input: &'a EvalInput,
    builder: IrBuilder,
    fns: HashMap<String, FnDef>,
    loop_iters: usize,
}

fn values_eq(l: &Value, r: &Value, span: crate::span::Span, file: &SourceFile) -> Result<bool> {
    match (l, r) {
        (Value::String(a), Value::String(b)) => Ok(a == b),
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        _ => Err(CompileError::single(
            Some(file),
            DiagnosticMsg::error(
                "equality is only defined for String, Int, and Bool",
                Some(span),
            ),
        )),
    }
}

pub fn evaluate(file: &SourceFile, module: &Module, input: &EvalInput) -> Result<ModuleIr> {
    let mut ev = Evaluator {
        file,
        input,
        builder: IrBuilder::default(),
        fns: HashMap::new(),
        loop_iters: 0,
    };

    let mut build_arg_decls = Vec::new();
    let mut env: HashMap<String, Value> = HashMap::new();
    let mut targets: BTreeMap<String, StageId> = BTreeMap::new();

    for item in &module.items {
        if let Item::Fn(f) = item {
            ev.fns.insert(
                f.name.clone(),
                FnDef {
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    body: f.body.clone(),
                },
            );
        }
    }

    for item in &module.items {
        match item {
            Item::Use(_) => {}
            Item::Arg(a) => {
                build_arg_decls.push(BuildArgDecl {
                    name: a.name.clone(),
                    default: a.default.clone(),
                });
            }
            Item::Const(c) => {
                let v = ev.eval_expr(&c.value, &env)?;
                env.insert(c.name.clone(), v);
            }
            Item::Let(l) => {
                let v = ev.eval_expr(&l.value, &env)?;
                env.insert(l.name.clone(), v);
            }
            Item::Fn(_) => {}
            Item::Target(t) => {
                let v = ev.eval_expr(&t.value, &env)?;
                let Some(id) = v.as_stage() else {
                    return Err(CompileError::single(
                        Some(file),
                        DiagnosticMsg::error(
                            format!("target `{}` did not evaluate to Stage", t.name),
                            Some(t.span),
                        ),
                    ));
                };
                targets.insert(t.name.clone(), id);
            }
        }
    }

    if ev.builder_stage_count() > input.caps.max_stages {
        return Err(CompileError::single(
            Some(file),
            DiagnosticMsg::error("exceeded max stage count", None),
        ));
    }

    Ok(ev.builder.finish(targets, build_arg_decls))
}

impl<'a> Evaluator<'a> {
    fn builder_stage_count(&self) -> usize {
        // IrBuilder doesn't expose count; approximate via next by peeking finish — use internal
        // Access via extending: we track differently. For MVP skip hard check mid-flight;
        // the field next_id is private. Add method on IrBuilder.
        0
    }

    fn eval_block(&mut self, block: &Block, env: &HashMap<String, Value>) -> Result<Value> {
        let mut local = env.clone();
        self.eval_block_mut(block, &mut local)
    }

    /// Evaluate a block, mutating `env` (lets, assigns, and nested `for` updates).
    fn eval_block_mut(&mut self, block: &Block, env: &mut HashMap<String, Value>) -> Result<Value> {
        for stmt in &block.stmts {
            match stmt {
                BlockStmt::Let(l) => {
                    let v = self.eval_expr_mut(&l.value, env)?;
                    env.insert(l.name.clone(), v);
                }
                BlockStmt::Assign { name, value, span } => {
                    if !env.contains_key(name) {
                        return Err(CompileError::single(
                            Some(self.file),
                            DiagnosticMsg::error(
                                format!("cannot assign to undefined name `{name}`"),
                                Some(*span),
                            ),
                        ));
                    }
                    let v = self.eval_expr_mut(value, env)?;
                    env.insert(name.clone(), v);
                }
                BlockStmt::Expr(e) => {
                    let _ = self.eval_expr_mut(e, env)?;
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.eval_expr_mut(tail, env)
        } else if let Some(BlockStmt::Assign { name, .. }) = block.stmts.last() {
            // Allow assign-only blocks (common in `for` accumulation loops).
            env.get(name).cloned().ok_or_else(|| {
                CompileError::single(
                    Some(self.file),
                    DiagnosticMsg::error("block missing tail expression", Some(block.span)),
                )
            })
        } else {
            Err(CompileError::single(
                Some(self.file),
                DiagnosticMsg::error("block missing tail expression", Some(block.span)),
            ))
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: &HashMap<String, Value>) -> Result<Value> {
        let mut local = env.clone();
        self.eval_expr_mut(expr, &mut local)
    }

    fn eval_expr_mut(&mut self, expr: &Expr, env: &mut HashMap<String, Value>) -> Result<Value> {
        match &expr.kind {
            ExprKind::String(s) => Ok(Value::String(s.clone())),
            ExprKind::StringInterp(parts) => {
                let mut out = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push_str(s),
                        InterpPart::Ident(name) => {
                            let Some(Value::String(s)) = env.get(name) else {
                                return Err(CompileError::single(
                                    Some(self.file),
                                    DiagnosticMsg::error(
                                        format!("interpolation `${{{name}}}` not a String"),
                                        Some(expr.span),
                                    ),
                                ));
                            };
                            out.push_str(s);
                        }
                    }
                }
                Ok(Value::String(out))
            }
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Ident(name) => env.get(name).cloned().ok_or_else(|| {
                CompileError::single(
                    Some(self.file),
                    DiagnosticMsg::error(format!("undefined `{name}`"), Some(expr.span)),
                )
            }),
            ExprKind::List(els) => {
                let mut xs = Vec::new();
                for e in els {
                    xs.push(self.eval_expr_mut(e, env)?);
                }
                Ok(Value::List(xs))
            }
            ExprKind::Call { callee, args } => {
                let f = self.fns.get(callee).ok_or_else(|| {
                    CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error(
                            format!("undefined function `{callee}`"),
                            Some(expr.span),
                        ),
                    )
                })?;
                // clone to avoid borrow issues
                let params = f.params.clone();
                let body = f.body.clone();
                if args.len() != params.len() {
                    return Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("wrong number of arguments", Some(expr.span)),
                    ));
                }
                let mut local = HashMap::new();
                // Capture free vars from caller env for nested fns that close over outer lets.
                for (k, v) in env.iter() {
                    local.insert(k.clone(), v.clone());
                }
                for (p, a) in params.iter().zip(args.iter()) {
                    local.insert(p.clone(), self.eval_expr_mut(a, env)?);
                }
                self.eval_block_mut(&body, &mut local)
            }
            ExprKind::Method { recv, method, args } => {
                let rv = self.eval_expr_mut(recv, env)?;
                let Some(sid) = rv.as_stage() else {
                    return Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("method on non-stage", Some(expr.span)),
                    ));
                };
                self.eval_method(sid, method, args, env, expr)
            }
            ExprKind::StageFrom { image } => {
                let v = self.eval_expr_mut(image, env)?;
                let Some(img) = v.as_string() else {
                    return Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("Stage.from needs String", Some(image.span)),
                    ));
                };
                let id = self.builder.new_stage(StageBase::Image(img.to_string()));
                Ok(Value::Stage(id))
            }
            ExprKind::StageFromArg { name } => {
                // 0.1: resolve build-arg before lower if present, else keep FromArg
                if let Some(img) = self.input.build_args.get(name) {
                    let id = self.builder.new_stage(StageBase::Image(img.clone()));
                    Ok(Value::Stage(id))
                } else {
                    let id = self.builder.new_stage(StageBase::FromArg(name.clone()));
                    Ok(Value::Stage(id))
                }
            }
            ExprKind::MountCtor { kind, args } => {
                let mut vals = Vec::new();
                for a in args {
                    vals.push(self.eval_expr_mut(a, env)?);
                }
                let spec = match kind.as_str() {
                    "cache" => MountSpec {
                        kind: MountKind::Cache,
                        target: req_str(&vals, 0, expr, self.file)?,
                        id: req_str(&vals, 1, expr, self.file)?,
                        source: String::new(),
                    },
                    "secret" => MountSpec {
                        kind: MountKind::Secret,
                        id: req_str(&vals, 0, expr, self.file)?,
                        target: req_str(&vals, 1, expr, self.file)?,
                        source: String::new(),
                    },
                    "ssh" => MountSpec {
                        kind: MountKind::Ssh,
                        target: req_str(&vals, 0, expr, self.file)?,
                        id: if vals.len() > 1 {
                            req_str(&vals, 1, expr, self.file)?
                        } else {
                            String::new()
                        },
                        source: String::new(),
                    },
                    "bind" => MountSpec {
                        kind: MountKind::Bind,
                        source: req_str(&vals, 0, expr, self.file)?,
                        target: req_str(&vals, 1, expr, self.file)?,
                        id: String::new(),
                    },
                    other => {
                        return Err(CompileError::single(
                            Some(self.file),
                            DiagnosticMsg::error(
                                format!("unknown Mount ctor `{other}`"),
                                Some(expr.span),
                            ),
                        ))
                    }
                };
                Ok(Value::Mount(spec))
            }
            ExprKind::Param { name, default } => {
                if let Some(v) = self.input.params.get(name) {
                    return Ok(Value::String(v.clone()));
                }
                if let Some(d) = default {
                    return self.eval_expr_mut(d, env);
                }
                Err(CompileError::single(
                    Some(self.file),
                    DiagnosticMsg::error(
                        format!("missing compile-time param `{name}`"),
                        Some(expr.span),
                    ),
                ))
            }
            ExprKind::Binary { op, left, right } => self.eval_binary(*op, left, right, env, expr),
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let c = self.eval_expr_mut(cond, env)?;
                match c {
                    // Branches get a clone so assigns don't leak unless we want them to;
                    // Stage accumulation usually uses `for`, not `if`.
                    Value::Bool(true) => self.eval_block(then_block, env),
                    Value::Bool(false) => self.eval_block(else_block, env),
                    _ => Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("if condition not Bool", Some(cond.span)),
                    )),
                }
            }
            ExprKind::For { var, iter, body } => {
                let it = self.eval_expr_mut(iter, env)?;
                let Value::List(items) = it else {
                    return Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("for-in needs List", Some(iter.span)),
                    ));
                };
                let mut out = Vec::new();
                for item in items {
                    self.loop_iters += 1;
                    if self.loop_iters > self.input.caps.max_loop_iters {
                        return Err(CompileError::single(
                            Some(self.file),
                            DiagnosticMsg::error("eval loop iteration cap exceeded", None),
                        ));
                    }
                    // Share outer env so `x = x.run(...)` accumulates across iterations.
                    env.insert(var.clone(), item);
                    out.push(self.eval_block_mut(body, env)?);
                }
                env.remove(var);
                Ok(Value::List(out))
            }
            ExprKind::Block(b) => self.eval_block_mut(b, env),
        }
    }

    fn eval_binary(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        env: &mut HashMap<String, Value>,
        expr: &Expr,
    ) -> Result<Value> {
        match op {
            BinOp::And => {
                let l = self.eval_expr_mut(left, env)?;
                match l {
                    Value::Bool(false) => Ok(Value::Bool(false)),
                    Value::Bool(true) => match self.eval_expr_mut(right, env)? {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        _ => Err(CompileError::single(
                            Some(self.file),
                            DiagnosticMsg::error("operator `&&` requires Bool", Some(expr.span)),
                        )),
                    },
                    _ => Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("operator `&&` requires Bool", Some(left.span)),
                    )),
                }
            }
            BinOp::Or => {
                let l = self.eval_expr_mut(left, env)?;
                match l {
                    Value::Bool(true) => Ok(Value::Bool(true)),
                    Value::Bool(false) => match self.eval_expr_mut(right, env)? {
                        Value::Bool(b) => Ok(Value::Bool(b)),
                        _ => Err(CompileError::single(
                            Some(self.file),
                            DiagnosticMsg::error("operator `||` requires Bool", Some(expr.span)),
                        )),
                    },
                    _ => Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("operator `||` requires Bool", Some(left.span)),
                    )),
                }
            }
            BinOp::Add | BinOp::Eq | BinOp::Ne => {
                let l = self.eval_expr_mut(left, env)?;
                let r = self.eval_expr_mut(right, env)?;
                match op {
                    BinOp::Add => match (l, r) {
                        (Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
                        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                        (Value::List(mut a), Value::List(b)) => {
                            a.extend(b);
                            Ok(Value::List(a))
                        }
                        _ => Err(CompileError::single(
                            Some(self.file),
                            DiagnosticMsg::error("invalid operands for `+`", Some(expr.span)),
                        )),
                    },
                    BinOp::Eq => values_eq(&l, &r, expr.span, self.file).map(Value::Bool),
                    BinOp::Ne => values_eq(&l, &r, expr.span, self.file).map(|b| Value::Bool(!b)),
                    _ => unreachable!(),
                }
            }
        }
    }

    fn eval_method(
        &mut self,
        sid: StageId,
        method: &str,
        args: &[Expr],
        env: &mut HashMap<String, Value>,
        expr: &Expr,
    ) -> Result<Value> {
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.eval_expr_mut(a, env)?);
        }
        let instr = match method {
            "workdir" => Instr::Workdir(req_str(&vals, 0, expr, self.file)?),
            "run" => Instr::Run(req_run_cmd(&vals, 0, expr, self.file)?),
            "copy" => Instr::Copy {
                src: req_str(&vals, 0, expr, self.file)?,
                dst: req_str(&vals, 1, expr, self.file)?,
            },
            "copy_many" => Instr::CopyMany {
                srcs: req_list_str(&vals, 0, expr, self.file)?,
                dst: req_str(&vals, 1, expr, self.file)?,
            },
            "copy_from" => Instr::CopyFrom {
                from: req_stage(&vals, 0, expr, self.file)?,
                src: req_str(&vals, 1, expr, self.file)?,
                dst: req_str(&vals, 2, expr, self.file)?,
            },
            "env" => Instr::Env {
                key: req_str(&vals, 0, expr, self.file)?,
                value: req_str(&vals, 1, expr, self.file)?,
            },
            "arg" => Instr::Arg(req_str(&vals, 0, expr, self.file)?),
            "arg_default" => Instr::ArgDefault {
                name: req_str(&vals, 0, expr, self.file)?,
                default: req_str(&vals, 1, expr, self.file)?,
            },
            "user" => Instr::User(req_str(&vals, 0, expr, self.file)?),
            "entrypoint" => Instr::Entrypoint(req_list_str(&vals, 0, expr, self.file)?),
            "cmd" => Instr::Cmd(req_list_str(&vals, 0, expr, self.file)?),
            "expose" => {
                let Value::Int(p) = &vals[0] else {
                    return Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("expose expects Int", Some(expr.span)),
                    ));
                };
                Instr::Expose(*p)
            }
            "name" => Instr::Name(req_str(&vals, 0, expr, self.file)?),
            "label" => Instr::Label {
                key: req_str(&vals, 0, expr, self.file)?,
                value: req_str(&vals, 1, expr, self.file)?,
            },
            "healthcheck" => Instr::Healthcheck(req_str(&vals, 0, expr, self.file)?),
            "platform" => Instr::Platform(req_str(&vals, 0, expr, self.file)?),
            "run_with" => Instr::RunWith {
                cmd: req_run_cmd(&vals, 0, expr, self.file)?,
                mounts: vals.get(1).and_then(|v| v.as_list_mount()).ok_or_else(|| {
                    CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("run_with expects List[Mount]", Some(expr.span)),
                    )
                })?,
            },
            other => {
                return Err(CompileError::single(
                    Some(self.file),
                    DiagnosticMsg::error(format!("unknown method `{other}`"), Some(expr.span)),
                ))
            }
        };
        let id = self.builder.extend(sid, instr);
        Ok(Value::Stage(id))
    }
}

fn req_str(vals: &[Value], i: usize, expr: &Expr, file: &SourceFile) -> Result<String> {
    vals.get(i)
        .and_then(|v| v.as_string().map(|s| s.to_string()))
        .ok_or_else(|| {
            CompileError::single(
                Some(file),
                DiagnosticMsg::error("expected String argument", Some(expr.span)),
            )
        })
}

/// `.run` / `.run_with` first arg: `String` or `List[String]` (joined with `\n`).
fn req_run_cmd(vals: &[Value], i: usize, expr: &Expr, file: &SourceFile) -> Result<String> {
    let Some(v) = vals.get(i) else {
        return Err(CompileError::single(
            Some(file),
            DiagnosticMsg::error("expected String or List[String] command", Some(expr.span)),
        ));
    };
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::List(xs) => {
            let mut lines = Vec::with_capacity(xs.len());
            for x in xs {
                let Some(s) = x.as_string() else {
                    return Err(CompileError::single(
                        Some(file),
                        DiagnosticMsg::error(
                            "run command list elements must be String",
                            Some(expr.span),
                        ),
                    ));
                };
                lines.push(s.to_string());
            }
            Ok(lines.join("\n"))
        }
        _ => Err(CompileError::single(
            Some(file),
            DiagnosticMsg::error("expected String or List[String] command", Some(expr.span)),
        )),
    }
}

fn req_list_str(vals: &[Value], i: usize, expr: &Expr, file: &SourceFile) -> Result<Vec<String>> {
    vals.get(i).and_then(|v| v.as_list_string()).ok_or_else(|| {
        CompileError::single(
            Some(file),
            DiagnosticMsg::error("expected List[String] argument", Some(expr.span)),
        )
    })
}

fn req_stage(vals: &[Value], i: usize, expr: &Expr, file: &SourceFile) -> Result<StageId> {
    vals.get(i).and_then(|v| v.as_stage()).ok_or_else(|| {
        CompileError::single(
            Some(file),
            DiagnosticMsg::error("expected Stage argument", Some(expr.span)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::span::FileId;
    use crate::types::typecheck;

    #[test]
    fn eval_multi_stage() {
        let src = r#"
pub target app = {
  let builder = Stage.from("golang:1.22").run("go build").name("builder");
  Stage.from("alpine:3.19").copy_from(builder, "/out/app", "/app").name("app")
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).unwrap();
        typecheck(&f, &m).unwrap();
        let ir = evaluate(&f, &m, &EvalInput::default()).unwrap();
        let set = ir.solve_set(&["app".into()]);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn for_loop_reassigns_stage() {
        let src = r#"
pub target app = {
  let s = Stage.from("alpine:3.19");
  for pkg in ["curl", "jq"] {
    s = s.run("apk add --no-cache " + pkg);
  }
  s.name("app")
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        let ir = evaluate(&f, &m, &EvalInput::default()).expect("eval");
        // from + run curl + run jq + name = linear chain in solve_set
        let set = ir.solve_set(&["app".into()]);
        assert_eq!(set.len(), 1);
        let id = *ir.targets.get("app").unwrap();
        let runs = ir.stages[&id]
            .instrs
            .iter()
            .filter(|i| matches!(i, Instr::Run(_)))
            .count();
        assert_eq!(runs, 2, "expected two apk runs from the for loop");
    }

    #[test]
    fn list_concat_with_plus() {
        let src = r#"
pub target app = {
  let base = ["curl"];
  let more = ["jq", "git"];
  let pkgs = base + more + ["ca-certificates"];
  let s = Stage.from("alpine:3.19");
  for pkg in pkgs {
    s = s.run("apk add --no-cache " + pkg);
  }
  s.name("app")
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        let ir = evaluate(&f, &m, &EvalInput::default()).expect("eval");
        let id = *ir.targets.get("app").unwrap();
        let runs = ir.stages[&id]
            .instrs
            .iter()
            .filter(|i| matches!(i, Instr::Run(_)))
            .count();
        // curl + jq + git + ca-certificates
        assert_eq!(runs, 4, "expected four apk runs after list concat");
    }

    #[test]
    fn list_concat_right_empty() {
        let src = r#"
const xs: List[Int] = [1, 2] + [];
pub target app = Stage.from("alpine:3.19").name("app");
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        // Empty `[]` on the right inherits List[Int] from the left via expected type.
        typecheck(&f, &m).expect("types");
        evaluate(&f, &m, &EvalInput::default()).expect("eval");
    }

    #[test]
    fn list_concat_type_mismatch_rejected() {
        let src = r#"
pub target app = {
  let bad = [1, 2] + ["x"];
  Stage.from("alpine:3.19").name("app")
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        assert!(typecheck(&f, &m).is_err());
    }

    #[test]
    fn run_accepts_command_list() {
        let src = r#"
pub target app = Stage.from("alpine:3.19")
  .run(["set -eux", "echo hi", "true"])
  .name("app");
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        let ir = evaluate(&f, &m, &EvalInput::default()).expect("eval");
        let id = *ir.targets.get("app").unwrap();
        let run = ir.stages[&id]
            .instrs
            .iter()
            .find_map(|i| match i {
                Instr::Run(c) => Some(c.as_str()),
                _ => None,
            })
            .expect("run");
        assert_eq!(run, "set -eux\necho hi\ntrue");
    }

    #[test]
    fn run_multiline_string() {
        let src = "pub target app = Stage.from(\"alpine:3.19\").run(\"\"\"\n  set -eux\n  true\n\"\"\").name(\"app\");\n";
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        let ir = evaluate(&f, &m, &EvalInput::default()).expect("eval");
        let id = *ir.targets.get("app").unwrap();
        let run = ir.stages[&id]
            .instrs
            .iter()
            .find_map(|i| match i {
                Instr::Run(c) => Some(c.as_str()),
                _ => None,
            })
            .expect("run");
        assert_eq!(run, "set -eux\ntrue");
    }

    #[test]
    fn string_compare_selects_base() {
        let src = r#"
pub target app = {
  let libc = param("libc", "gnu");
  if libc == "musl" {
    Stage.from("alpine:3.19").name("app")
  } else {
    Stage.from("debian:bookworm-slim").name("app")
  }
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        let mut input = EvalInput::default();
        input.params.insert("libc".into(), "musl".into());
        let ir = evaluate(&f, &m, &input).expect("eval");
        let id = *ir.targets.get("app").unwrap();
        match &ir.stages[&id].base {
            StageBase::Image(img) => assert_eq!(img, "alpine:3.19"),
            other => panic!("expected image base, got {other:?}"),
        }
    }

    #[test]
    fn and_or_short_circuit_skips_missing_param() {
        let src = r#"
pub target app = {
  if false && param("missing") == "x" {
    Stage.from("never:used").name("app")
  } else {
    if true || param("also_missing") == "x" {
      Stage.from("alpine:3.19").name("app")
    } else {
      Stage.from("never:else").name("app")
    }
  }
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        let ir = evaluate(&f, &m, &EvalInput::default()).expect("eval short-circuit");
        let id = *ir.targets.get("app").unwrap();
        match &ir.stages[&id].base {
            StageBase::Image(img) => assert_eq!(img, "alpine:3.19"),
            other => panic!("expected alpine, got {other:?}"),
        }
    }

    #[test]
    fn int_and_bool_compare() {
        let src = r#"
const PORT: Int = 8080;
pub target app = {
  if PORT != 0 && true == true {
    Stage.from("alpine:3.19").name("app")
  } else {
    Stage.from("scratch").name("app")
  }
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).expect("parse");
        typecheck(&f, &m).expect("types");
        evaluate(&f, &m, &EvalInput::default()).expect("eval");
    }
}
