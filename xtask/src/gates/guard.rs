//! Decides whether an agent's edit or shell command may go ahead (ADR-0005). A guard rail against mistakes and
//! shortcuts, not a security boundary: string matching cannot follow a variable or a script file, so CODEOWNERS, the
//! git hooks, and CI repeat the real checks.

/// Set by the person, in the environment that starts the session, to approve changing a protected file.
pub(crate) const APPROVAL_ENV: &str = "XTASK_ALLOW_PROTECTED";

const JUDGE: &str = "a rule that judges your work";
const FIXTURE: &str = "a fixture that proves a rule fires";
const LEGAL: &str = "legal text";

/// Protected once committed: adding a new fixture or gate adds rules; changing a committed one weakens them.
const PROTECTED: [(&str, &str); 18] = [
    ("Cargo.toml", JUDGE),
    ("rust-toolchain.toml", JUDGE),
    ("clippy.toml", JUDGE),
    ("rustfmt.toml", JUDGE),
    ("deny.toml", JUDGE),
    ("architecture.toml", JUDGE),
    ("typos.toml", JUDGE),
    (".config/nextest.toml", JUDGE),
    (".cargo/config.toml", JUDGE),
    ("lefthook.yml", JUDGE),
    (".github/workflows/**", JUDGE),
    (".github/CODEOWNERS", JUDGE),
    (".claude/settings.json", JUDGE),
    ("xtask/src/gates.rs", JUDGE),
    ("xtask/src/gates/**", JUDGE),
    ("fixtures/**", FIXTURE),
    ("LICENSE", LEGAL),
    ("LICENSES/**", LEGAL),
];

const MUTATORS: [&str; 19] = [
    "rm",
    "mv",
    "cp",
    "tee",
    "truncate",
    "dd",
    "del",
    "erase",
    "rmdir",
    "ln",
    "set-content",
    "add-content",
    "out-file",
    "remove-item",
    "move-item",
    "copy-item",
    "rename-item",
    "clear-content",
    "new-item",
];

/// Words that only set up the environment of the command after them.
const PREFIX_WORDS: [&str; 3] = ["export", "env", "set"];

/// The flags that hand a quoted script to a shell: `sh -c`, `pwsh -Command`, `cmd /c`.
const SCRIPT_FLAGS: [&str; 3] = ["-c", "-command", "/c"];

/// Where a command runs: its paths resolve against `cwd` and are protected relative to `root`.
pub(crate) struct Repo<'a> {
    pub(crate) root: &'a str,
    pub(crate) cwd: &'a str,
}

fn approval_hint() -> String {
    format!("If the user wants it changed, they approve by starting the session with {APPROVAL_ENV}=1.")
}

/// `rel` is relative to the repository root. `committed` is asked only for a protected path, since it costs a git
/// call.
pub(crate) fn check_edit(rel: &str, approved: bool, committed: impl Fn(&str) -> bool) -> Option<String> {
    if approved {
        return None;
    }
    let what = protection(rel)?;
    if !committed(rel) {
        return None;
    }
    Some(format!(
        "BLOCKED: {rel} is {what}. Fix the cause instead of changing it; adding new files is allowed. {}",
        approval_hint()
    ))
}

/// `None` lets the command run; `Some(reason)` blocks it. `approved` lifts only the protected-file rule: approval to
/// change a rule is not approval to skip the gates or to hang the session.
pub(crate) fn check_command(cmd: &str, approved: bool, repo: &Repo<'_>) -> Option<String> {
    let cmd = strip_heredocs(cmd);
    if let Some(found) = shell_scripts(&cmd)
        .iter()
        .find_map(|script| check_command(script, approved, repo))
    {
        return Some(found);
    }
    let masked = mask(&cmd).replace(">|", ">");
    let joined = masked.replace("&&", "\n").replace("||", "\n").replace("$(", "\n");
    for segment in joined.split(['\n', ';', '|', '(', ')', '{', '}', '`']) {
        let tokens: Vec<&str> = segment.split_whitespace().collect();
        let words = tokens.iter().take_while(|t| PREFIX_WORDS.contains(t)).count();
        let tokens = tokens.get(words..).unwrap_or_default();
        let (env, command) = tokens.split_at(tokens.iter().take_while(|t| is_env_assignment(t)).count());
        let found = bypasses_hooks(env, command)
            .or_else(|| {
                if approved {
                    None
                } else {
                    changes_protected(command, repo)
                }
            })
            .or_else(|| never_returns(command));
        if found.is_some() {
            return found;
        }
    }
    None
}

