# Overlay Monitor

A Windows-only Rust desktop overlay that shows FPS and CPU, GPU, and DDR5 load and temperature over the desktop and borderless games. It is built on a copy of the owner's strict rust-base, so it keeps rules a tool checks, a fixture that proves each rule fires, skills an agent follows, and one command surface.

The decisions behind it: [docs/adr/](docs/adr/README.md). What is deliberately not built yet, and what triggers it: [docs/deferred.md](docs/deferred.md). Do not report a rule as enforced unless a tool in this repo checks it.

## How to work

- **Red, then green.** Write the failing test, run that crate's tests and watch it go red for the right reason, make it pass, run them again: `cargo nextest run -p <crate>`. Between steps that is the only check you run.
- **The hooks run the gates.** The pre-commit hook runs `cargo xtask verify`, the pre-push hook and CI run `--full`, and a rejected commit names exactly what to fix. Run `verify` by hand only to read a failure in full (the logs are in `.xtask/logs`).
- **Write Rust-shaped code**, idiomatic to Rust rather than ported line by line from another language. For an idiom you are unsure of, read the [Microsoft Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/agents/all.txt).

## Commands

Run from the repository root. There is no Make; `cargo xtask` is the command surface.

| Need | Command |
| --- | --- |
| Tests (the TDD loop) | `cargo nextest run -p <crate>` (never `cargo test`: it has no per-test timeout; nextest kills a test after two minutes) |
| Run the gates (hooks and CI do this) | `cargo xtask verify`; `--full` adds cargo-deny, unused dependencies, docs, and the rules proof |
| Prove every rule fires on its fixture | `cargo xtask rules` |
| Check the toolchain and pinned tools | `cargo xtask doctor` (prints the install command for anything missing) |
| Install the git hooks (once per clone) | `lefthook install` |
| Start a new service | `cargo xtask new-service <name> --aggregate <Name>` (skill: `new-service`) |
| Check the PawnIO modules (added in Task 6) | `cargo xtask modules` |
| Check a commit message | `cargo xtask commitmsg <file>` (the commit-msg hook calls it) |
| Format | `cargo fmt --all` |
| Make the skills visible to Claude Code (once per clone) | Windows: `cmd /c mklink /J .claude\skills .agents\skills`; Unix: `ln -s ../.agents/skills .claude/skills` |

## Hard rules

- **Test first.** No production code without a test that failed first, for the right reason. Unit tests live in a sibling file (`#[cfg(test)] mod tests;` next to `<module>/tests.rs`), never inline. Follow the `writing-rust-tests` skill.
- **Start a service with the generator.** `cargo xtask new-service` copies the witness `services/example` under a new name, so it starts green. Change `services/example` only when every future service should change, and keep it passing every gate.
- **Errors are typed values owned by the layer that raises them.** `Result<T, E>` with a `thiserror` enum per operation; never panic for an expected failure; keep the cause as `#[source]`; only `interfaces` maps an outcome to a status, with an exhaustive `match`. `anyhow` is for the tool only. Follow the `handling-errors` skill (ADR-0011).
- **Threads only through `std::thread::scope`; no async runtime (ADR-0014).** Serving only through `platform-http`, time only through `platform-clock`. clippy refuses `tokio::spawn`, `std::thread::spawn`, `std::thread::sleep`, `axum::serve`, `SystemTime::now`, `CancellationToken::new`, `std::env::var`, and `std::process::exit` everywhere else (ADR-0009).
- **`unsafe` only in `platform-win32` (ADR-0012).** Wrap each call in a safe function with a `// SAFETY:` comment; the `source-unsafe-scope` rule refuses an `unsafe_code` expectation anywhere else.
- **Suppress a lint only with `#[expect(lint, reason = "...")]` on the one item that needs it.** `#[allow]` and crate-wide `#![allow]` are refused. A suppression is debt: say so to the user.
- **A new dependency is a decision, not an edit.** Versions live once in the root `Cargo.toml` (`[workspace.dependencies]`), which is protected; a member writes `{ workspace = true }`. Propose it with an ADR, and credit it in the README.
- **Never weaken a gate to make a change pass.** `Cargo.toml`, `clippy.toml`, `architecture.toml`, `deny.toml`, the other root configs, `xtask/src/gates`, and `fixtures/` are the rules that judge you: change them only with the user's explicit approval. Fix the cause instead.
- **Every rule has a fixture.** A new or changed rule gets a violation fixture in `fixtures/violations/<id>/` with a `rule.toml` naming its enforcer (`clippy` or `gates`) and the text it must report, and `cargo xtask rules` must stay green. The clean twin in `fixtures/clean/` must stay free of findings.
- **Never bypass git hooks** (`--no-verify`, `LEFTHOOK=0`). `lefthook.yml` runs `verify` before each commit, `commitmsg` on the message, and `verify --full` before a push.
- **A `refactor:` commit may only add tests**, never change an existing test line (the commit-msg hook enforces it): a changed expectation means behaviour changed, so it is a `fix:` or `feat:`.
- **Evidence, not claims.** "Tests pass" means you ran them and saw the result. Name every step you skipped and why.
- **Comments say why**, never what, and stay short. Conventional Commits: `type(scope): subject`, imperative, lowercase, at most 72 characters.
- **Credit what you reference.** A design, config, or code from a public source goes in the README "Credits" in the same change; copied or adapted material also needs its license and a `NOTICE.md` entry. Only MIT, BSD, and Apache-2.0 material comes in.
- **Do not commit `.local/`** (gitignored notes) and do not install software system-wide without asking.

## Architecture

