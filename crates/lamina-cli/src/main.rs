//! `lamina` CLI.

mod build_targets;

use clap::{Parser, Subcommand};
use lamina_lang::compile::{compile_project, write_lockfile, CompileOptions};
use lamina_lang::config::{resolve_project_root, CONFIG_FILE};
use lamina_lang::lint::LINT_IDS;
use lamina_llb::{lower, render_internal_dockerfile, summary};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

/// Path to a Lamina project (`Lamina.toml` here, or under `.lamina/`).
const PATH_HELP: &str = "Project directory (Lamina.toml here or in .lamina/)";

#[derive(Debug, Parser)]
#[command(
    name = "lamina",
    version = lamina_lang::VERSION,
    about = "Lamina: typed language for container images (BuildKit LLB → OCI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Typecheck + build IR + lints (no BuildKit daemon).
    Check {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
        /// Promote lint id(s) to errors (`all` / `warnings` = every lint).
        #[arg(long = "deny", value_name = "LINT")]
        deny: Vec<String>,
        /// Print available lint ids
        #[arg(long)]
        list_lints: bool,
        /// Require Lamina.lock and verify module hashes
        #[arg(long)]
        locked: bool,
    },
    /// Print solve_set / stage DAG summary.
    Explain {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        /// All `pub target`s (same as omitting `--target` for explain).
        #[arg(long = "all-targets")]
        all_targets: bool,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
    },
    /// Dump stable LLB op summary (debug).
    EmitLlb {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "all-targets")]
        all_targets: bool,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
    },
    /// Lower + solve via Docker Buildx / BuildKit and tag an image.
    Build {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        /// Build every `pub target` (sequential solves; default tags `{package}:{target}`).
        #[arg(long = "all-targets")]
        all_targets: bool,
        #[arg(short = 't', long = "tag")]
        tags: Vec<String>,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
        #[arg(long)]
        builder: Option<String>,
        #[arg(long, default_value = "auto")]
        progress: String,
        /// Target platform(s), e.g. linux/amd64 or linux/amd64,linux/arm64
        #[arg(long = "platform", value_name = "PLAT")]
        platforms: Vec<String>,
        /// Push to registry (required for multi-platform)
        #[arg(long)]
        push: bool,
        #[arg(long = "deny", value_name = "LINT")]
        deny: Vec<String>,
        #[arg(long)]
        locked: bool,
    },
    /// Write/update Lamina.lock for path/stdlib modules.
    Lock {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
    },
    /// Lossy Dockerfile dump for debugging only (NOT a product artifact).
    EmitDockerfile {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "all-targets")]
        all_targets: bool,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
    },
    /// Format `.lam` sources (project entry or explicit files).
    Fmt {
        #[arg(default_value = ".", help = PATH_HELP)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        check: bool,
    },
    /// Start the Language Server (stdio). Same as `lamina-lsp`.
    Lsp,
    /// Remove local images and project BuildKit cache produced by `lamina build`.
    Clear {
        #[arg(default_value = ".", help = PATH_HELP)]
        path: PathBuf,
        /// Also remove these image refs (in addition to labeled images and `{name}:dev`)
        #[arg(short = 't', long = "tag")]
        tags: Vec<String>,
        /// Print what would be removed without deleting
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("{e:?}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> miette::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Check {
            path,
            params,
            build_args,
            deny,
            list_lints,
            locked,
        } => {
            if list_lints {
                for id in LINT_IDS {
                    println!("{id}");
                }
                return Ok(());
            }
            let mut opts = options(params, build_args, deny);
            opts.locked = locked;
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            for f in &compiled.lint_findings {
                eprintln!("warning: [{}] {}", f.id, f.message);
            }
            println!(
                "ok: {} target(s) in {} ({} lint warning(s), {} module(s))",
                compiled.ir.targets.len(),
                compiled.config.package.name,
                compiled.lint_findings.len(),
                compiled.resolved_modules.len()
            );
        }
        Commands::Explain {
            path,
            target,
            all_targets,
            params,
            build_args,
        } => {
            let mut opts = options(params, build_args, vec![]);
            opts.run_lints = false;
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            let names = resolve_cli_targets(&compiled, &target, all_targets)?;
            print!("{}", compiled.ir.explain(&names));
        }
        Commands::EmitLlb {
            path,
            target,
            all_targets,
            params,
            build_args,
        } => {
            let mut opts = options(params, build_args, vec![]);
            opts.run_lints = false;
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            let names = resolve_cli_targets(&compiled, &target, all_targets)?;
            let g = lower(&compiled.ir, &names);
            print!("{}", summary(&g));
        }
        Commands::Build {
            path,
            target,
            all_targets,
            tags,
            params,
            build_args,
            builder,
            progress,
            platforms,
            push,
            deny,
            locked,
        } => {
            let mut opts = options(params, build_args, deny);
            opts.locked = locked;
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            for f in &compiled.lint_findings {
                eprintln!("warning: [{}] {}", f.id, f.message);
            }
            let context = compiled.config.context_path(&compiled.root);
            lamina_client::ensure_context(&context).map_err(|e| miette::miette!(e))?;
            let available: Vec<String> = compiled.ir.targets.keys().cloned().collect();
            let selected = build_targets::select_target_names(&available, &target, all_targets)
                .map_err(|e| miette::miette!("{e}"))?;
            let plans = build_targets::plan_solves(&compiled.config.package.name, &selected, &tags)
                .map_err(|e| miette::miette!("{e}"))?;
            let mut plats = platforms;
            if plats.is_empty() {
                plats = compiled.config.build.platforms.clone();
            }
            // flatten comma-separated --platform values
            let platforms: Vec<String> = plats
                .into_iter()
                .flat_map(|p| {
                    p.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .collect();
            eprintln!(
                "note: solve uses an internal BuildKit bridge (ephemeral, not written to the project)."
            );
            let n = plans.len();
            let mut built_tags = Vec::new();
            for (i, plan) in plans.iter().enumerate() {
                if n > 1 {
                    let label = if plan.target.is_empty() {
                        "(default)".into()
                    } else {
                        plan.target.clone()
                    };
                    eprintln!(
                        "building target {label} ({}/{}) → {}",
                        i + 1,
                        n,
                        plan.tags.join(", ")
                    );
                }
                let req = lamina_client::SolveRequest {
                    context: context.clone(),
                    targets: if plan.target.is_empty() {
                        Vec::new()
                    } else {
                        vec![plan.target.clone()]
                    },
                    tags: plan.tags.clone(),
                    progress: progress.clone(),
                    builder: builder.clone(),
                    platforms: platforms.clone(),
                    push,
                    project_root: compiled.root.clone(),
                    package_name: compiled.config.package.name.clone(),
                };
                lamina_client::solve(&compiled.ir, &req).map_err(|e| miette::miette!(e))?;
                built_tags.extend(plan.tags.iter().cloned());
            }
            println!("built {}", built_tags.join(", "));
        }
        Commands::Clear {
            path,
            tags,
            dry_run,
        } => {
            let root = resolve_project_root(&path);
            let cfg_path = root.join(CONFIG_FILE);
            let config = if cfg_path.is_file() {
                lamina_lang::config::LaminaToml::load(&cfg_path).map_err(|e| miette::miette!(e))?
            } else {
                return Err(miette::miette!(
                    "no {} in {} (or under .lamina/)",
                    CONFIG_FILE,
                    path.display()
                ));
            };
            let res = lamina_client::clear(&lamina_client::ClearRequest {
                project_root: root,
                package_name: config.package.name.clone(),
                extra_tags: tags,
                dry_run,
            })
            .map_err(|e| miette::miette!(e))?;
            let verb = if dry_run { "would remove" } else { "removed" };
            if res.removed_images.is_empty() && res.removed_cache.is_none() {
                println!("nothing to clear for package `{}`", config.package.name);
            } else {
                for img in &res.removed_images {
                    println!("{verb} image {img}");
                }
                if let Some(cache) = &res.removed_cache {
                    println!("{verb} build cache {}", cache.display());
                }
            }
        }
        Commands::Lock { path } => {
            let mut opts = options(vec![], vec![], vec![]);
            opts.run_lints = false;
            let lock_path = write_lockfile(&path, &opts).map_err(to_miette)?;
            println!("wrote {}", lock_path.display());
        }
        Commands::EmitDockerfile {
            path,
            target,
            all_targets,
            params,
            build_args,
        } => {
            eprintln!(
                "warning: emit-dockerfile is a LOSSY debug export — not a product artifact; do not commit as source of truth"
            );
            let mut opts = options(params, build_args, vec![]);
            opts.run_lints = false;
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            let names = resolve_cli_targets(&compiled, &target, all_targets)?;
            print!("{}", render_internal_dockerfile(&compiled.ir, &names));
        }
        Commands::Fmt { paths, check } => {
            let mut files: Vec<PathBuf> = Vec::new();
            for p in paths {
                if p.is_dir() {
                    let root = resolve_project_root(&p);
                    let cfg = root.join(CONFIG_FILE);
                    let entry = if cfg.exists() {
                        lamina_lang::config::LaminaToml::load(&cfg)
                            .map(|c| c.entry_path(&root))
                            .unwrap_or_else(|_| root.join("src/image.lam"))
                    } else {
                        root.join("src/image.lam")
                    };
                    if entry.is_file() {
                        files.push(entry);
                    }
                    let src_dir = root.join("src");
                    if src_dir.is_dir() {
                        if let Ok(rd) = std::fs::read_dir(src_dir) {
                            for e in rd.flatten() {
                                let path = e.path();
                                if path.extension().and_then(|x| x.to_str()) == Some("lam")
                                    && !files.contains(&path)
                                {
                                    files.push(path);
                                }
                            }
                        }
                    }
                } else {
                    files.push(p);
                }
            }
            if files.is_empty() {
                return Err(miette::miette!("no .lam files to format"));
            }
            let mut dirty = false;
            for f in files {
                let src = std::fs::read_to_string(&f)
                    .map_err(|e| miette::miette!("read {}: {e}", f.display()))?;
                let formatted = lamina_lang::fmt::format_source(f.to_string_lossy().as_ref(), &src)
                    .map_err(to_miette)?;
                if formatted != src {
                    if check {
                        eprintln!("would reformat {}", f.display());
                        dirty = true;
                    } else {
                        std::fs::write(&f, formatted)
                            .map_err(|e| miette::miette!("write {}: {e}", f.display()))?;
                        println!("formatted {}", f.display());
                    }
                } else if !check {
                    println!("unchanged {}", f.display());
                }
            }
            if check && dirty {
                return Err(miette::miette!("formatting differs (run lamina fmt)"));
            }
        }
        Commands::Lsp => {
            // Stdio LSP; logs go to stderr via lamina-lsp.
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| miette::miette!("tokio runtime: {e}"))?;
            rt.block_on(lamina_lsp::run_stdio());
        }
    }
    Ok(())
}

fn resolve_cli_targets(
    compiled: &lamina_lang::compile::Compiled,
    named: &[String],
    all_targets: bool,
) -> miette::Result<Vec<String>> {
    let available: Vec<String> = compiled.ir.targets.keys().cloned().collect();
    build_targets::select_target_names(&available, named, all_targets)
        .map_err(|e| miette::miette!("{e}"))
}

fn options(params: Vec<String>, build_args: Vec<String>, deny: Vec<String>) -> CompileOptions {
    let lint_deny: Vec<String> = deny
        .into_iter()
        .flat_map(|d| {
            d.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();
    CompileOptions {
        params: parse_kv(params),
        build_args: parse_kv(build_args),
        targets: vec![],
        stdlib_paths: vec![],
        lint_deny,
        run_lints: true,
        locked: false,
    }
}

fn parse_kv(items: Vec<String>) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for it in items {
        if let Some((k, v)) = it.split_once('=') {
            m.insert(k.to_string(), v.to_string());
        }
    }
    m
}

fn to_miette(e: lamina_lang::diag::CompileError) -> miette::Error {
    miette::Error::new(e)
}
