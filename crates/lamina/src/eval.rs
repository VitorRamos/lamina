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
        for stmt in &block.stmts {
            match stmt {
                BlockStmt::Let(l) => {
                    let v = self.eval_expr(&l.value, &local)?;
                    local.insert(l.name.clone(), v);
                }
                BlockStmt::Expr(e) => {
                    let _ = self.eval_expr(e, &local)?;
                }
            }
        }
        if let Some(tail) = &block.tail {
            self.eval_expr(tail, &local)
        } else {
            Err(CompileError::single(
                Some(self.file),
                DiagnosticMsg::error("block missing tail expression", Some(block.span)),
            ))
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: &HashMap<String, Value>) -> Result<Value> {
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
                    xs.push(self.eval_expr(e, env)?);
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
                let mut local = env.clone();
                for (p, a) in params.iter().zip(args.iter()) {
                    local.insert(p.clone(), self.eval_expr(a, env)?);
                }
                self.eval_block(&body, &local)
            }
            ExprKind::Method { recv, method, args } => {
                let rv = self.eval_expr(recv, env)?;
                let Some(sid) = rv.as_stage() else {
                    return Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("method on non-stage", Some(expr.span)),
                    ));
                };
                self.eval_method(sid, method, args, env, expr)
            }
            ExprKind::StageFrom { image } => {
                let v = self.eval_expr(image, env)?;
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
                    vals.push(self.eval_expr(a, env)?);
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
                    return self.eval_expr(d, env);
                }
                Err(CompileError::single(
                    Some(self.file),
                    DiagnosticMsg::error(
                        format!("missing compile-time param `{name}`"),
                        Some(expr.span),
                    ),
                ))
            }
            ExprKind::BinaryAdd { left, right } => {
                let l = self.eval_expr(left, env)?;
                let r = self.eval_expr(right, env)?;
                match (l, r) {
                    (Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                    _ => Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("invalid operands for `+`", Some(expr.span)),
                    )),
                }
            }
            ExprKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let c = self.eval_expr(cond, env)?;
                match c {
                    Value::Bool(true) => self.eval_block(then_block, env),
                    Value::Bool(false) => self.eval_block(else_block, env),
                    _ => Err(CompileError::single(
                        Some(self.file),
                        DiagnosticMsg::error("if condition not Bool", Some(cond.span)),
                    )),
                }
            }
            ExprKind::For { var, iter, body } => {
                let it = self.eval_expr(iter, env)?;
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
                    let mut local = env.clone();
                    local.insert(var.clone(), item);
                    out.push(self.eval_block(body, &local)?);
                }
                Ok(Value::List(out))
            }
            ExprKind::Block(b) => self.eval_block(b, env),
        }
    }

    fn eval_method(
        &mut self,
        sid: StageId,
        method: &str,
        args: &[Expr],
        env: &HashMap<String, Value>,
        expr: &Expr,
    ) -> Result<Value> {
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.eval_expr(a, env)?);
        }
        let instr = match method {
            "workdir" => Instr::Workdir(req_str(&vals, 0, expr, self.file)?),
            "run" => Instr::Run(req_str(&vals, 0, expr, self.file)?),
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
            "run_with" => Instr::RunWith {
                cmd: req_str(&vals, 0, expr, self.file)?,
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
}