/// `path` relative to the repository root, with forward slashes, after resolving `.` and `..` against `cwd`.
/// The root itself, or a directory above it, is `Some("")`; a path outside the repository is `None`. Comparison
/// ignores case, since on Windows the same file arrives as `d:\..` from one tool and `D:\..` from another.
pub(crate) fn repo_relative(root: &str, cwd: &str, path: &str) -> Option<String> {
    let path = path.trim_matches(['"', '\'']).replace('\\', "/");
    let joined = if is_absolute(&path) {
        path
    } else {
        format!("{cwd}/{path}")
    };
    let (abs, root) = (normalize(&joined), normalize(&root.replace('\\', "/")));
    let (abs_key, root_key) = (abs.to_ascii_lowercase(), root.to_ascii_lowercase());
    let abs_dir = if abs_key.ends_with('/') {
        abs_key.clone()
    } else {
        format!("{abs_key}/")
    };
    if abs_key == root_key || root_key.starts_with(&abs_dir) {
        return Some(String::new());
    }
    abs_key.strip_prefix(&format!("{root_key}/"))?;
    abs.get(root.len() + 1..).map(str::to_owned)
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || path.get(1..2) == Some(":")
}

/// Resolves `.` and `..` lexically and drops repeated and trailing slashes.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                // Never climb above a drive letter or the filesystem root.
                if parts.last().is_some_and(|p| !p.ends_with(':')) {
                    parts.pop();
                }
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if path.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

/// What protects `rel`, if anything: the path itself, a directory holding it, or a glob that could reach it. The
/// empty path is the repository root, which holds everything.
fn protection(rel: &str) -> Option<&'static str> {
    let rel = rel.to_ascii_lowercase();
    let glob = rel.find(['*', '?', '[']).and_then(|at| rel.get(..at));
    PROTECTED
        .iter()
        .find(|(pattern, _)| {
            let pattern = pattern.to_ascii_lowercase();
            let base = pattern.strip_suffix("/**").unwrap_or(&pattern);
            match glob {
                Some(prefix) => pattern.starts_with(prefix),
                None => rel.is_empty() || matches(&pattern, &rel) || base.starts_with(&format!("{rel}/")),
            }
        })
        .map(|(_, what)| *what)
}

fn matches(pattern: &str, rel: &str) -> bool {
    match pattern.strip_suffix("/**") {
        Some(dir) => rel == dir || rel.strip_prefix(dir).is_some_and(|rest| rest.starts_with('/')),
        None => rel == pattern,
    }
}

/// Drops heredoc bodies: a commit message that merely mentions a file or a flag is not running it.
fn strip_heredocs(cmd: &str) -> String {
    let mut out = Vec::new();
    let mut terminator: Option<String> = None;
    for line in cmd.lines() {
        if let Some(t) = &terminator {
            if line.trim() == t {
                terminator = None;
            }
            continue;
        }
        out.push(line);
        terminator = heredoc_terminator(line);
    }
    out.join("\n")
}

