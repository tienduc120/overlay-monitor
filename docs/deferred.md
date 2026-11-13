# Deferred work

Left out on purpose until a real project needs it. Each entry says what triggers it and what must be true before it counts as done. Do not start one without its trigger, and do not report it as enforced until a tool in this repository checks it. Every item follows the repository's own rules: an ADR first, tests first, a fixture for every new rule, a credit in the README for any public source, and a pinned tool in `cargo xtask doctor`.

| Item | Trigger | Done when |
| --- | --- | --- |
| Bounded command runner | An agent session hangs on a command the guard does not know | `cargo xtask run -- <cmd>` with a deadline, an idle limit, a process-tree kill, and the output in `.xtask/logs`; the guard points hanging commands at it |
| Debt register | The first `#[expect]` that is not a permanent design choice | `cargo xtask debt` lists every `#[expect]`, untracked TODO, and function near the size limits with a stable id; decisions with an expiry live in `docs/debt/register.json`; `verify` fails on new undecided debt |
| Perf loop | The first performance problem | An ADR on the tools (pprof-rs and jemalloc heap profiles are Linux only; samply works everywhere); a load program |
| ADR and Markdown checks | The ADR index or the Markdown drifts | `cargo xtask` checks ADR numbering, sections, and the index, and one paragraph per line in Markdown |
| Toolchain 1.100 | Rust 1.100 reaches the stable channel | `rust-toolchain.toml`, `rust-version`, and `doctor` move together; `[lints.cargo] unused_dependencies` replaces cargo-shear if it proves equal |
| Overlay presentation fallback | A PresentMon run where a borderless game reaches `Hardware: Independent Flip` with no overlay and drops to `Composed: Flip` with the layered overlay shown (ADR-0016 could not test this: the owner's game was composed with or without it) | A new ADR supersedes ADR-0016 with the measurement and picks the fix: a composition swap chain, or moving the overlay to the other monitor while a game is in front |
| Restart a stopped ETW session | Plan 2's diagnostics | An ETW session stopped from outside is started again, so FPS does not stay "—" |
| Readings that need an extra query | The owner asks for one | Each comes with its own query and an ADR: VRAM used (`D3DKMTQueryStatistics` or the `GPU Adapter Memory` counters), per-core CPU load and CPU clocks (`NtQuerySystemInformation` or the `Processor Information` counters), Zen 4 chiplet temperatures (SMN `0x00059B08` + 4 × chiplet), GPU core clock and hotspot (not in the perf data), whether the fan reports at all (`KMTQAITYPE_ADAPTERPERFDATA_CAPS`, which would let 0 rpm show at once) |
