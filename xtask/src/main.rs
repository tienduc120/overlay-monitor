//! `cargo xtask`: the repository's single command surface (AGENTS.md).

mod doctor;
mod gates;
mod hook;
mod modules;
mod scaffold;

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cargo xtask",
    about = "The repository's single command surface: gates, guard, and generator."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run every gate (the pre-commit hook does). --full adds cargo-deny, unused deps, docs, and the rules proof.
    Verify {
        #[arg(long)]
        full: bool,
    },
    /// Prove every rule fires on its fixture in fixtures/violations and stays silent on fixtures/clean.
    Rules,
    /// Check a commit message file (the commit-msg hook calls this).
    Commitmsg {
        file: PathBuf,
        /// Judge a refactor by this commit's diff instead of the staged one (CI passes each pull request commit).
        #[arg(long)]
        commit: Option<String>,
    },
    /// Agent hook: reads Claude Code's tool call as JSON on stdin; exit 2 blocks it.
    Hook {
        #[arg(value_parser = ["pre-edit", "pre-bash"])]
        event: String,
    },
    /// Check the toolchain and the pinned tools; prints the install command for anything missing.
    Doctor,
    /// Download the pinned `PawnIO` modules, check their hashes, and copy them next to the debug and release builds.
    Modules,
    /// Start a service by copying the witness services/example under a new name.
    NewService {
        name: String,
        #[arg(long)]
        aggregate: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // xtask lives one level below the repository root; this holds wherever the command is run from.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let mut out = io::stdout().lock();
    let result = match cli.command {
        Cmd::Verify { full } => gates::verify::run(&root, full, &mut out),
        Cmd::Rules => gates::rules::run(&root, &mut out),
        Cmd::Commitmsg { file, commit } => {
            gates::commitmsg::run(&root, &file, commit.as_deref(), &mut io::stderr().lock())
        }
        Cmd::Hook { event } => return hook::run(&root, &event),
        Cmd::Doctor => doctor::run(&root, &mut out),
        Cmd::Modules => modules::run(&root, &mut out),
        Cmd::NewService { name, aggregate } => scaffold::new_service(&root, &name, &aggregate, &mut out).map(|()| true),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            writeln!(io::stderr(), "xtask: {e:#}").ok();
            ExitCode::FAILURE
        }
    }
}