/// The word that ends a heredoc opened on `line`. `<<` must follow a space and the word must start with a letter, so
/// a shift such as `$((1<<2))` never hides the lines after it. A heredoc this misses has its body checked as commands,
/// which only blocks more.
fn heredoc_terminator(line: &str) -> Option<String> {
    let at = line.find("<<")?;
    let after_space = line
        .get(..at)
        .is_some_and(|before| before.ends_with(char::is_whitespace));
    let rest = line
        .get(at + 2..)?
        .trim_start_matches('-')
        .trim_start()
        .trim_start_matches(['\'', '"']);
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (after_space && word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')).then_some(word)
}

/// The quoted scripts handed to a shell (`sh -c '...'`): each is a command line of its own.
fn shell_scripts(cmd: &str) -> Vec<String> {
    let mut scripts = Vec::new();
    let (mut word, mut last_word) = (String::new(), String::new());
    let mut chars = cmd.chars();
    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let inner: String = chars.by_ref().take_while(|d| *d != c).collect();
            if SCRIPT_FLAGS.contains(&last_word.to_ascii_lowercase().as_str()) {
                scripts.push(inner);
            }
            word.clear();
            last_word.clear();
        } else if c.is_whitespace() {
            if !word.is_empty() {
                last_word = std::mem::take(&mut word);
            }
        } else {
            word.push(c);
        }
    }
    scripts
}

/// Keeps simple quoted words, such as paths, and blanks quoted prose, such as commit messages.
fn mask(cmd: &str) -> String {
    let mut out = String::with_capacity(cmd.len());
    let mut chars = cmd.chars();
    while let Some(c) = chars.next() {
        if c != '"' && c != '\'' {
            out.push(c);
            continue;
        }
        let inner: String = chars.by_ref().take_while(|d| *d != c).collect();
        let simple = inner
            .chars()
            .all(|d| d.is_ascii_alphanumeric() || "./:+@=-_\\".contains(d));
        out.push_str(if simple { &inner } else { "\"\"" });
    }
    out
}

fn is_env_assignment(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn bypasses_hooks(env: &[&str], command: &[&str]) -> Option<String> {
    let reason =
        || "BLOCKED: this skips the git hooks that run the gates. Fix what the hook reports instead.".to_owned();
    let env_off = env
        .iter()
        .filter_map(|e| e.split_once('='))
        .any(|(k, v)| ((k == "LEFTHOOK" || k == "HUSKY") && v == "0") || k == "LEFTHOOK_EXCLUDE");
    let first = command.first().map(|f| f.to_ascii_lowercase()).unwrap_or_default();
    let ps_off = first.starts_with("$env:lefthook") || first.starts_with("$env:husky");
    let uninstall = first == "lefthook" && command.get(1) == Some(&"uninstall");
    let git = first == "git"
        && command.iter().any(|t| {
            t.contains("core.hooksPath")
                || *t == "--no-verify"
                || (command.contains(&"commit") && t.starts_with('-') && !t.starts_with("--") && t.contains('n'))
        });
    (env_off || ps_off || uninstall || git).then(reason)
}

fn changes_protected(command: &[&str], repo: &Repo<'_>) -> Option<String> {
    let mut targets = redirect_targets(command);
    if mutates(command) {
        targets.extend(command.iter().skip(1).filter(|t| !t.starts_with('-')));
    }
    // A blanked or variable target ("", "$OUT") names no file the guard can know.
    let knowable = |t: &&str| {
        let t = t.trim_matches(['"', '\'']);
        !t.is_empty() && !t.starts_with('$')
    };
    let hit = targets
        .into_iter()
        .filter(knowable)
        .find(|t| repo_relative(repo.root, repo.cwd, t).is_some_and(|rel| protection(&rel).is_some()))?;
    Some(format!(
        "BLOCKED: this changes a protected file ({hit}) through the shell. Rule files, gate code, fixtures, and \
         licenses are protected; fix the cause instead. {}",
        approval_hint()
    ))
}

/// The files a command writes through a redirect: `> f`, `>> f`, `1>f`, `2>>f`, `&>f`, `*>f`. `2>&1` names a stream.
fn redirect_targets<'a>(command: &[&'a str]) -> Vec<&'a str> {
    let mut targets = Vec::new();
    for (i, token) in command.iter().enumerate() {
        let token = token.trim_start_matches(|c: char| c.is_ascii_digit() || c == '&' || c == '*');
        let Some(rest) = token.strip_prefix(">>").or_else(|| token.strip_prefix('>')) else {
            continue;
        };
        let rest = rest.trim_start_matches('&');
        let target = if rest.is_empty() {
            command.get(i + 1).copied()
        } else {
            Some(rest)
        };
        targets.extend(target.filter(|t| !t.chars().all(|c| c.is_ascii_digit())));
    }
    targets
}

