//! Commit messages: Conventional Commits as this repository uses them, and a refactor that only adds tests.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result};

/// `cargo xtask commitmsg <file> [--commit <sha>]`. The commit-msg hook judges the staged diff; CI passes each pull
/// request commit with `--commit`, so a refactor is judged by that commit's own diff.
pub(crate) fn run(root: &Path, file: &Path, commit: Option<&str>, err: &mut impl Write) -> Result<bool> {
    let msg = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
    let mut problems = validate(&msg);
    if is_refactor(&msg) {
        let args: &[&str] = match commit {
            Some(sha) => &["show", "-U0", "--format=", sha],
            None => &["diff", "--cached", "-U0"],
        };
        match Command::new("git").arg("-C").arg(root).args(args).output() {
            Ok(o) if o.status.success() => problems.extend(refactor_problems(&String::from_utf8_lossy(&o.stdout))),
            Ok(o) => problems.push(format!(
                "cannot read the diff to check the refactor: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => problems.push(format!("cannot run git to check the refactor: {e}")),
        }
    }
    for p in &problems {
        writeln!(err, "commit message: {p}")?;
    }
    Ok(problems.is_empty())
}

/// Whether the message's type is `refactor`, with or without a scope or a breaking mark.
pub(crate) fn is_refactor(msg: &str) -> bool {
    content_lines(msg)
        .first()
        .and_then(|line| line.split_once(':'))
        .is_some_and(|(prefix, _)| {
            let kind = prefix.strip_suffix('!').unwrap_or(prefix);
            kind.split_once('(').map_or(kind, |(kind, _)| kind) == "refactor"
        })
}

/// The longest first line git tooling and terminals show without wrapping.
const MAX_HEADER: usize = 72;
const TYPES: [&str; 11] = [
    "feat", "fix", "perf", "refactor", "test", "docs", "build", "ci", "chore", "revert", "style",
];

/// Every problem with `msg`, so one pass tells the author everything to fix. Empty means valid.
pub(crate) fn validate(msg: &str) -> Vec<String> {
    let lines = content_lines(msg);
    let Some(first) = lines.first() else {
        return vec!["the message is empty".to_owned()];
    };
    if first.starts_with("Merge ") || first.starts_with("Revert \"") {
        return Vec::new();
    }
    let mut problems = Vec::new();
    let length = first.chars().count();
    if length > MAX_HEADER {
        problems.push(format!(
            "the first line is {length} characters; the limit is {MAX_HEADER}"
        ));
    }
    let Some((prefix, subject)) = first.split_once(": ") else {
        problems.push("the first line must read `type(scope): subject` with a space after the colon".to_owned());
        return problems;
    };
    let prefix = prefix.strip_suffix('!').unwrap_or(prefix);
    let (kind, scope) = match prefix.split_once('(') {
        Some((kind, rest)) => (kind, Some(rest.strip_suffix(')').unwrap_or("("))),
        None => (prefix, None),
    };
    if !TYPES.contains(&kind) {
        problems.push(format!("type `{kind}` is not one of: {}", TYPES.join(", ")));
    }
    if scope.is_some_and(|s| !is_kebab(s)) {
        problems.push("the scope must be lowercase kebab-case and not empty, like `xtask` or `billing-api`".to_owned());
    }
    if subject.chars().next().is_some_and(char::is_uppercase) {
        problems.push("the subject must start lowercase".to_owned());
    }
    if subject.ends_with('.') {
        problems.push("the subject must not end with a period".to_owned());
    }
    if lines.get(1).is_some_and(|l| !l.is_empty()) {
        problems.push("leave a blank line between the first line and the body".to_owned());
    }
    problems
}

/// Problems with a `refactor:` commit's staged diff (`git diff --cached -U0`): a refactor keeps behaviour, so an
/// existing test line it removes or changes means the commit is really a fix or a feature.
pub(crate) fn refactor_problems(diff: &str) -> Vec<String> {
    let mut file = "";
    let mut problems = Vec::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("--- ").or_else(|| line.strip_prefix("+++ ")) {
            if path != "/dev/null" {
                file = path
                    .strip_prefix("a/")
                    .or_else(|| path.strip_prefix("b/"))
                    .unwrap_or(path);
            }
            continue;
        }
        let Some(removed) = line.strip_prefix('-') else {
            continue;
        };
        let code = removed.trim();
        if is_test_path(file) && !code.is_empty() && !code.starts_with("//") {
            problems.push(format!(
                "{file}: this refactor changes the existing test line `{code}`; a refactor may only add tests \
                 (if behaviour changed, the commit is a fix or a feat)"
            ));
        }
    }
    problems
}

fn is_test_path(path: &str) -> bool {
    path.ends_with("/tests.rs") || path.contains("/tests/")
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Drops git's comment lines and trailing blanks.
fn content_lines(msg: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = msg.lines().filter(|l| !l.starts_with('#')).map(str::trim_end).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests;
