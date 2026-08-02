//! Analyze a buffer with the Lamina compiler for LSP features.

use crate::position::{span_to_range, word_at_offset};
use lamina_lang::ast::{Item, Module, Type};
use lamina_lang::config::LaminaToml;
use lamina_lang::diag::{CompileError, Severity};
use lamina_lang::modules::{load_and_merge, ModuleLoadContext};
use lamina_lang::parser::parse;
use lamina_lang::span::{FileId, SourceFile};
use lamina_lang::types::typecheck;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, Location, MarkupContent, MarkupKind, Position, Range, Url,
};

#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub name: String,
    pub detail: String,
    pub range: Range,
    /// If set, definition is in another file (e.g. path `use`).
    pub target_uri: Option<Url>,
}

#[derive(Debug, Default)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: HashMap<String, SymbolDef>,
    pub source: String,
    pub path: PathBuf,
}

pub fn find_project_root(file: &Path) -> PathBuf {
    let mut cur = file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    loop {
        if cur.join("Lamina.toml").is_file() {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    file.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn analyze(path: &Path, source: &str) -> Analysis {
    let root = find_project_root(path);
    let mut analysis = Analysis {
        source: source.to_string(),
        path: path.to_path_buf(),
        ..Default::default()
    };

    let file = SourceFile::new(FileId(0), path.display().to_string(), source);
    let module = match parse(&file) {
        Ok(m) => m,
        Err(e) => {
            analysis.diagnostics = compile_error_to_diagnostics(&e, &file);
            return analysis;
        }
    };

    collect_symbols(&module, &file, &root, &mut analysis);

    let mut ctx = ModuleLoadContext::new(root.clone());
    // LSP: avoid network; remotes only if already cached
    ctx.offline = true;

    let module_for_types = match load_and_merge(&file, module.clone(), &ctx) {
        Ok(loaded) => {
            // Index pub fns from resolved modules (path / std / git) with real file URIs
            // so goto-definition jumps into golang.lam etc., not the consumer file.
            index_imported_modules(&loaded.resolved, &mut analysis);
            loaded.module
        }
        Err(e) => {
            analysis
                .diagnostics
                .extend(compile_error_to_diagnostics(&e, &file));
            module
        }
    };

    if let Err(e) = typecheck(&file, &module_for_types) {
        analysis
            .diagnostics
            .extend(compile_error_to_diagnostics(&e, &file));
    }

    // Optional lints as warnings when project config exists
    let _ = LaminaToml::load_or_default(root.join("Lamina.toml"));

    analysis
}

fn index_imported_modules(
    resolved: &[lamina_lang::lock::ResolvedModule],
    analysis: &mut Analysis,
) {
    for rm in resolved {
        let Ok(src) = std::fs::read_to_string(&rm.path) else {
            continue;
        };
        let Ok(uri) = Url::from_file_path(&rm.path) else {
            continue;
        };
        let sf = SourceFile::new(FileId(0), rm.path.display().to_string(), src);
        let Ok(mod_) = parse(&sf) else {
            continue;
        };
        for item in &mod_.items {
            if let Item::Fn(f) = item {
                if !f.is_pub {
                    continue;
                }
                // Prefer definition site in the module file over any local stub.
                analysis.symbols.insert(
                    f.name.clone(),
                    SymbolDef {
                        name: f.name.clone(),
                        detail: format!("{}  ({})", format_fn_sig(f), rm.path.display()),
                        range: span_to_range(&sf, f.span),
                        target_uri: Some(uri.clone()),
                    },
                );
            }
        }
    }
}

fn format_fn_sig(f: &lamina_lang::ast::FnDecl) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty.as_str()))
        .collect();
    let vis = if f.is_pub { "pub " } else { "" };
    format!(
        "{vis}fn {}({}) -> {}",
        f.name,
        params.join(", "),
        f.ret.as_str()
    )
}

