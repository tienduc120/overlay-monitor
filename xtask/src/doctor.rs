//! `cargo xtask doctor`: checks the toolchain and the pinned tools, and prints the install command for anything
//! missing. It reports; it never installs.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result};

struct Tool<'a> {
    name: &'a str,
    program: &'a str,
    args: &'a [&'a str],
    version: &'a str,
    install: &'a str,
}

/// Moving a pin is an ADR (docs/adr/0001-foundations.md); CI installs the same versions.
const TOOLS: [Tool<'static>; 5] = [
    Tool {
        name: "cargo-nextest",
        program: "cargo",
        args: &["nextest", "--version"],
        version: "0.9.151",
        install: "cargo binstall cargo-nextest@0.9.151",
    },
    Tool {
        name: "cargo-deny",
        program: "cargo",
        args: &["deny", "--version"],
        version: "0.20.2",
        install: "cargo binstall cargo-deny@0.20.2",
    },
    Tool {
        name: "cargo-shear",
        program: "cargo",
        args: &["shear", "--version"],
        version: "1.14.0",
        install: "cargo binstall cargo-shear@1.14.0",
    },
    Tool {
        name: "typos",
        program: "typos",
        args: &["--version"],
        version: "1.52.2",
        install: "cargo binstall typos-cli@1.52.2",
    },
    Tool {
        name: "lefthook",
        program: "lefthook",
        args: &["version"],
        version: "2.2.2",
        install: "winget install evilmartians.lefthook (Windows) or brew install lefthook",
    },
];

pub(crate) fn run(root: &Path, out: &mut impl Write) -> Result<bool> {
    let toolchain: toml::Table = toml::from_str(&fs::read_to_string(root.join("rust-toolchain.toml"))?)?;
    let channel = toolchain
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(toml::Value::as_str)
        .context("rust-toolchain.toml has no toolchain.channel")?;
    let rustc = Tool {
        name: "rustc",
        program: "rustc",
        args: &["--version"],
        version: channel,
        install: "rustup (it reads rust-toolchain.toml)",
    };
    let mut ok = check(&rustc, out)?;
    for tool in &TOOLS {
        ok &= check(tool, out)?;
    }
    // Ask git where the hook lives, so a worktree or `core.hooksPath` is read correctly.
    let hook = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--git-path", "hooks/pre-commit"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| root.join(String::from_utf8_lossy(&o.stdout).trim()));
    let hooked = hook.is_some_and(|path| fs::read_to_string(path).is_ok_and(|h| h.contains("lefthook")));
    if hooked {
        writeln!(out, "ok       git hooks")?;
    } else {
        writeln!(out, "MISSING  git hooks: run `lefthook install` once per clone")?;
    }
    Ok(ok && hooked)
}

fn check(tool: &Tool<'_>, out: &mut impl Write) -> Result<bool> {
    let found = Command::new(tool.program).args(tool.args).current_dir(".").output();
    let text = found
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    if text.contains(tool.version) {
        writeln!(out, "ok       {} {}", tool.name, tool.version)?;
        return Ok(true);
    }
    let state = if text.is_empty() { "MISSING " } else { "WRONG   " };
    let seen = text.lines().next().map(|l| format!(" (found {l})")).unwrap_or_default();
    writeln!(out, "{state} {} {}{seen}: {}", tool.name, tool.version, tool.install)?;
    Ok(false)
}
