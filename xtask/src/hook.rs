//! `cargo xtask hook pre-edit|pre-bash`: Claude Code runs this before every edit and shell command
//! (.claude/settings.json). Exit 2 blocks the call and the reason on stderr goes to the agent; any other failure exits
//! 1, which lets the call through, so a broken guard never bricks a session (ADR-0005).

use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::Deserialize;

use crate::gates::guard::{APPROVAL_ENV, Repo, check_command, check_edit, repo_relative};

const BLOCK: u8 = 2;

#[derive(Default, Deserialize)]
struct Call {
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    tool_input: ToolInput,
}

#[derive(Default, Deserialize)]
struct ToolInput {
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    notebook_path: String,
    #[serde(default)]
    command: String,
}

/// `built_root` is where this binary was built; the hook prefers the checkout it is run in (settings.json `cd`s
/// there), so a second checkout or worktree guards itself.
pub(crate) fn run(built_root: &Path, event: &str) -> ExitCode {
    // No size cap: a large Write payload that failed to parse would be let through.
    let mut input = String::new();
    let call: Call = match io::stdin()
        .read_to_string(&mut input)
        .map(|_| serde_json::from_str(&input))
    {
        Ok(Ok(call)) => call,
        Ok(Err(_)) | Err(_) => return ExitCode::FAILURE,
    };
    let root = std::env::current_dir()
        .ok()
        .filter(|dir| dir.join("architecture.toml").is_file())
        .unwrap_or_else(|| built_root.to_path_buf());
    #[expect(
        clippy::disallowed_methods,
        reason = "the approval lives in the environment that starts the session (ADR-0005)"
    )]
    let approved = std::env::var(APPROVAL_ENV).is_ok_and(|v| v == "1");
    let decision = if event == "pre-edit" {
        edit(&root, &call, approved)
    } else {
        let root = slashed(&root);
        let cwd = if call.cwd.is_empty() {
            root.clone()
        } else {
            call.cwd.replace('\\', "/")
        };
        check_command(&call.tool_input.command, approved, &Repo { root: &root, cwd: &cwd })
    };
    let Some(reason) = decision else {
        return ExitCode::SUCCESS;
    };
    // The reason is what the agent reads; if stderr is gone there is no one to tell.
    writeln!(io::stderr(), "{reason}").ok();
    ExitCode::from(BLOCK)
}

fn edit(root: &Path, call: &Call, approved: bool) -> Option<String> {
    let input = &call.tool_input;
    let target = if input.file_path.is_empty() {
        &input.notebook_path
    } else {
        &input.file_path
    };
    if target.is_empty() {
        return None;
    }
    let cwd = if call.cwd.is_empty() {
        root.to_path_buf()
    } else {
        PathBuf::from(&call.cwd)
    };
    let rel = relative(root, &cwd.join(target))?;
    // "Protected" means committed: a rule in HEAD was accepted; one still being written this session was not.
    let committed = |rel: &str| {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["ls-tree", "-r", "--name-only", "HEAD"])
            .output()
            .is_ok_and(|o| o.status.success() && listed(&String::from_utf8_lossy(&o.stdout), rel))
    };
    check_edit(&rel, approved, committed)
}

/// A file inside the repository as a path relative to its root; `None` outside it, or for the root itself.
pub(crate) fn relative(root: &Path, path: &Path) -> Option<String> {
    let root = slashed(root);
    repo_relative(&root, &root, &slashed(path)).filter(|rel| !rel.is_empty())
}

/// Whether `head` (the output of `git ls-tree -r --name-only HEAD`) lists `rel`, ignoring case as Windows does.
pub(crate) fn listed(head: &str, rel: &str) -> bool {
    head.lines().any(|line| line.eq_ignore_ascii_case(rel))
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests;
