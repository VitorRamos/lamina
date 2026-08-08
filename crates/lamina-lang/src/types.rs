//! Typechecker for MVP language + Stage intrinsics.

use crate::ast::*;
use crate::diag::{CompileError, DiagnosticMsg, Result};
use crate::span::SourceFile;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<Type>,
    ret: Type,
}

pub fn typecheck(file: &SourceFile, module: &Module) -> Result<()> {
    let mut tenv: HashMap<String, Type> = HashMap::new();
    let mut fns: HashMap<String, FnSig> = HashMap::new();
    let mut diags = Vec::new();

    // First pass: collect fn signatures
    for item in &module.items {
        if let Item::Fn(f) = item {
            fns.insert(
                f.name.clone(),
                FnSig {
                    params: f.params.iter().map(|p| p.ty.clone()).collect(),
                    ret: f.ret.clone(),
                },
            );
        }
    }

    for item in &module.items {
        match item {
            Item::Use(_) => {}
            Item::Arg(_) => {}
            Item::Const(c) => match check_expr(&c.value, &tenv, &fns, Some(&c.ty), &mut diags) {
                Some(t) if t != c.ty => {
                    diags.push(DiagnosticMsg::error(
                        format!(
                            "const `{}` type mismatch: expected {}, got {}",
                            c.name,
                            c.ty.as_str(),
                            t.as_str()
                        ),
                        Some(c.span),
                    ));
                }
                Some(_) => {
                    tenv.insert(c.name.clone(), c.ty.clone());
                }
                None => {}
            },
            Item::Let(l) => {
                let expected = l.ty.as_ref();
                if let Some(t) = check_expr(&l.value, &tenv, &fns, expected, &mut diags) {
                    if let Some(et) = expected {
                        if t != *et {
                            diags.push(DiagnosticMsg::error(
                                format!(
                                    "let `{}` type mismatch: expected {}, got {}",
                                    l.name,
                                    et.as_str(),
                                    t.as_str()
                                ),
                                Some(l.span),
                            ));
                        }
                    }
                    tenv.insert(l.name.clone(), t);
                }
            }
            Item::Fn(f) => {
                let mut local = tenv.clone();
                for p in &f.params {
                    local.insert(p.name.clone(), p.ty.clone());
                }
                if let Some(t) = check_block(&f.body, &local, &fns, Some(&f.ret), &mut diags) {
                    if t != f.ret {
                        diags.push(DiagnosticMsg::error(
                            format!(
                                "function `{}` returns {}, expected {}",
                                f.name,
                                t.as_str(),
                                f.ret.as_str()
                            ),
                            Some(f.span),
                        ));
                    }
                }
            }
            Item::Target(t) => {
                if let Some(ty) = check_expr(&t.value, &tenv, &fns, Some(&Type::Stage), &mut diags)
                {
                    if ty != Type::Stage {
                        diags.push(DiagnosticMsg::error(
                            format!("target `{}` must be Stage, got {}", t.name, ty.as_str()),
                            Some(t.span),
                        ));
                    }
                }
            }
        }
    }

    if diags
        .iter()
        .any(|d| d.severity == crate::diag::Severity::Error)
    {
        Err(CompileError::from_diags(Some(file), diags))
    } else {
        Ok(())
    }
}