fn mutates(command: &[&str]) -> bool {
    let Some(name) = command.first().map(|n| n.to_ascii_lowercase()) else {
        return false;
    };
    let rest = command.get(1..).unwrap_or_default();
    MUTATORS.contains(&name.as_str())
        || (name == "sed"
            && rest
                .iter()
                .any(|t| t.starts_with('-') && !t.starts_with("--") && t.contains('i')))
        || (name == "git" && matches!(rest.first(), Some(&("rm" | "mv"))))
}

fn never_returns(command: &[&str]) -> Option<String> {
    let name = command.first()?.to_ascii_lowercase();
    let has = |flag: &str| command.iter().any(|t| t.eq_ignore_ascii_case(flag));
    let follows = has("-f") || has("--follow");
    // `cargo +nightly -q test` still runs `test`.
    let sub = if name == "cargo" {
        command.iter().skip(1).find(|t| !t.starts_with(['+', '-'])).copied()
    } else {
        command.get(1).copied()
    };
    let reason = match (name.as_str(), sub) {
        ("cargo", Some("test")) if !has("--doc") => {
            "`cargo test` has no per-test timeout, so one hung test hangs the session. Run `cargo nextest run`: it \
             fails a test after two minutes (.config/nextest.toml)."
        }
        ("cargo", Some("run")) if runs_service(command) => {
            "a service runs until it is stopped, which hangs the session. Its tests start it on port 0; to run it \
             by hand, start it in the background."
        }
        ("cargo", Some("watch")) => "`cargo watch` never returns. Run the one command you need.",
        ("cargo", Some("update")) if !(has("-p") || has("--package") || has("--precise")) => {
            "`cargo update` moves every dependency at once. Update one: `cargo update -p <crate> --precise <version>`, \
             and say why."
        }
        ("docker" | "docker-compose", _) if has("up") && !(has("-d") || has("--detach")) => {
            "`docker compose up` in the foreground never returns. Use `-d`, then `docker compose logs --tail 100`."
        }
        ("docker" | "docker-compose" | "kubectl", _) if has("logs") && follows => {
            "following logs never returns. Read a bounded slice: `logs --tail 100`."
        }
        ("tail", _) if has("--follow") || command.iter().any(|t| t.starts_with('-') && t.contains(['f', 'F'])) => {
            "`tail -f` never returns. Read a bounded slice: `tail -n 100 <file>`."
        }
        ("get-content" | "gc" | "cat", _) if has("-wait") => {
            "`Get-Content -Wait` never returns. Read a bounded slice: `Get-Content <file> -Tail 100`."
        }
        _ => return None,
    };
    Some(format!("BLOCKED: {reason}"))
}

/// A `-api` or `-worker` binary is a server: it runs until stopped.
fn runs_service(command: &[&str]) -> bool {
    let named = command.windows(2).filter_map(|w| match w {
        [flag, name] if matches!(*flag, "-p" | "--package" | "--bin") => Some(*name),
        _ => None,
    });
    let inline = command.iter().filter_map(|t| {
        t.strip_prefix("--package=")
            .or_else(|| t.strip_prefix("--bin="))
            .or_else(|| t.strip_prefix("-p="))
    });
    named
        .chain(inline)
        .any(|n| n.ends_with("-api") || n.ends_with("-worker"))
}

#[cfg(test)]
mod tests;