fn collect_symbols(module: &Module, file: &SourceFile, root: &Path, analysis: &mut Analysis) {
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                analysis.symbols.insert(
                    f.name.clone(),
                    SymbolDef {
                        name: f.name.clone(),
                        detail: format_fn_sig(f),
                        range: span_to_range(file, f.span),
                        target_uri: None,
                    },
                );
            }
            Item::Const(c) => {
                analysis.symbols.insert(
                    c.name.clone(),
                    SymbolDef {
                        name: c.name.clone(),
                        detail: format!("const {}: {}", c.name, c.ty.as_str()),
                        range: span_to_range(file, c.span),
                        target_uri: None,
                    },
                );
            }
            Item::Let(l) => {
                let ty =
                    l.ty.as_ref()
                        .map(|t| t.as_str())
                        .unwrap_or_else(|| "unknown".into());
                analysis.symbols.insert(
                    l.name.clone(),
                    SymbolDef {
                        name: l.name.clone(),
                        detail: format!("let {}: {ty}", l.name),
                        range: span_to_range(file, l.span),
                        target_uri: None,
                    },
                );
            }
            Item::Target(t) => {
                analysis.symbols.insert(
                    t.name.clone(),
                    SymbolDef {
                        name: t.name.clone(),
                        detail: format!("pub target {}: Stage", t.name),
                        range: span_to_range(file, t.span),
                        target_uri: None,
                    },
                );
            }
            Item::Use(u) => {
                analysis.symbols.insert(
                    format!("use:{}", u.path),
                    SymbolDef {
                        name: u.path.clone(),
                        detail: format!("use \"{}\"", u.path),
                        range: span_to_range(file, u.span),
                        target_uri: resolve_use_uri(&u.path, &analysis.path, root),
                    },
                );
            }
            Item::Arg(a) => {
                analysis.symbols.insert(
                    a.name.clone(),
                    SymbolDef {
                        name: a.name.clone(),
                        detail: format!("arg \"{}\"", a.name),
                        range: span_to_range(file, a.span),
                        target_uri: None,
                    },
                );
            }
        }
    }
}

fn resolve_use_uri(spec: &str, file_path: &Path, root: &Path) -> Option<Url> {
    if spec.starts_with("git+") {
        return None;
    }
    if let Some(rest) = spec.strip_prefix("std/") {
        // Walk ancestors for stdlib/ (same as ModuleLoadContext).
        let mut cur = root.to_path_buf();
        loop {
            let base = cur.join("stdlib");
            let p = base.join(rest);
            for c in [p.clone(), p.with_extension("lam")] {
                if c.is_file() {
                    return Url::from_file_path(c).ok();
                }
            }
            if !cur.pop() {
                break;
            }
        }
        return None;
    }
    let parent = file_path.parent()?;
    let target = parent.join(spec);
    let canon = target.canonicalize().ok()?;
    Url::from_file_path(canon).ok()
}

fn compile_error_to_diagnostics(err: &CompileError, file: &SourceFile) -> Vec<Diagnostic> {
    if err.diagnostics.is_empty() {
        return vec![Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("lamina".into()),
            message: err.message.clone(),
            ..Default::default()
        }];
    }
    err.diagnostics
        .iter()
        .map(|d| {
            let range = d.span.map(|s| span_to_range(file, s)).unwrap_or(Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            });
            let severity = match d.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
            };
            let mut message = d.message.clone();
            if let Some(help) = &d.help {
                message.push('\n');
                message.push_str(help);
            }
            Diagnostic {
                range,
                severity: Some(severity),
                source: Some("lamina".into()),
                message,
                ..Default::default()
            }
        })
        .collect()
}

pub fn hover_at(analysis: &Analysis, pos: Position) -> Option<MarkupContent> {
    let offset = crate::position::position_to_offset(&analysis.source, pos);
    let (_, _, word) = word_at_offset(&analysis.source, offset)?;

    if let Some(sig) = stage_method_docs(word) {
        return Some(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```lamina\n{sig}\n```\n\nStage intrinsic method."),
        });
    }
    if word == "Stage" {
        return Some(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```lamina\nStage.from(image: String) -> Stage\nStage.from_arg(name: String) -> Stage\n```".into(),
        });
    }
    if word == "Mount" {
        return Some(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "```lamina\nMount.cache(target, id) -> Mount\nMount.secret(id, target) -> Mount\nMount.ssh(target[, id]) -> Mount\nMount.bind(source, target) -> Mount\n```".into(),
        });
    }
    if let Some(sym) = analysis.symbols.get(word) {
        return Some(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```lamina\n{}\n```", sym.detail),
        });
    }
    // Type names
    for t in ["String", "Int", "Bool", "Stage", "Mount", "List"] {
        if word == t {
            return Some(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("type `{t}`"),
            });
        }
    }
    let _ = Type::String;
    None
}

pub fn goto_at(analysis: &Analysis, pos: Position) -> Option<Location> {
    let offset = crate::position::position_to_offset(&analysis.source, pos);
    let (start, end, word) = word_at_offset(&analysis.source, offset)?;
    let _ = (start, end);

    if let Some(sym) = analysis.symbols.get(word) {
        let uri = if let Some(uri) = &sym.target_uri {
            uri.clone()
        } else {
            Url::from_file_path(&analysis.path).ok()?
        };
        return Some(Location {
            uri,
            range: sym.range,
        });
    }

    // Goto on use path: if cursor on a string in use — weak: match use: keys
    for (k, sym) in &analysis.symbols {
        if let Some(path) = k.strip_prefix("use:") {
            if analysis.source[start..end.min(analysis.source.len())].contains(path)
                || word == path
                || path.contains(word)
            {
                if let Some(uri) = &sym.target_uri {
                    return Some(Location {
                        uri: uri.clone(),
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 0,
                            },
                        },
                    });
                }
            }
        }
    }
    None
}

