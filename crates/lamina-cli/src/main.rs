//! `lamina` CLI entrypoint.
//!
//! Subcommands (`check`, `explain`, `build`, …) land in later issues.
//! Scaffold only provides `--version` / `--help`.

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "lamina",
    version = lamina::VERSION,
    about = "Lamina: typed language for container images (BuildKit LLB → OCI)",
    long_about = None
)]
struct Cli {
    // Future: subcommands. Keep empty so `lamina --version` works today.
}

fn main() {
    let _cli = Cli::parse();
    // No default action yet — use --help or --version.
    // Later: default to help or require a subcommand.
    eprintln!(
        "lamina {version}: no subcommands yet (scaffold). Try --help.",
        version = lamina::VERSION
    );
}
