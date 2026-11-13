# 0005 - Agent guard

- **Status**: accepted
- **Date**: 2026-11-12

## Context

The gates only mean something if the agent they judge cannot quietly change them. The easiest way to make a failing build pass is to loosen the rule, delete the fixture, skip the hook, or run a command that never returns.

## Decision

1. **Claude Code runs `cargo xtask hook` before every edit and shell command** (`.claude/settings.json`). Exit 2 blocks the call and the reason on stderr goes to the agent; any other failure exits 1 and lets the call through, so a broken guard cannot brick a session.
2. **Protected means committed.** The root configs, `.github/`, `.claude/settings.json`, `xtask/src/gates`, `fixtures/`, and the license are protected once they are in HEAD. A new file is always allowed, so rules can be added freely and weakening a committed one is deliberate.
3. **The shell is checked too**: hook bypasses (including `export` and `env` forms), shell writes to a protected path, and commands that never return (`cargo test` without a per-test timeout, a service in the foreground, `cargo watch`, followed logs) or move too much at once (a bare `cargo update`). A path is resolved before it is judged: against the session's directory, through `..`, absolute or relative, in any case; a directory that holds a protected file and a glob that could reach one count too. A script handed to a shell (`sh -c '...'`) and a subshell are checked as commands. Quoted prose and heredoc bodies are not, and a target the guard cannot know (a variable) is let through.
4. **The user approves a protected change by starting the session with `XTASK_ALLOW_PROTECTED=1`.** It lifts only the protected-file rule.
5. **The hook runs a prebuilt copy** in `.xtask/bin`, refreshed by every `verify`, so a hook starts in milliseconds and a rebuild never fights a running binary. Without it the hook falls back to `cargo run`, which keeps the exit code.
6. **It is a guard rail, not a security boundary.** `CODEOWNERS` over the same paths, the git hooks, and CI are the boundary.

## Consequences

- Positive: weakening a rule, skipping a gate, or hanging a session is stopped when it is attempted, with the fix in the message.
- Negative: adding a dependency means editing the protected root `Cargo.toml`, so it needs the approval variable. That is intended: a dependency is a decision.
- Neutral: other agents do not run these hooks; the git hooks and CI still apply to them.

## Alternatives considered

- **Protect every file ever created**: blocks finishing a new gate in the session that writes it.
- **Fail closed when the guard breaks**: a typo in the guard would block every tool call.