pub fn stage_method_docs(name: &str) -> Option<&'static str> {
    Some(match name {
        "from" => "Stage.from(image: String) -> Stage",
        "from_arg" => "Stage.from_arg(name: String) -> Stage",
        "workdir" => "(s: Stage).workdir(path: String) -> Stage",
        "run" => "(s: Stage).run(command: String) -> Stage",
        "run_with" => "(s: Stage).run_with(command: String, mounts: List[Mount]) -> Stage",
        "copy" => "(s: Stage).copy(src: String, dst: String) -> Stage",
        "copy_many" => "(s: Stage).copy_many(srcs: List[String], dst: String) -> Stage",
        "copy_from" => "(s: Stage).copy_from(from: Stage, src: String, dst: String) -> Stage",
        "env" => "(s: Stage).env(key: String, value: String) -> Stage",
        "arg" => "(s: Stage).arg(name: String) -> Stage",
        "arg_default" => "(s: Stage).arg_default(name: String, default: String) -> Stage",
        "user" => "(s: Stage).user(user: String) -> Stage",
        "entrypoint" => "(s: Stage).entrypoint(args: List[String]) -> Stage",
        "cmd" => "(s: Stage).cmd(args: List[String]) -> Stage",
        "expose" => "(s: Stage).expose(port: Int) -> Stage",
        "name" => "(s: Stage).name(stage_name: String) -> Stage",
        "label" => "(s: Stage).label(key: String, value: String) -> Stage",
        "healthcheck" => "(s: Stage).healthcheck(cmd: String) -> Stage",
        "platform" => "(s: Stage).platform(platform: String) -> Stage",
        "cache" => "Mount.cache(target: String, id: String) -> Mount",
        "secret" => "Mount.secret(id: String, target: String) -> Mount",
        "ssh" => "Mount.ssh(target: String[, id: String]) -> Mount",
        "bind" => "Mount.bind(source: String, target: String) -> Mount",
        _ => return None,
    })
}

pub fn stage_method_completions() -> Vec<(&'static str, &'static str)> {
    [
        ("workdir", "workdir(path)"),
        ("run", "run(command)"),
        ("run_with", "run_with(command, mounts)"),
        ("copy", "copy(src, dst)"),
        ("copy_many", "copy_many(srcs, dst)"),
        ("copy_from", "copy_from(from, src, dst)"),
        ("env", "env(key, value)"),
        ("arg", "arg(name)"),
        ("arg_default", "arg_default(name, default)"),
        ("user", "user(name)"),
        ("entrypoint", "entrypoint(args)"),
        ("cmd", "cmd(args)"),
        ("expose", "expose(port)"),
        ("name", "name(label)"),
        ("label", "label(key, value)"),
        ("healthcheck", "healthcheck(cmd)"),
        ("platform", "platform(triple)"),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn diagnostics_on_type_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.lam");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"pub target app = Stage.from(1);"#).unwrap();
        let src = std::fs::read_to_string(&path).unwrap();
        let a = analyze(&path, &src);
        assert!(!a.diagnostics.is_empty(), "expected diagnostics, got none");
    }

    #[test]
    fn symbols_for_fn() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ok.lam");
        std::fs::write(
            &path,
            r#"
fn helper(s: Stage) -> Stage { s }
pub target app = helper(Stage.from("alpine:3.19"));
"#,
        )
        .unwrap();
        let src = std::fs::read_to_string(&path).unwrap();
        let a = analyze(&path, &src);
        assert!(a.symbols.contains_key("helper"));
        assert!(a.symbols.contains_key("app"));
    }

    #[test]
    fn goto_imported_stdlib_fn() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("stdlib")).unwrap();
        std::fs::write(
            repo.join("stdlib/golang.lam"),
            r#"
pub fn from_version(version: String) -> Stage {
  Stage.from("golang:" + version)
}
"#,
        )
        .unwrap();
        let proj = repo.join("examples/stdlib-go");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        let image = proj.join("src/image.lam");
        std::fs::write(
            &image,
            r#"
use "std/golang.lam";
pub target app = from_version("1.22-alpine").name("app");
"#,
        )
        .unwrap();
        let src = std::fs::read_to_string(&image).unwrap();
        let a = analyze(&image, &src);
        let sym = a
            .symbols
            .get("from_version")
            .expect("from_version should be indexed from stdlib");
        let uri = sym.target_uri.as_ref().expect("should point at golang.lam");
        let path = uri.to_file_path().unwrap();
        assert!(
            path.ends_with("stdlib/golang.lam") || path.ends_with("golang.lam"),
            "uri path = {path:?}"
        );
        // Hover word position: start of from_version on line 1 (0-based)
        let pos = Position {
            line: 1,
            character: 18, // inside from_version(
        };
        let loc = goto_at(&a, pos).expect("goto from_version");
        assert_eq!(&loc.uri, uri);
    }
}