fn check_block(
    block: &Block,
    tenv: &HashMap<String, Type>,
    fns: &HashMap<String, FnSig>,
    expected: Option<&Type>,
    diags: &mut Vec<DiagnosticMsg>,
) -> Option<Type> {
    let mut local = tenv.clone();
    for stmt in &block.stmts {
        match stmt {
            BlockStmt::Let(l) => {
                let t = check_expr(&l.value, &local, fns, l.ty.as_ref(), diags)?;
                local.insert(l.name.clone(), t);
            }
            BlockStmt::Assign { name, value, span } => {
                let Some(prev) = local.get(name).cloned() else {
                    diags.push(DiagnosticMsg::error(
                        format!("cannot assign to undefined name `{name}` (use `let` first)"),
                        Some(*span),
                    ));
                    return None;
                };
                let t = check_expr(value, &local, fns, Some(&prev), diags)?;
                if t != prev {
                    diags.push(DiagnosticMsg::error(
                        format!(
                            "cannot assign {} to `{name}` (expected {})",
                            t.as_str(),
                            prev.as_str()
                        ),
                        Some(*span),
                    ));
                    return None;
                }
                local.insert(name.clone(), t);
            }
            BlockStmt::Expr(e) => {
                check_expr(e, &local, fns, None, diags)?;
            }
        }
    }
    if let Some(tail) = &block.tail {
        check_expr(tail, &local, fns, expected, diags)
    } else if let Some(BlockStmt::Assign { name, .. }) = block.stmts.last() {
        // Assign-only block: type is the assigned binding's type (for accumulation loops).
        local.get(name).cloned().or_else(|| {
            diags.push(DiagnosticMsg::error(
                "block requires a tail expression",
                Some(block.span),
            ));
            None
        })
    } else {
        diags.push(DiagnosticMsg::error(
            "block requires a tail expression",
            Some(block.span),
        ));
        None
    }
}

