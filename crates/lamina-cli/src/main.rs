//! `lamina` CLI.

use clap::{Parser, Subcommand};
use lamina::compile::{compile_project, CompileOptions};
use lamina_llb::{lower, summary};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "lamina",
    version = lamina::VERSION,
    about = "Lamina: typed language for container images (BuildKit LLB → OCI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Typecheck + build IR (no BuildKit daemon).
    Check {
        /// Project root (directory with Lamina.toml)
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
    },
    /// Print solve_set / stage DAG summary.
    Explain {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
    },
    /// Dump stable LLB op summary (debug).
    EmitLlb {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long = "build-arg", value_name = "KEY=VALUE")]
        build_args: Vec<String>,
    },
    /// Lower + solve via Docker Buildx / BuildKit and tag an image.
    Build {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        target: Vec<String>,
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
        } => {
            let opts = options(params, build_args);
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            println!(
                "ok: {} target(s) in {}",
                compiled.ir.targets.len(),
                compiled.config.package.name
            );
        }
        Commands::Explain {
            path,
            target,
            params,
            build_args,
        } => {
            let opts = options(params, build_args);
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            print!("{}", compiled.ir.explain(&target));
        }
        Commands::EmitLlb {
            path,
            target,
            params,
            build_args,
        } => {
            let opts = options(params, build_args);
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            let g = lower(&compiled.ir, &target);
            print!("{}", summary(&g));
        }
        Commands::Build {
            path,
            target,
            tags,
            params,
            build_args,
            builder,
            progress,
        } => {
            let opts = options(params, build_args);
            let compiled = compile_project(&path, &opts).map_err(to_miette)?;
            let context = compiled.config.context_path(&compiled.root);
            lamina_client::ensure_context(&context).map_err(|e| miette::miette!(e))?;
            let tags = if tags.is_empty() {
                vec![format!("{}:dev", compiled.config.package.name)]
            } else {
                tags
            };
            let req = lamina_client::SolveRequest {
                context,
                targets: target,
                tags: tags.clone(),
                progress,
                builder,
            };
            eprintln!(
                "note: 0.1 solve uses an internal BuildKit bridge (ephemeral, not written to the project)."
            );
            lamina_client::solve(&compiled.ir, &req).map_err(|e| miette::miette!(e))?;
            println!("built {}", tags.join(", "));
        }
    }
    Ok(())
}

fn options(params: Vec<String>, build_args: Vec<String>) -> CompileOptions {
    CompileOptions {
        params: parse_kv(params),
        build_args: parse_kv(build_args),
        targets: vec![],
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

fn to_miette(e: lamina::diag::CompileError) -> miette::Error {
    miette::Error::new(e)
}
