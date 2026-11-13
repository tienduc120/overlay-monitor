# Overlay Monitor

A Windows app that draws a small hardware readout (FPS, CPU, GPU, and DDR5 load and temperature) over the desktop and borderless games. It replaces HWiNFO, RTSS, and RTSS's DesktopOverlayHost with one program.

It is built on a copy of the owner's strict `rust-base`, so its rules are checked by tools: lint, layer, manifest, and source rules, a fixture that proves every rule fires, an agent guard, and skills an agent follows.

Read [AGENTS.md](AGENTS.md) first, then [docs/adr/](docs/adr/README.md).

## Build and run

Overlay Monitor runs on Windows 10 1809 or later, x64.

- The CPU and DDR5 temperatures need the PawnIO driver installed ([pawnio.eu](https://pawnio.eu), version 2.1.0 or later) and the app running as administrator.
- FPS needs administrator or membership in the "Performance Log Users" group.
- GPU load and temperature and CPU and memory load need neither.
- `cargo xtask modules` downloads the pinned PawnIO modules, checks their hashes, and copies them into `target/{debug,release}/modules`. They are LGPL-2.1 and never committed; see [Credits](#credits).
- `cargo build -p monitor-api --release --features elevated` builds `target\release\overlay-monitor.exe`, whose manifest asks for administrator.
- Without `--features elevated` the exe runs as the invoking user, which is the default for development and tests; the CPU and DDR5 temperatures then show "—".
- Start with Windows: when the setting is on, the app creates a Task Scheduler task named "Overlay Monitor" (at sign-in, highest privileges, no time or battery limit) through `schtasks.exe`, and registers it again at each start so it follows the exe. It needs the elevated build, because creating a highest-privileges task needs administrator ([ADR-0020](docs/adr/0020-start-with-windows.md)).

A value that is missing or unreadable shows "—". The tray icon shows or hides the overlay, and its menu exits the app. Drag the overlay anywhere; it snaps to monitor edges.

- Lock position (in the overlay's or the tray's menu) makes clicks pass through the overlay, so it cannot be dragged or right-clicked; unlock it from the tray menu or with Ctrl+Shift+L.
- Ctrl+Shift+O shows or hides the overlay and Ctrl+Shift+L locks or unlocks it, from any program. If another program holds a combination, the app logs it and carries on without it; a saved lock is then dropped when Ctrl+Shift+L is the one that is taken.
- After either menu closes, the window you were in gets the focus back.

## Readings

Each line shows the values the settings turn on, in this order. A missing or stale value shows "—". Every value comes from a call the app already makes ([ADR-0019](docs/adr/0019-extra-readings.md)).

| Line | Value | Source | Default |
| --- | --- | --- | --- |
| FPS | FPS | graphics kernel `Present_Info` events (ETW) | on |
| FPS | frame time | `1000 / FPS` | off |
| FPS | worst frame | longest gap between two presents, from the same events | off |
| CPU | load | system times | on |
| CPU | temperature | PawnIO, from the CPU's sensor | on |
| GPU | load | `GPU Engine` counters, busiest engine type | on |
| GPU | temperature | D3DKMT adapter performance data | on |
| GPU | fan | the same performance data; "—" until the fan has turned once | off |
| GPU | VRAM clock | the same performance data | off |
| GPU | video | the `GPU Engine` counters, busiest video engine without JPEG | off |
| Memory | load | physical memory status | on |
| Memory | used | the same status, in GB | off |
| Memory | temperature | PawnIO, hottest DDR5 SPD hub | on |
| Memory | each stick | the same hubs, in address order | off |

## Settings

The app keeps its settings in `%APPDATA%\OverlayMonitor\settings.toml`, created on the first change. The app rewrites the file when a setting changes, so edit it while the app is closed. Every key is optional; a file that cannot be read is kept as `settings.toml.bad` and the defaults are used. The keys and the rules are in [ADR-0018](docs/adr/0018-the-settings-file.md).

## Quick start

You need rustup (it reads `rust-toolchain.toml`), Git, and on Windows the Visual Studio Build Tools with the C++ workload.

```sh
cargo xtask doctor          # checks the toolchain and the pinned tools, prints install commands
lefthook install            # runs the gates on every commit and push
cargo xtask verify          # every gate; --full adds cargo-deny, unused deps, docs, and the rules proof
cargo nextest run -p <crate>  # the test loop
```

There is no Make. `cargo xtask` is the single command surface.

## Layout

```text
services/monitor/{domain,application,infrastructure,interfaces,api}  the monitor service
platform/{clock,win32}                                              shared technical crates, no business logic; win32 holds all unsafe
xtask/                                                              gates, guard, tools; gate code in src/gates
fixtures/{violations/<rule>,clean}                                  one failing fixture per rule, one compliant twin
architecture.toml  clippy.toml  deny.toml  Cargo.toml               the rules
docs/adr/  .agents/skills/                                          decisions and agent skills
```

## License

MIT, Copyright (c) 2026 Luu Tien Duc. See [LICENSE](LICENSE).

## Credits

This base stands on public work. Material that is copied or adapted is listed in [NOTICE.md](NOTICE.md) with its license; this section thanks everyone whose work shaped a decision, whether or not any text was copied.

**Used**

- The Rust project: the language, cargo, rustfmt, clippy, and rustdoc.
- Crates we depend on: [serde](https://github.com/serde-rs/serde) (also the app's settings file), [thiserror](https://github.com/dtolnay/thiserror), [anyhow](https://github.com/dtolnay/anyhow), [tracing](https://github.com/tokio-rs/tracing), [jiff](https://github.com/BurntSushi/jiff), [clap](https://github.com/clap-rs/clap), [syn](https://github.com/dtolnay/syn), [cargo_metadata](https://github.com/oli-obk/cargo_metadata), [toml and toml_edit](https://github.com/toml-rs/toml) (`toml` also reads and writes the app's settings file), [walkdir](https://github.com/BurntSushi/walkdir), [serde_json](https://github.com/serde-rs/json), [embed-resource](https://github.com/nabijaczleweli/rust-embed-resource) (the exe icon), the [`windows`](https://github.com/microsoft/windows-rs) crate.
- [PawnIO](https://github.com/namazso/PawnIO) and [PawnIO.Modules](https://github.com/namazso/PawnIO.Modules): the driver the app talks to, and the LGPL-2.1 modules shipped beside it, never committed.
- The app icon (`services/monitor/api/assets/icon.ico`) is the owner's own work, made with Claude; it has no third-party source or license.
- Developer tools we run, not copy: [cargo-nextest](https://github.com/nextest-rs/nextest), [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) (Embark Studios), [cargo-shear](https://github.com/Boshen/cargo-shear), [typos](https://github.com/crate-ci/typos), [lefthook](https://github.com/evilmartians/lefthook) (Evil Martians), [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), [zizmor](https://github.com/zizmorcore/zizmor), [taiki-e/install-action](https://github.com/taiki-e/install-action), [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache).

**Referenced for design** (no code copied)

| Source                                                                                                                   | What shaped us                                                                                                        |
| ------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| [go-base](https://github.com/tienduc120/go-base)                                                                          | the whole shape: every rule proven by a fixture, one command surface, the agent guard, the layer rules                |
| [matklad&#39;s cargo xtask](https://github.com/matklad/cargo-xtask)                                                       | tooling written in Rust and run with `cargo xtask` instead of Make                                                  |
| [zed](https://github.com/zed-industries/zed)                                                                              | forbidden crate edges checked from the dependency graph; no `mod.rs` files                                          |
| [openai/codex](https://github.com/openai/codex)                                                                           | `unwrap_used`/`expect_used` at deny with test exemptions; cargo-deny `wrappers`; the clean-worktree check in CI |
| [astral-sh/uv](https://github.com/astral-sh/uv) and [ruff](https://github.com/astral-sh/ruff)                              | `CARGO_BUILD_WARNINGS=deny`; nextest timeouts; agent rules on panics, `#[expect]`, and exhaustive matches         |
| [rust-analyzer](https://github.com/rust-lang/rust-analyzer)                                                               | tidy checks as tests in xtask; new dependencies and public items as architectural changes                             |
| [Microsoft Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/)                                       | lint posture, `#[expect]` over `#[allow]`, errors per layer, Rust-shaped code from agents                         |
| [How To Code It: hexagonal architecture in Rust](https://www.howtocodeit.com/articles/master-hexagonal-architecture-rust) | ports as traits with explicit `Send` futures, generics over `dyn`                                                 |
| [AGENTS.md](https://agents.md)                                                                                            | the shared agent instruction format                                                                                   |
| [PresentMon](https://github.com/GameTechDev/PresentMon) (MIT)                                                             | counting presented frames from ETW                                                                                    |
| [spd5118 documentation](https://docs.kernel.org/hwmon/spd5118.html) (Linux kernel)                                        | the SPD5118 temperature register format                                                                               |
