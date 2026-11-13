# Overlay Monitor

A Windows app that draws a small hardware readout (FPS, CPU, GPU, and DDR5 load and temperature) over the desktop and borderless games. It replaces HWiNFO, RTSS, and RTSS's DesktopOverlayHost with one program.

It is built on a copy of the owner's strict `rust-base`, so its rules are checked by tools: lint, layer, manifest, and source rules, a fixture that proves every rule fires, an agent guard, and skills an agent follows.

Read [AGENTS.md](AGENTS.md) first, then [docs/adr/](docs/adr/README.md).

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
services/<name>/{domain,application,infrastructure,interfaces,api}   one bounded context, one crate per layer
services/monitor/{domain,application,infrastructure,interfaces,api}  the monitor service
platform/{task,http,clock,win32}                                    shared technical crates, no business logic; win32 holds all unsafe
xtask/                                                              gates, guard, generator; gate code in src/gates
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
- Crates we depend on: [tokio](https://github.com/tokio-rs/tokio), tokio-util, [axum](https://github.com/tokio-rs/axum), [tower](https://github.com/tower-rs/tower), [tower-http](https://github.com/tower-rs/tower-http), [serde](https://github.com/serde-rs/serde), [thiserror](https://github.com/dtolnay/thiserror), [anyhow](https://github.com/dtolnay/anyhow), [tracing](https://github.com/tokio-rs/tracing), [jiff](https://github.com/BurntSushi/jiff), [clap](https://github.com/clap-rs/clap), [syn](https://github.com/dtolnay/syn), [cargo_metadata](https://github.com/oli-obk/cargo_metadata), [toml and toml_edit](https://github.com/toml-rs/toml), [walkdir](https://github.com/BurntSushi/walkdir), [serde_json](https://github.com/serde-rs/json), [http-body-util](https://github.com/hyperium/http-body), the [`windows`](https://github.com/microsoft/windows-rs) crate.
- [PawnIO](https://github.com/namazso/PawnIO) and [PawnIO.Modules](https://github.com/namazso/PawnIO.Modules): the driver the app talks to, and the LGPL-2.1 modules shipped beside it, never committed.
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
