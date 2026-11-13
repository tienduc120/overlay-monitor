# Deferred work

Left out on purpose until a real project needs it. Each entry says what triggers it and what must be true before it counts as done. Do not start one without its trigger, and do not report it as enforced until a tool in this repository checks it. Every item follows the repository's own rules: an ADR first, tests first, a fixture for every new rule, a credit in the README for any public source, and a pinned tool in `cargo xtask doctor`.

| Item | Trigger | Done when |
| --- | --- | --- |
| Bounded command runner | An agent session hangs on a command the guard does not know | `cargo xtask run -- <cmd>` with a deadline, an idle limit, a process-tree kill, and the output in `.xtask/logs`; the guard points hanging commands at it |
| Debt register | The first `#[expect]` that is not a permanent design choice | `cargo xtask debt` lists every `#[expect]`, untracked TODO, and function near the size limits with a stable id; decisions with an expiry live in `docs/debt/register.json`; `verify` fails on new undecided debt |
| Perf loop | The first performance problem | An ADR on the tools (pprof-rs and jemalloc heap profiles are Linux only; samply works everywhere); a load program |
| ADR and Markdown checks | The ADR index or the Markdown drifts | `cargo xtask` checks ADR numbering, sections, and the index, and one paragraph per line in Markdown |
| Toolchain 1.100 | Rust 1.100 reaches the stable channel | `rust-toolchain.toml`, `rust-version`, and `doctor` move together; `[lints.cargo] unused_dependencies` replaces cargo-shear if it proves equal |