One bounded context per service under `services/<name>/`, one crate per layer. A dependency that crosses a layer the wrong way is a missing Cargo edge, so it does not compile; `cargo xtask verify` also checks every edge against `architecture.toml`:

| Crate | May depend on | External crates |
| --- | --- | --- |
| `<svc>-domain` | nothing of ours | only `thiserror`, `jiff`; the crate is `#![no_std]`, so the compiler keeps I/O, threads, env, and the clock out (ADR-0006) |
| `<svc>-application` | its domain, `platform-clock` | only `thiserror`, `tracing`, `jiff` |
| `<svc>-infrastructure` | its domain and application, any platform | not `axum`, `tower-http`, `hyper`, `anyhow` |
| `<svc>-interfaces` | its application, any platform | not `sqlx`, `anyhow` |
| `<svc>-api` | its own layers, any platform | not `anyhow` |
| `platform-<name>` | other platform crates, never a service | not `anyhow` |
| `platform-win32` | other platform crates | `windows`; the only crate with `unsafe` |

- A service never depends on another service's crates. Nothing depends on `xtask`.
- Ports are traits declared in `application` with plain `fn` methods, not futures (ADR-0014). Adapters are chosen in the composition root; use cases are generic over their ports (ADR-0007).
- An aggregate root carries `agent:aggregate` in its doc comment, has a field `id` of a type its crate defines, `new` (checks invariants), and `restore` (rebuilds a stored value). Only infrastructure calls `restore` outside tests (ADR-0007).
- A use case takes a command of plain values and returns its own result and error types; delivery never handles a domain type. Fakes of a crate's ports live in its `testing` module behind the `test-support` feature, so other layers' tests can use them.

Enforced, each with a fixture:

- **Layer rules** (`cargo xtask verify`, `architecture.toml`): the table above, `#![no_std]` in the domain, and every crate under `services/<svc>/<layer>`, `platform/<name>`, or `xtask`.
- **Manifest policy**: `[lints] workspace = true`, package fields inherited from the workspace, the package named after where it lives, external versions only from the workspace.
- **Source rules** (syn): aggregate shape, `restore` only in infrastructure, no `extern crate std` in a `no_std` crate, no inline test modules, no crate-wide `#![allow]`.
- **Lints** (`Cargo.toml`, `clippy.toml`): clippy `all` and `pedantic` at deny, plus picked restriction lints: no `unwrap`/`expect`/`panic`/`todo`/indexing outside tests, no `println!`, no `mod.rs`, no wildcard arm on an enum, no `#[allow]`, no `unsafe`, functions under 80 lines, no lock guard across an `.await`, and the bans listed under Hard rules.
- **Supply chain** (`deny.toml`, in `verify --full`): permissive licenses only, advisories, crates.io only, and banned crates (`anyhow` outside the tool, `async-trait`, `openssl`, deprecated YAML crates).
- **Tests**: nextest fails a test after two minutes and fails one that leaks a child process.

## Agent guard

In Claude Code, `.claude/settings.json` runs `cargo xtask hook` before every edit and shell command and refuses, with a reason you can act on:

- changing a committed rule file (the root configs, `.github/`, `.claude/settings.json`, `xtask/src/gates`, `fixtures/`, `LICENSE`). New files are always allowed, so adding a fixture or a gate is fine; protection starts once a file is committed.
- skipping the git hooks: `--no-verify`, `git commit -n`, `LEFTHOOK=0`, `core.hooksPath`, `lefthook uninstall`.
- writing, moving, or deleting a protected path through the shell.
- commands that never return or move too much at once: `cargo test` (use nextest), `cargo run` of an `-api` or `-worker` binary in the foreground, `cargo watch`, a bare `cargo update`, `docker compose up` without `-d`, following logs, `tail -f`, `Get-Content -Wait`.

When you are blocked, fix the cause; do not look for another way to make the same change. If the rule really should change, ask the user: they approve by starting the session with `XTASK_ALLOW_PROTECTED=1`, which lifts only the protected-file block. The guard is a guard rail, not a security boundary: `CODEOWNERS`, the git hooks, and CI are the boundary, and other agents do not run these hooks (ADR-0005).

## Conventions not enforced by a tool

- Markdown: one paragraph or list item per line; never hard-wrap prose.
- A name says what the thing is for in the domain's words; a function does one thing at one level.

## Things that will surprise you

Grow this by correction: when any tool gets something wrong, add the line.

- **The guard judges paths from the session's directory.** `cd elsewhere && rm Cargo.toml` is judged as if `Cargo.toml` were the repository's, which blocks more, never less. Shell writes to a protected path are refused even before it is committed; write a gate you are still building with the edit tools.
- **clippy reports the resolved path of a banned call** (`tokio::task::spawn`, not `tokio::spawn`). A fixture's expected text must name the resolved path.
- **A test in one layer cannot name another layer's types it may not depend on.** Use the fakes from that layer's `testing` module (`features = ["test-support"]` on the dev-dependency).
- **`doc_markdown` wants backticks** around identifiers in doc comments, such as `no_std` or `CreateNote`.
- **Cargo treats a `members` glob that matches nothing as a literal path** and fails to load the workspace.
- **Tests that need admin are `#[ignore]`.** The owner runs them with `--run-ignored only` in an elevated terminal.
- **The agent's shell cannot elevate.**
- **Launch the app with `Start-Process`**, because the guard refuses `cargo run` of an `-api` binary in the foreground.
- **On Windows, a tool installed mid-session is not on the shell's PATH.** In PowerShell, reload it: `$env:Path = [Environment]::GetEnvironmentVariable('Path','User') + ';' + [Environment]::GetEnvironmentVariable('Path','Machine')`. In Git Bash use `/c/Users/...` paths, not `$USERPROFILE`.