fn check_expr(
    expr: &Expr,
    tenv: &HashMap<String, Type>,
    fns: &HashMap<String, FnSig>,
    expected: Option<&Type>,
    diags: &mut Vec<DiagnosticMsg>,
) -> Option<Type> {
    let t = match &expr.kind {
        ExprKind::String(_) | ExprKind::StringInterp(_) => Type::String,
        ExprKind::Int(_) => Type::Int,
        ExprKind::Bool(_) => Type::Bool,
        ExprKind::Ident(name) => {
            if let Some(t) = tenv.get(name) {
                t.clone()
            } else {
                diags.push(DiagnosticMsg::error(
                    format!("undefined name `{name}`"),
                    Some(expr.span),
                ));
                return None;
            }
        }
        ExprKind::List(els) => {
            if els.is_empty() {
                if let Some(Type::List(inner)) = expected {
                    Type::List(inner.clone())
                } else {
                    // default List[String] for empty without context is ambiguous; use List[String]
                    Type::List(Box::new(Type::String))
                }
            } else {
                let first = check_expr(&els[0], tenv, fns, None, diags)?;
                for e in &els[1..] {
                    let t = check_expr(e, tenv, fns, Some(&first), diags)?;
                    if t != first {
                        diags.push(DiagnosticMsg::error(
                            "list elements must have uniform type",
                            Some(e.span),
                        ));
                        return None;
                    }
                }
                Type::List(Box::new(first))
            }
        }
        ExprKind::Call { callee, args } => {
            let Some(sig) = fns.get(callee) else {
                diags.push(DiagnosticMsg::error(
                    format!("undefined function `{callee}`"),
                    Some(expr.span),
                ));
                return None;
            };
            if args.len() != sig.params.len() {
                diags.push(DiagnosticMsg::error(
                    format!(
                        "function `{callee}` expects {} args, got {}",
                        sig.params.len(),
                        args.len()
                    ),
                    Some(expr.span),
                ));
                return None;
            }
            for (a, pt) in args.iter().zip(sig.params.iter()) {
                let at = check_expr(a, tenv, fns, Some(pt), diags)?;
                if at != *pt {
                    diags.push(DiagnosticMsg::error(
                        format!(
                            "argument type mismatch: expected {}, got {}",
                            pt.as_str(),
                            at.as_str()
                        ),
                        Some(a.span),
                    ));
                }
            }
            sig.ret.clone()
        }
        ExprKind::Method { recv, method, args } => {
            let rt = check_expr(recv, tenv, fns, None, diags)?;
            check_method(expr, &rt, method, args, tenv, fns, diags)?
        }
        ExprKind::StageFrom { image } => {
            let t = check_expr(image, tenv, fns, Some(&Type::String), diags)?;
            if t != Type::String {
                diags.push(DiagnosticMsg::error(
                    "Stage.from expects String",
                    Some(image.span),
                ));
            }
            Type::Stage
        }
        ExprKind::StageFromArg { .. } => Type::Stage,
        ExprKind::MountCtor { kind, args } => {
            check_mount_ctor(expr, kind, args, tenv, fns, diags)?;
            Type::Mount
        }
        ExprKind::Param { default, .. } => {
            if let Some(d) = default {
                let _ = check_expr(d, tenv, fns, Some(&Type::String), diags);
            }
            Type::String
        }
        ExprKind::Binary { op, left, right } => match op {
            BinOp::Add => {
                // Type the left first; use it as the expected type for the right so
                // empty list literals (`[]`) pick up the left element type when on the right.
                let lt = check_expr(left, tenv, fns, expected, diags)?;
                let rt = check_expr(right, tenv, fns, Some(&lt), diags)?;
                match (&lt, &rt) {
                    (Type::String, Type::String) => Type::String,
                    (Type::Int, Type::Int) => Type::Int,
                    (Type::List(a), Type::List(b)) if a == b => Type::List(a.clone()),
                    (Type::List(a), Type::List(b)) => {
                        diags.push(DiagnosticMsg::error(
                            format!(
                                "cannot concatenate List[{}] and List[{}]",
                                a.as_str(),
                                b.as_str()
                            ),
                            Some(expr.span),
                        ));
                        return None;
                    }
                    _ => {
                        diags.push(DiagnosticMsg::error(
                            format!(
                                "operator `+` not defined for {} and {}",
                                lt.as_str(),
                                rt.as_str()
                            ),
                            Some(expr.span),
                        ));
                        return None;
                    }
                }
            }
            BinOp::Eq | BinOp::Ne => {
                let lt = check_expr(left, tenv, fns, None, diags)?;
                let rt = check_expr(right, tenv, fns, Some(&lt), diags)?;
                let comparable = matches!(
                    (&lt, &rt),
                    (Type::String, Type::String)
                        | (Type::Int, Type::Int)
                        | (Type::Bool, Type::Bool)
                );
                if !comparable {
                    diags.push(DiagnosticMsg::error(
                        format!(
                            "operator `{}` not defined for {} and {} (only String, Int, Bool)",
                            op.as_str(),
                            lt.as_str(),
                            rt.as_str()
                        ),
                        Some(expr.span),
                    ));
                    return None;
                }
                Type::Bool
            }
            BinOp::And | BinOp::Or => {
                let lt = check_expr(left, tenv, fns, Some(&Type::Bool), diags)?;
                let rt = check_expr(right, tenv, fns, Some(&Type::Bool), diags)?;
                if lt != Type::Bool || rt != Type::Bool {
                    diags.push(DiagnosticMsg::error(
                        format!(
                            "operator `{}` requires Bool operands, got {} and {}",
                            op.as_str(),
                            lt.as_str(),
                            rt.as_str()
                        ),
                        Some(expr.span),
                    ));
                    return None;
                }
                Type::Bool
            }
        },
        ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            let ct = check_expr(cond, tenv, fns, Some(&Type::Bool), diags)?;
            if ct != Type::Bool {
                diags.push(DiagnosticMsg::error(
                    "if condition must be Bool",
                    Some(cond.span),
                ));
            }
            let tt = check_block(then_block, tenv, fns, expected, diags)?;
            let et = check_block(else_block, tenv, fns, Some(&tt), diags)?;
            if tt != et {
                diags.push(DiagnosticMsg::error(
                    "if branches must have the same type",
                    Some(expr.span),
                ));
            }
            tt
        }
        ExprKind::For { var, iter, body } => {
            let it = check_expr(iter, tenv, fns, None, diags)?;
            let Type::List(inner) = it else {
                diags.push(DiagnosticMsg::error(
                    "for-in requires List",
                    Some(iter.span),
                ));
                return None;
            };
            let mut local = tenv.clone();
            local.insert(var.clone(), *inner);
            let body_t = check_block(body, &local, fns, None, diags)?;
            Type::List(Box::new(body_t))
        }
        ExprKind::Block(b) => check_block(b, tenv, fns, expected, diags)?,
    };

    if let Some(exp) = expected {
        if t != *exp {
            // allow check sites to report; some callers re-check
        }
    }
    Some(t)
}

