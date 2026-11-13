//! Runs every gate in one pass (ADR-0002): each step runs even after a failure, the full output goes to
//! `.xtask/logs`, and only the tail of a failure comes back.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;

/// How much of a failing step's output comes back; the rest is in the log.
const TAIL_LINES: usize = 30;

enum Step {
    Gates,
    Rules,
    Cmd {
        name: &'static str,
        program: &'static str,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
    },
}

/// Warnings fail the build, without touching RUSTFLAGS and the build cache (Rust 1.97).
const DENY_WARNINGS: &[(&str, &str)] = &[("CARGO_BUILD_WARNINGS", "deny")];

fn plan(full: bool) -> Vec<Step> {
    let mut steps = vec![
        Step::Cmd {
            name: "fmt",
            program: "cargo",
            args: &["fmt", "--all", "--check"],
            env: &[],
        },
        Step::Gates,
        Step::Cmd {
            name: "clippy",
            program: "cargo",
            args: &["clippy", "--workspace", "--all-targets", "--locked"],
            env: DENY_WARNINGS,
        },
        Step::Cmd {
            name: "tests",
            program: "cargo",
            args: &["nextest", "run", "--workspace", "--locked", "--no-tests=pass"],
            env: DENY_WARNINGS,
        },
        Step::Cmd {
            name: "typos",
            program: "typos",
            args: &[],
            env: &[],
        },
    ];
    if full {
        steps.extend([
            Step::Cmd {
                name: "deny",
                program: "cargo",
                args: &["deny", "--locked", "check"],
                env: &[],
            },
            Step::Cmd {
                name: "unused deps",
                program: "cargo",
                args: &["shear"],
                env: &[],
            },
            Step::Cmd {
                name: "docs",
                program: "cargo",
                args: &["doc", "--workspace", "--no-deps", "--locked"],
                env: &[("RUSTDOCFLAGS", "-D warnings")],
            },
            Step::Rules,
        ]);
    }
    steps
}

pub(crate) fn run(root: &Path, full: bool, out: &mut impl Write) -> Result<bool> {
    let logs = root.join(".xtask/logs");
    fs::create_dir_all(&logs)?;
    let mut failed = 0;
    for step in plan(full) {
        let start = Instant::now();
        let (name, pass, output) = match step {
            Step::Gates => {
                let found = super::findings(root)?;
                (
                    "gates",
                    found.is_empty(),
                    found.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"),
                )
            }
            Step::Rules => {
                let mut buf = Vec::new();
                let pass = super::rules::run(root, &mut buf)?;
                ("rules", pass, String::from_utf8_lossy(&buf).into_owned())
            }
            Step::Cmd {
                name,
                program,
                args,
                env,
            } => {
                let (pass, output) = command(root, program, args, env);
                (name, pass, output)
            }
        };
        let log = logs.join(format!("{}.log", name.replace(' ', "-")));
        fs::write(&log, &output)?;
        report(out, name, pass, start.elapsed(), &output, &log)?;
        failed += usize::from(!pass);
    }
    install_hook_binary(root);
    writeln!(
        out,
        "\n{}",
        if failed == 0 {
            "all gates passed".to_owned()
        } else {
            format!("{failed} gate(s) failed")
        }
    )?;
    Ok(failed == 0)
}

fn command(root: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> (bool, String) {
    match Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(root)
        .output()
    {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            (o.status.success(), text)
        }
        Err(e) => (
            false,
            format!("cannot run {program}: {e}. Run `cargo xtask doctor` for the install command."),
        ),
    }
}

fn report(out: &mut impl Write, name: &str, pass: bool, took: Duration, output: &str, log: &Path) -> Result<()> {
    let took = format!("{:.1}s", took.as_secs_f64());
    if pass {
        writeln!(out, "ok    {name}  {took}")?;
        return Ok(());
    }
    writeln!(out, "FAIL  {name}  {took}")?;
    let lines: Vec<&str> = output.trim_end().lines().collect();
    let skip = lines.len().saturating_sub(TAIL_LINES);
    if skip > 0 {
        writeln!(out, "      ... {skip} earlier lines in the log")?;
    }
    for line in lines.iter().skip(skip) {
        writeln!(out, "      {line}")?;
    }
    writeln!(out, "      full log: {}", log.display())?;
    Ok(())
}

/// The agent hooks run this copy, so a hook starts in milliseconds and a rebuild never fights a running binary.
fn install_hook_binary(root: &Path) {
    let bin = root.join(".xtask/bin");
    if let Ok(exe) = std::env::current_exe()
        && fs::create_dir_all(&bin).is_ok()
        && let Some(name) = exe.file_name()
    {
        // A failed copy only means the hooks keep the previous binary.
        let _copied = fs::copy(&exe, bin.join(name));
    }
}