fn check_mount_ctor(
    expr: &Expr,
    kind: &str,
    args: &[Expr],
    tenv: &HashMap<String, Type>,
    fns: &HashMap<String, FnSig>,
    diags: &mut Vec<DiagnosticMsg>,
) -> Option<()> {
    match kind {
        "cache" => {
            // Mount.cache(target, id)
            if args.len() != 2 {
                diags.push(DiagnosticMsg::error(
                    "Mount.cache(target, id) expects 2 args",
                    Some(expr.span),
                ));
                return None;
            }
            expect_ty(&args[0], Type::String, tenv, fns, diags);
            expect_ty(&args[1], Type::String, tenv, fns, diags);
        }
        "secret" => {
            // Mount.secret(id, target)
            if args.len() != 2 {
                diags.push(DiagnosticMsg::error(
                    "Mount.secret(id, target) expects 2 args",
                    Some(expr.span),
                ));
                return None;
            }
            expect_ty(&args[0], Type::String, tenv, fns, diags);
            expect_ty(&args[1], Type::String, tenv, fns, diags);
        }
        "ssh" => {
            // Mount.ssh(target) or Mount.ssh(target, id)
            if args.is_empty() || args.len() > 2 {
                diags.push(DiagnosticMsg::error(
                    "Mount.ssh(target[, id]) expects 1 or 2 args",
                    Some(expr.span),
                ));
                return None;
            }
            for a in args {
                expect_ty(a, Type::String, tenv, fns, diags);
            }
        }
        "bind" => {
            // Mount.bind(source, target)
            if args.len() != 2 {
                diags.push(DiagnosticMsg::error(
                    "Mount.bind(source, target) expects 2 args",
                    Some(expr.span),
                ));
                return None;
            }
            expect_ty(&args[0], Type::String, tenv, fns, diags);
            expect_ty(&args[1], Type::String, tenv, fns, diags);
        }
        other => {
            diags.push(DiagnosticMsg::error(
                format!("unknown Mount constructor `{other}`"),
                Some(expr.span),
            ));
            return None;
        }
    }
    Some(())
}

fn check_method(
    expr: &Expr,
    recv: &Type,
    method: &str,
    args: &[Expr],
    tenv: &HashMap<String, Type>,
    fns: &HashMap<String, FnSig>,
    diags: &mut Vec<DiagnosticMsg>,
) -> Option<Type> {
    if recv != &Type::Stage {
        diags.push(DiagnosticMsg::error(
            format!("method `{method}` called on non-Stage"),
            Some(expr.span),
        ));
        return None;
    }
    let ok = match method {
        "workdir" | "run" | "user" | "name" | "arg" | "healthcheck" | "platform" => args.len() == 1,
        "copy" | "label" => args.len() == 2,
        "copy_many" => args.len() == 2,
        "copy_from" => args.len() == 3,
        "env" | "arg_default" => args.len() == 2,
        "entrypoint" | "cmd" => args.len() == 1,
        "expose" => args.len() == 1,
        "run_with" => args.len() == 2,
        _ => {
            diags.push(DiagnosticMsg::error(
                format!("unknown Stage method `{method}`"),
                Some(expr.span),
            ));
            return None;
        }
    };
    if !ok {
        diags.push(DiagnosticMsg::error(
            format!("wrong arity for Stage.{method}"),
            Some(expr.span),
        ));
        return None;
    }
    match method {
        "workdir" | "user" | "name" | "arg" | "healthcheck" | "platform" => {
            expect_ty(&args[0], Type::String, tenv, fns, diags);
        }
        "run" => {
            expect_run_cmd(&args[0], tenv, fns, diags);
        }
        "copy" | "label" => {
            expect_ty(&args[0], Type::String, tenv, fns, diags);
            expect_ty(&args[1], Type::String, tenv, fns, diags);
        }
        "copy_many" => {
            expect_ty(
                &args[0],
                Type::List(Box::new(Type::String)),
                tenv,
                fns,
                diags,
            );
            expect_ty(&args[1], Type::String, tenv, fns, diags);
        }
        "copy_from" => {
            expect_ty(&args[0], Type::Stage, tenv, fns, diags);
            expect_ty(&args[1], Type::String, tenv, fns, diags);
            expect_ty(&args[2], Type::String, tenv, fns, diags);
        }
        "env" | "arg_default" => {
            expect_ty(&args[0], Type::String, tenv, fns, diags);
            expect_ty(&args[1], Type::String, tenv, fns, diags);
        }
        "entrypoint" | "cmd" => {
            expect_ty(
                &args[0],
                Type::List(Box::new(Type::String)),
                tenv,
                fns,
                diags,
            );
        }
        "expose" => {
            expect_ty(&args[0], Type::Int, tenv, fns, diags);
        }
        "run_with" => {
            expect_run_cmd(&args[0], tenv, fns, diags);
            expect_ty(
                &args[1],
                Type::List(Box::new(Type::Mount)),
                tenv,
                fns,
                diags,
            );
        }
        _ => {}
    }
    Some(Type::Stage)
}

/// `.run` / `.run_with` command: `String` or `List[String]` (joined with newlines).
fn expect_run_cmd(
    expr: &Expr,
    tenv: &HashMap<String, Type>,
    fns: &HashMap<String, FnSig>,
    diags: &mut Vec<DiagnosticMsg>,
) {
    // Prefer List[String] when the expr is a list literal so `[]` / mixed check works.
    let prefer_list = matches!(expr.kind, ExprKind::List(_));
    let expected = if prefer_list {
        Some(Type::List(Box::new(Type::String)))
    } else {
        None
    };
    if let Some(t) = check_expr(expr, tenv, fns, expected.as_ref(), diags) {
        let ok = match &t {
            Type::String => true,
            Type::List(inner) if **inner == Type::String => true,
            _ => false,
        };
        if !ok {
            diags.push(DiagnosticMsg::error(
                format!(
                    "run command expects String or List[String], got {}",
                    t.as_str()
                ),
                Some(expr.span),
            ));
        }
    }
}

fn expect_ty(
    expr: &Expr,
    want: Type,
    tenv: &HashMap<String, Type>,
    fns: &HashMap<String, FnSig>,
    diags: &mut Vec<DiagnosticMsg>,
) {
    if let Some(t) = check_expr(expr, tenv, fns, Some(&want), diags) {
        if t != want {
            diags.push(DiagnosticMsg::error(
                format!("expected {}, got {}", want.as_str(), t.as_str()),
                Some(expr.span),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::span::FileId;

    #[test]
    fn typecheck_simple_target() {
        let src = r#"pub target app = Stage.from("alpine:3.19").run("true").name("app");"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).unwrap();
        typecheck(&f, &m).unwrap();
    }

    #[test]
    fn typecheck_string_compare_if() {
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
        let m = parse(&f).unwrap();
        typecheck(&f, &m).unwrap();
    }

    #[test]
    fn typecheck_rejects_stage_equality() {
        let src = r#"
pub target app = {
  let a = Stage.from("alpine:3.19");
  let b = Stage.from("alpine:3.19");
  if a == b {
    a.name("app")
  } else {
    b.name("app")
  }
};
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).unwrap();
        assert!(typecheck(&f, &m).is_err());
    }

    #[test]
    fn typecheck_rejects_and_on_strings() {
        let src = r#"
const bad: Bool = "a" && "b";
pub target app = Stage.from("alpine:3.19").name("app");
"#;
        let f = SourceFile::new(FileId(0), "t.lam", src);
        let m = parse(&f).unwrap();
        assert!(typecheck(&f, &m).is_err());
    }
}
